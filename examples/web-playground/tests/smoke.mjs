// Drives dist/web-playground/smoke.html in headless Chromium over the DevTools
// protocol and prints what the page decided.
//
//   node examples/web-playground/tests/smoke.mjs [--url <page>] [--shot <png>]
//
// No dependencies: Node 22's global WebSocket is the whole CDP client. The
// browser is snap-confined and may only write under ~/snap/chromium/common, so
// both the profile and the screenshot go there.

import { spawn } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};

const URL_ = opt('--url', 'http://127.0.0.1:8080/web-playground/smoke.html');
const CONFINED = join(homedir(), 'snap/chromium/common');
const SHOT = opt('--shot', join(CONFINED, 'slotted-smoke.png'));
const PROFILE = join(CONFINED, 'slotted-smoke-profile');
const BROWSER = opt('--browser', '/snap/bin/chromium');
// Wide enough that the item browser's 352 px panel fits in the strip beside
// the chest; below about 1600 the panel docks but is squeezed to one column.
const SIZE = opt('--size', '1400,820');
const PORT = 9333;

mkdirSync(PROFILE, { recursive: true });

const chrome = spawn(BROWSER, [
  '--headless=new',
  `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${PROFILE}`,
  `--window-size=${SIZE}`,
  // SwiftShader, because there is no GPU here and bevy needs a WebGL2 context.
  '--use-angle=swiftshader',
  '--enable-unsafe-swiftshader',
  '--no-first-run',
  '--no-default-browser-check',
  // CI runners and containers have no user namespaces for Chrome's sandbox;
  // without this flag Chrome exits before opening the debugging port.
  ...(process.env.CI ? ['--no-sandbox', '--disable-dev-shm-usage'] : []),
  'about:blank',
]);
chrome.on('exit', (code, signal) => {
  if (code !== 0 && code !== null) process.stderr.write(`chrome exited early: code ${code} signal ${signal}\n`);
});
chrome.stderr.on('data', (chunk) => {
  const text = String(chunk);
  if (/ERROR|FATAL/.test(text)) process.stderr.write(`chrome: ${text}`);
});

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function endpoint() {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try {
      const response = await fetch(`http://127.0.0.1:${PORT}/json/version`);
      return (await response.json()).webSocketDebuggerUrl;
    } catch {
      await sleep(250);
    }
  }
  throw new Error('Chromium never opened its debugging port');
}

class Cdp {
  constructor(socket) {
    this.socket = socket;
    this.next = 1;
    this.waiting = new Map();
    this.logs = [];
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(event.data);
      if (message.id && this.waiting.has(message.id)) {
        const { resolve, reject } = this.waiting.get(message.id);
        this.waiting.delete(message.id);
        message.error ? reject(new Error(JSON.stringify(message.error))) : resolve(message.result);
        return;
      }
      if (message.method === 'Runtime.consoleAPICalled') {
        this.logs.push(message.params.args.map((a) => a.value ?? a.description).join(' '));
      }
      if (message.method === 'Runtime.exceptionThrown') {
        this.logs.push(`EXCEPTION ${message.params.exceptionDetails.text}`);
      }
    });
  }

  send(method, params = {}, sessionId) {
    const id = this.next++;
    return new Promise((resolve, reject) => {
      this.waiting.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params, sessionId }));
    });
  }
}

async function open(url) {
  const socket = new WebSocket(url);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });
  return new Cdp(socket);
}

async function main() {
  const cdp = await open(await endpoint());
  const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
  await cdp.send('Runtime.enable', {}, sessionId);
  await cdp.send('Page.enable', {}, sessionId);
  await cdp.send('Page.navigate', { url: URL_ }, sessionId);

  const evaluate = async (expression) => {
    const result = await cdp.send(
      'Runtime.evaluate',
      { expression, returnByValue: true, awaitPromise: false },
      sessionId,
    );
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.exception?.description ?? 'evaluate threw');
    }
    return result.result?.value;
  };

  // `--wait <ms>` is the screenshot-only mode: no assertions, just let the
  // page settle and capture it.
  const wait = opt('--wait', null);
  let title = '';
  if (wait) {
    await sleep(Number(wait));
  } else {
    const deadline = Date.now() + 120_000;
    while (Date.now() < deadline) {
      title = (await evaluate('document.title')) ?? '';
      if (title === 'smoke: pass' || title === 'smoke: fail') break;
      await sleep(500);
    }
  }

  const report = wait
    ? `captured ${URL_} after ${wait} ms`
    : ((await evaluate("document.getElementById('out').textContent")) ?? '(no output)');

  // Phase two: the real page, driven the way a person drives it. The editor's
  // buffer is replaced and Run is pressed; the assertion is that the line the
  // edited chunk logs turns up in the page's own console pane.
  let pageResult = '';
  if (!wait && title === 'smoke: pass') {
    const page = new URL('index.html', URL_).href;
    await cdp.send('Page.navigate', { url: page }, sessionId);
    // The page boots the module, then loads the editor from a CDN. Wait for
    // the editor rather than for a guessed number of seconds.
    for (let attempt = 0; attempt < 120; attempt += 1) {
      if (await evaluate('!!(window.slottedPlayground && window.slottedPlayground.state.editor)')) {
        break;
      }
      await sleep(500);
    }
    await sleep(1500);
    const marker = 'smoke: the page pressed Run';
    // Exactly what a person does: pick the mod, pick the file, replace the
    // text in the editor, press Run. `run()` reads the editor, not the map.
    const edited = `slotted.info("${marker}")\n`;
    await evaluate(`(() => {
      const p = window.slottedPlayground;
      p.state.modId = 'copper_chest';
      p.state.file = 'control.lua';
      p.state.editor.setValue(${JSON.stringify(edited)});
      p.run();
    })()`);
    for (let attempt = 0; attempt < 40; attempt += 1) {
      const text = (await evaluate("document.getElementById('console').textContent")) ?? '';
      if (text.includes(marker)) break;
      await sleep(250);
    }
    const text = (await evaluate("document.getElementById('console').textContent")) ?? '';
    const ok = text.includes(marker);
    pageResult = `${ok ? 'PASS' : 'FAIL'}  the page's Run button reloaded the mod and showed its line\n      ${text.slice(-220)}`;
    if (!ok) title = 'smoke: fail';
    await sleep(1500);

    // ---------------------------------------------------------------------
    // The restart. On wasm a raised Lua error aborts the whole module
    // (ADR 0004), so the page's answer is to build another one and put the
    // chest back. This is the only place that whole path can be exercised:
    // it needs a real trap in a real tab.
    // ---------------------------------------------------------------------
    const before = (await evaluate('window.slottedPlayground.snapshot_state()')) ?? '';
    await evaluate(`(() => {
      const p = window.slottedPlayground;
      p.state.modId = 'copper_chest';
      p.state.file = 'control.lua';
      p.state.editor.setValue(${JSON.stringify('error("boom")\n')});
      p.run();
    })()`);

    let restarts = 0;
    for (let attempt = 0; attempt < 120; attempt += 1) {
      restarts = (await evaluate('window.slottedPlayground.state.restarts')) ?? 0;
      if (restarts > 0) break;
      await sleep(250);
    }
    // Let the second module boot and take the restored state back.
    await sleep(4000);
    const console_ = (await evaluate("document.getElementById('console').textContent")) ?? '';
    const said = console_.includes('the mod crashed the Lua runtime; restarting');
    const after = (await evaluate('window.slottedPlayground.snapshot_state()')) ?? '';
    const kept = before.length > 0 && after === before;
    const restarted = restarts > 0 && said && kept;
    pageResult +=
      `\n${restarted ? 'PASS' : 'FAIL'}  an error() in control.lua restarted the runtime ` +
      `with the chest intact\n      restarts=${restarts} reported=${said} chest-kept=${kept}` +
      `\n      before: ${before.slice(0, 160)}\n      after:  ${after.slice(0, 160)}`;
    if (!restarted) title = 'smoke: fail';
    await sleep(1000);
  }
  const shot = await cdp.send('Page.captureScreenshot', { format: 'png' }, sessionId);
  writeFileSync(SHOT, Buffer.from(shot.data, 'base64'));

  console.log(report);
  if (pageResult) console.log(`\n${pageResult}`);
  if (cdp.logs.length) console.log(`\n--- browser console ---\n${cdp.logs.join('\n')}`);
  console.log(`\nscreenshot: ${SHOT}`);
  chrome.kill();
  process.exit(wait || title === 'smoke: pass' ? 0 : 1);
}

main().catch((error) => {
  console.error(error);
  chrome.kill();
  process.exit(2);
});
