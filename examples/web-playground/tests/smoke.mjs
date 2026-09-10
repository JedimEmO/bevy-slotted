// Drives dist/web-playground/smoke.html in headless Chromium over the DevTools
// protocol and prints what the page decided.
//
//   node examples/web-playground/tests/smoke.mjs [--url <page>] [--shot <png>]
//   node examples/web-playground/tests/smoke.mjs --scene chest --shot chest.png
//   node examples/web-playground/tests/smoke.mjs --showcase --shots <dir>
//
// `--scene` opens one showcase scene through its `?scene=` link, asserts the
// head strip matches the scene table, and captures it. `--showcase` does that
// for all nine in one browser, and also switches to each one through the rail
// so both routes into a scene are covered.
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

// `?overlay` asks smoke.html to also show the in-canvas console once. Not on
// CI: under SwiftShader on a runner that one overlay costs the page tens of
// seconds a frame (see the note in smoke.html), and `--no-overlay` skips it
// anywhere else.
const overlay = !process.env.CI && !args.includes('--no-overlay');
const URL_ = opt(
  '--url',
  `http://127.0.0.1:8080/web-playground/smoke.html${overlay ? '?overlay' : ''}`,
);
const CONFINED = join(homedir(), 'snap/chromium/common');
const SHOT = opt('--shot', join(CONFINED, 'slotted-smoke.png'));
const PROFILE = join(CONFINED, 'slotted-smoke-profile');
// Inside the repo, so CI can upload it after a failure.
const ARTIFACTS = opt('--artifacts', 'dist/smoke');
const BROWSER = opt('--browser', '/snap/bin/chromium');
// Wide enough that the item browser's 352 px panel fits in the strip beside
// the chest; below about 1600 the panel docks but is squeezed to one column.
const SIZE = opt('--size', '1400,820');
// `--throttle 4` slows the page's main thread 4x through DevTools, to see
// what a check does on a runner far slower than the machine at hand.
const THROTTLE = Number(opt('--throttle', '1'));
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
    this.errors = [];
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(event.data);
      if (message.id && this.waiting.has(message.id)) {
        const { resolve, reject } = this.waiting.get(message.id);
        this.waiting.delete(message.id);
        message.error ? reject(new Error(JSON.stringify(message.error))) : resolve(message.result);
        return;
      }
      if (message.method === 'Runtime.consoleAPICalled') {
        const text = message.params.args.map((a) => a.value ?? a.description).join(' ');
        this.logs.push(text);
        if (message.params.type === 'error') this.errors.push(text);
      }
      if (message.method === 'Runtime.exceptionThrown') {
        // `.text` is the word "Uncaught" and nothing else. The description is
        // where a wasm trap's `RuntimeError: unreachable` and a panic hook's
        // message actually are, and without it a failure here says only that
        // something went wrong.
        const details = message.params.exceptionDetails;
        const description =
          details.exception?.description ?? details.exception?.value ?? details.text;
        const where = details.url ? ` (${details.url}:${details.lineNumber})` : '';
        const text = `EXCEPTION ${String(description).split('\n').slice(0, 3).join(' | ')}${where}`;
        this.logs.push(text);
        this.errors.push(text);
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

/**
 * The nine scenes, in rail order, with the titles the page must show.
 *
 * `examples/showcase/src/lib.rs` is the source of the copy; this list is the
 * independent copy that makes the assertion mean something. The caption and
 * the three "try" lines are checked against what the module handed the page,
 * because those are long and the risk worth testing is that the page renders
 * the wrong entry, not that someone retyped a sentence.
 */
const SCENES = [
  { id: 'chest', title: 'Chest' },
  { id: 'machine', title: 'Machine' },
  { id: 'menus', title: 'Menus' },
  { id: 'dialogue', title: 'Dialogue' },
  { id: 'themes', title: 'Themes' },
  { id: 'mods', title: 'Mods' },
  { id: 'hud', title: 'HUD' },
  { id: 'multiplayer', title: 'Multiplayer' },
  { id: 'testing', title: 'Testing' },
];

/**
 * Console errors that are not the page's fault and not a regression.
 *
 * `not yet:` is a scene whose Rust has not landed, which the contract says the
 * page renders as a disabled control rather than a failure. The WebGL lines
 * are SwiftShader telling us it is not a GPU.
 */
const IGNORE = /not yet:|WebGL|swiftshader|GroupMarkerNotSet|Automatic fallback to software/i;

/** Waits for the page to have a module and an editor, or gives up. */
async function ready(evaluate) {
  for (let attempt = 0; attempt < 160; attempt += 1) {
    if (await evaluate('!!(window.slottedPlayground && window.slottedPlayground.state.editor)')) {
      return true;
    }
    await sleep(500);
  }
  return false;
}

/** Asserts the head strip and the rail agree with the scene table. */
async function check(evaluate, id, how = 'link') {
  const seen = await evaluate(`(() => {
    const p = window.slottedPlayground;
    const table = p.state.scenes.find((s) => s.id === ${JSON.stringify(id)});
    const tries = [...document.querySelectorAll('#scene-tries li')].map((n) => n.textContent);
    const rail = document.querySelector('[data-scene="' + p.state.sceneId + '"]');
    return {
      active: p.state.sceneId,
      count: p.state.scenes.length,
      title: document.getElementById('scene-title').textContent,
      caption: document.getElementById('scene-caption').textContent,
      tries,
      wantTitle: table && table.title,
      wantCaption: table && table.caption,
      wantTries: table && table.tries,
      selected: rail ? rail.getAttribute('aria-selected') : null,
      controls: [...document.querySelectorAll('.side .ctl')]
        .filter((n) => !n.hidden)
        .map((n) => n.id),
    };
  })()`);

  const expected = SCENES.find((s) => s.id === id);
  const problems = [];
  if (seen.count !== SCENES.length) problems.push(`the table has ${seen.count} scenes, not ${SCENES.length}`);
  if (seen.active !== id) problems.push(`the page is on ${seen.active}`);
  if (seen.title !== expected.title) problems.push(`the head says "${seen.title}", not "${expected.title}"`);
  if (seen.wantCaption && seen.caption !== seen.wantCaption) problems.push('the caption is not the table\'s');
  if (seen.wantTries && seen.tries.join('|') !== seen.wantTries.join('|')) {
    problems.push('the three "what to try" lines are not the table\'s');
  }
  if (seen.tries.length !== 3) problems.push(`${seen.tries.length} things to try, not 3`);
  if (seen.selected !== 'true') problems.push('the rail entry is not selected');
  if (seen.controls.length !== 1) problems.push(`${seen.controls.length} control blocks are visible, not 1`);

  const ok = problems.length === 0;
  return {
    bad: ok ? 0 : 1,
    lines: [
      `${ok ? 'PASS' : 'FAIL'}  ${id} via the ${how}: head, rail and controls agree with the table` +
        (ok ? ` (${seen.controls[0] ?? 'none'})` : ''),
      ...problems.map((why) => `      ${why}`),
    ],
  };
}

// ---------------------------------------------------------------------------
// Driving a scene, rather than only looking at it
// ---------------------------------------------------------------------------
//
// `check` above asserts the page rendered the right copy for the scene. These
// press the scene's own controls and assert the game answered, which is the
// half a screenshot cannot show. Each returns `{bad, lines}` the same way.
//
// The three "what to try" items are the script: where one is scriptable from
// the page's control block it is scripted here, and where it is a gesture
// inside the canvas it is a synthetic pointer over the canvas.

/**
 * Polls `expression` until it is truthy, and answers whether it became so.
 *
 * A throw counts as "not yet", not as a failure: the page rebuilds the wasm
 * module after a trap (ADR 0004) and every export is briefly undefined while
 * it does, which a poll running across a restart would otherwise turn into an
 * exception that ends the whole run.
 */
async function waitFor(evaluate, expression, ms = 8000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    try {
      if (await evaluate(expression)) return true;
    } catch {
      // still coming up
    }
    await sleep(200);
  }
  return false;
}

/** `evaluate`, with a throw rendered as `fallback` rather than ending the run. */
async function tryEvaluate(evaluate, expression, fallback = null) {
  try {
    return await evaluate(expression);
  } catch (error) {
    return fallback ?? `threw: ${String(error).slice(0, 120)}`;
  }
}

/** The canvas in page coordinates. */
const canvasRect = (evaluate) =>
  evaluate(`(() => {
    const r = document.getElementById('slotted-canvas').getBoundingClientRect();
    return { x: r.x, y: r.y, w: r.width, h: r.height };
  })()`);

/** A point at fractions `(fx, fy)` of the canvas. */
async function canvasPoint(evaluate, fx, fy) {
  const r = await canvasRect(evaluate);
  return { x: Math.round(r.x + r.w * fx), y: Math.round(r.y + r.h * fy) };
}

async function mouse(cdp, sessionId, type, at, extra = {}) {
  await cdp.send(
    'Input.dispatchMouseEvent',
    { type, x: at.x, y: at.y, button: 'left', buttons: type === 'mouseReleased' ? 0 : 1, clickCount: 1, ...extra },
    sessionId,
  );
}

/** A press, a few moves and a release: a drag the game sees as a drag. */
async function drag(cdp, sessionId, from, to) {
  await mouse(cdp, sessionId, 'mouseMoved', from, { buttons: 0 });
  await sleep(120);
  await mouse(cdp, sessionId, 'mousePressed', from);
  await sleep(120);
  for (let step = 1; step <= 8; step += 1) {
    await mouse(cdp, sessionId, 'mouseMoved', {
      x: Math.round(from.x + ((to.x - from.x) * step) / 8),
      y: Math.round(from.y + ((to.y - from.y) * step) / 8),
    });
    await sleep(60);
  }
  await mouse(cdp, sessionId, 'mouseReleased', to);
  await sleep(400);
}

/**
 * One key, down and up, to whatever has the focus. Bevy hears the keyboard
 * through the canvas, so callers focus it first (`focusCanvas`).
 *
 * `windowsVirtualKeyCode` is what Chromium turns into the DOM `keyCode`, and
 * winit reads `code`; both are sent so the event is the one a real press is.
 */
const VIRTUAL_KEYS = { Enter: 13, Escape: 27, ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40, x: 88 };
async function key(cdp, sessionId, name, code = name) {
  const base = { key: name, code, windowsVirtualKeyCode: VIRTUAL_KEYS[name] ?? 0 };
  const text = name.length === 1 ? { text: name } : name === 'Enter' ? { text: '\r' } : {};
  await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', ...base, ...text }, sessionId);
  await sleep(40);
  await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', ...base }, sessionId);
  await sleep(250);
}

const focusCanvas = (evaluate) => evaluate("document.getElementById('slotted-canvas').focus()");

async function click(cdp, sessionId, at) {
  await mouse(cdp, sessionId, 'mouseMoved', at, { buttons: 0 });
  await sleep(120);
  await mouse(cdp, sessionId, 'mousePressed', at);
  await sleep(80);
  await mouse(cdp, sessionId, 'mouseReleased', at);
  await sleep(400);
}

/** One assertion, rendered the way `check` renders its own. */
function verdict(id, what, ok, detail = '') {
  return {
    bad: ok ? 0 : 1,
    lines: [`${ok ? 'PASS' : 'FAIL'}  ${id}: ${what}`, ...(ok || !detail ? [] : [`      ${detail}`])],
  };
}

/** An assertion that could not run, which is not a failure. */
function skip(id, what, why) {
  return { bad: 0, lines: [`SKIP  ${id}: ${what}`, `      ${why}`] };
}

/**
 * Whether the raw export `name` is still the skeleton's stub, and if so what
 * it said. The page's own wrappers swallow `not yet:` and grey a control
 * (docs/design/showcase-refresh-contract.md section 9), which is right for
 * a visitor and useless here, so the probe goes past them to the module.
 *
 * Answers '' for an export that ran, `not yet: ...` for a stub, `missing`
 * for a name the module has no export for, and any other error's text.
 */
async function stubbed(evaluate, name, ...callArgs) {
  return evaluate(`(() => {
    const w = window.slottedPlayground.state.wasm;
    if (!w || typeof w[${JSON.stringify(name)}] !== 'function') return 'missing';
    try { w[${JSON.stringify(name)}](...${JSON.stringify(callArgs)}); return ''; }
    catch (e) { return String(e && e.message ? e.message : e); }
  })()`);
}

/** A `stubbed` answer as a verdict: `not yet:` and a missing export skip, anything else fails. */
function stubOrFail(id, what, name, said) {
  return /not yet:/.test(said) || said === 'missing'
    ? skip(id, what, `the module said "${said}"`)
    : verdict(id, what, false, `${name} threw "${said}"`);
}

/** The id of the screen on top of the stack, or null when the module has no `current_screen`. */
async function topScreen(evaluate) {
  const top = await evaluate(`(() => {
    const w = window.slottedPlayground.state.wasm;
    return w && typeof w.current_screen === 'function' ? w.current_screen() : null;
  })()`);
  return typeof top === 'string' ? top : null;
}

/** A scene visit may take this long before the driver gives up on a wait. */
const SCENE_WAIT_MS = 30_000;

/**
 * Puts a scene into the pose its screenshot wants, before the shot is taken.
 *
 * The contract wants the Menus picture to be the pause over the chest and the
 * Dialogue picture to be the choice node, neither of which is what `enter`
 * leaves on screen. `drive` starts from the pose, so what happened here is
 * handed on. A stub export answers `not yet:` and the pose does nothing.
 */
async function pose(cdp, sessionId, rawEvaluate, id) {
  const evaluate = (expression) => tryEvaluate(rawEvaluate, expression);
  const posed = { id, stub: '', played: null, paused: null, smith: false, prompt: false };

  if (id === 'menus') {
    posed.stub = await stubbed(evaluate, 'settings_ron');
    if (posed.stub) return posed;
    // Play is the title's first button and holds the focus ring when the
    // screen opens, so Enter is a click on it.
    await focusCanvas(evaluate);
    await key(cdp, sessionId, 'Enter');
    posed.played = await waitFor(evaluate, `(() => {
      const w = window.slottedPlayground.state.wasm;
      return typeof w.current_screen === 'function' ? w.current_screen() === 'demo:chest' : null;
    })()`, 15_000);
    if (posed.played === false && (await topScreen(evaluate)) === null) await sleep(2500);
    // Esc pops the chest; Esc with nothing open pauses.
    await key(cdp, sessionId, 'Escape');
    await key(cdp, sessionId, 'Escape');
    posed.paused = await waitFor(evaluate, `(() => {
      const w = window.slottedPlayground.state.wasm;
      return typeof w.current_screen === 'function' && w.current_screen() === 'slotted:pause';
    })()`, 8000);
    if (!posed.paused) {
      // The page's own way in, which is the picture even when the keyboard
      // route did not get there; `drive` says which route it was.
      await evaluate("window.slottedPlayground.menu_open('pause')");
    }
    await sleep(2000);
  }

  if (id === 'dialogue') {
    // Probed through `set_value` with the switch's starting value rather than
    // `talk_again`, which would answer the running conversation with a line.
    posed.stub = await stubbed(evaluate, 'set_value', 'demo.found_key', 'false');
    if (posed.stub) return posed;
    // The first line types out; the transcript pane gets it as it is entered.
    posed.smith = await waitFor(evaluate, "/smith/i.test(document.getElementById('dialogue-log').textContent)", SCENE_WAIT_MS);
    // Enter skips the typing, then advances. The choice node is the second
    // smith line in the transcript; press until it shows, a few times at most.
    await focusCanvas(evaluate);
    for (let press = 0; press < 4 && !posed.prompt; press += 1) {
      await key(cdp, sessionId, 'Enter');
      posed.prompt = await waitFor(evaluate, "document.querySelectorAll('#dialogue-log .row').length >= 2", 2500);
    }
    await sleep(2000);
  }

  return posed;
}

/**
 * Presses the scene's controls and asserts the game answered.
 *
 * A scene with nothing scriptable answers with no assertions rather than a
 * failure; the page-side check has already run by the time this is called.
 */
async function drive(cdp, sessionId, rawEvaluate, id, posed = {}) {
  const evaluate = (expression) => tryEvaluate(rawEvaluate, expression);
  const out = [];
  const add = (v) => out.push(v);

  // A restart mid-scene is a trapped module, which is a failure of the scene
  // and not of the assertion that happened to be running when it hit.
  const restartsBefore = (await evaluate('window.slottedPlayground.state.restarts')) ?? 0;

  if (id === 'chest') {
    // The sweep, which is the scene's second "what to try": hold the button
    // down across empty slots and let go. The assertion is that the world
    // still adds up, because a sweep that loses a stack is the bug worth
    // catching and the phantom preview is only visible mid-gesture.
    const before = await evaluate('window.slottedPlayground.snapshot_state()');
    const from = await canvasPoint(evaluate, 0.3, 0.28);
    const to = await canvasPoint(evaluate, 0.42, 0.42);
    await drag(cdp, sessionId, from, to);
    const after = await evaluate('window.slottedPlayground.snapshot_state()');
    const count = (text) => (String(text).match(/count:/g) ?? []).length;
    add(
      verdict(
        id,
        'a sweep across the chest leaves the snapshot readable and non-empty',
        typeof after === 'string' && after.includes('inventories:') && count(after) > 0,
        `${count(before)} stacks before, ${count(after)} after`,
      ),
    );
  }

  if (id === 'chest') {
    // The chips type a whole query into the game's own search field.
    await evaluate("window.slottedPlayground.browser_search('#c:ingots')");
    await sleep(1200);
    const complained = await evaluate(
      "document.getElementById('console').textContent.includes('no browser is docked')",
    );
    add(
      verdict(
        id,
        'a search chip reaches a docked browser rather than falling on the floor',
        !complained,
        'the module logged "no browser is docked", so no panel was attached to this scene',
      ),
    );
  }

  if (id === 'machine') {
    await evaluate("document.getElementById('redstone-on').click()");
    await sleep(800);
    const on = await evaluate(
      "document.getElementById('redstone-on').getAttribute('aria-pressed')",
    );
    await evaluate("document.getElementById('redstone-off').click()");
    await sleep(600);
    const off = await evaluate(
      "document.getElementById('redstone-off').getAttribute('aria-pressed')",
    );
    add(
      verdict(id, 'the redstone toggle answers in both directions', on === 'true' && off === 'true', `on=${on} off=${off}`),
    );
  }

  if (id === 'menus') {
    // Everything here goes through what the page can see: the stack's top
    // through `current_screen`, the control block's readout, `settings_ron`,
    // and `localStorage`. A stub export is the Rust not having landed, which
    // the contract makes a skip and not a failure.
    if (posed.stub) {
      // A stub or an absent export is the Rust not having landed; any other
      // throw is the export refusing, which is a failure.
      add(stubOrFail(id, 'the menus are driven', 'settings_ron', posed.stub));
    } else {
      const top = await topScreen(evaluate);
      if (top === null) {
        add(skip(id, 'Play then Esc twice leaves the pause on top', 'the module has no current_screen export to read the stack with'));
      } else {
        add(
          verdict(
            id,
            'Play then Esc twice leaves the pause on top',
            posed.played === true && posed.paused === true,
            `after Play the top was ${posed.played ? 'demo:chest' : 'not demo:chest'}; after Esc, Esc it is ${top}` +
              (posed.paused ? '' : ' and the pose fell back to menu_open'),
          ),
        );
      }

      // Settings from the page, then the first slider with the keyboard. The
      // display tab opens first with a select on its first row and the UI
      // scale slider on the second; each arrow that lands on either writes a
      // save, and the assertion is that one reached the page.
      await evaluate("document.getElementById('menu-settings').click()");
      const settings = await waitFor(evaluate, `(() => {
        const w = window.slottedPlayground.state.wasm;
        return typeof w.current_screen === 'function' ? w.current_screen() === 'demo:settings' : true;
      })()`, 10_000);
      add(verdict(id, 'the Settings button opens demo:settings', settings, `the top is ${await topScreen(evaluate)}`));
      await focusCanvas(evaluate);
      for (const name of ['ArrowDown', 'ArrowRight', 'ArrowRight', 'ArrowDown', 'ArrowRight']) {
        await key(cdp, sessionId, name);
      }
      const saved = await waitFor(evaluate, "window.slottedPlayground.settings_ron().length > 0", 10_000);
      const before = String(await evaluate('window.slottedPlayground.settings_ron()'));
      add(verdict(id, 'moving a control with the keyboard writes a save the page can read', saved, `settings_ron is ${before.length} bytes`));

      // The page copies the save into localStorage on its one-second poll,
      // and says so in the readout.
      const stored = await waitFor(evaluate, `localStorage.getItem('slotted.settings') === ${JSON.stringify(before)}`, 5000);
      const readout = await evaluate("document.getElementById('settings-stored').textContent");
      add(verdict(id, 'the save reaches localStorage and the readout says how big it is', stored && /bytes/.test(readout), `readout: ${readout}`));

      // The third "what to try", done for real: reload, and read it back.
      await cdp.send('Page.navigate', { url: await evaluate('location.href') }, sessionId);
      await ready(evaluate);
      const cameBack = await waitFor(evaluate, `window.slottedPlayground.settings_ron() === ${JSON.stringify(before)}`, SCENE_WAIT_MS);
      const after = String(await evaluate('window.slottedPlayground.settings_ron()'));
      add(
        verdict(
          id,
          'after a reload settings_ron reads back what was saved',
          saved && cameBack,
          `${before.length} bytes before, ${after.length} after: ${after.slice(0, 120)}`,
        ),
      );
      await sleep(2000);

      // Reset clears the key and the store, and the page says so.
      await evaluate("document.getElementById('settings-reset').click()");
      const cleared = await waitFor(evaluate, `(() => {
        const gone = localStorage.getItem('slotted.settings') === null;
        return gone && /nothing saved/.test(document.getElementById('settings-stored').textContent);
      })()`, 5000);
      add(verdict(id, 'Reset forgets the saved settings', cleared, `readout: ${await evaluate("document.getElementById('settings-stored').textContent")}`));
    }
  }

  if (id === 'dialogue') {
    if (posed.stub) {
      // A stub or an absent export is the Rust not having landed; any other
      // throw is the export refusing, which is a failure.
      add(stubOrFail(id, 'the dialogue is driven', 'set_value', posed.stub));
    } else {
      const lines = () => evaluate("[...document.querySelectorAll('#dialogue-log .row')].map((r) => r.textContent)");
      add(verdict(id, "the smith's first line reaches the transcript pane", posed.smith, `transcript: ${JSON.stringify(await lines())}`));
      add(
        verdict(
          id,
          'Enter skips the typing and Enter again reaches the prompt',
          posed.prompt,
          `the transcript holds ${(await lines()).length} line(s): ${JSON.stringify(await lines())}`,
        ),
      );

      // Finish this conversation first, because Talk again is a no-op while
      // one runs: Enter takes the focused first answer, then two more skip
      // and advance the smith's reply to the end, which hands the focus back
      // to the furnace.
      await focusCanvas(evaluate);
      await key(cdp, sessionId, 'Enter');
      await sleep(600);
      await key(cdp, sessionId, 'Enter');
      await sleep(600);
      await key(cdp, sessionId, 'Enter');
      const ended = await waitFor(evaluate, `(() => {
        const w = window.slottedPlayground.state.wasm;
        return typeof w.current_screen === 'function' ? w.current_screen() === 'machine:furnace' : true;
      })()`, 10_000);
      add(verdict(id, 'answering and advancing to the end leaves the furnace on top', ended, `the top is ${await topScreen(evaluate)}; transcript: ${JSON.stringify(await lines())}`));

      // The switch writes `demo.found_key` through `set_value`; a stub would
      // have greyed it, which the page records in `state.notYet`.
      await evaluate("document.getElementById('dialogue-key').click()");
      await sleep(600);
      const on = await evaluate("document.getElementById('dialogue-key').checked && !window.slottedPlayground.state.notYet.has('set_value')");
      add(verdict(id, 'the "found the key" switch turns on and set_value took the value', on));

      // Talk again, walk to the choice, and take the gated answer: the third
      // option is the one `demo.found_key` enables, and the focus ring skips
      // a disabled option, so landing on it is the assertion.
      const countBefore = (await lines()).length;
      await evaluate("document.getElementById('dialogue-again').click()");
      const again = await waitFor(evaluate, `document.querySelectorAll('#dialogue-log .row').length > ${countBefore}`, 15_000);
      add(verdict(id, 'Talk again starts the conversation over', again, `${countBefore} lines before, ${(await lines()).length} after`));
      await focusCanvas(evaluate);
      let atChoice = false;
      for (let press = 0; press < 4 && !atChoice; press += 1) {
        await key(cdp, sessionId, 'Enter');
        atChoice = await waitFor(evaluate, `document.querySelectorAll('#dialogue-log .row').length >= ${countBefore + 2}`, 2500);
      }
      await sleep(800);
      await key(cdp, sessionId, 'ArrowDown');
      await key(cdp, sessionId, 'ArrowDown');
      const countAtChoice = (await lines()).length;
      await key(cdp, sessionId, 'Enter');
      // "I found the key" is the gated option's text (examples/showcase/locale),
      // and the transcript's `you:` line is what was chosen.
      const chose = await waitFor(evaluate, `document.querySelectorAll('#dialogue-log .row').length > ${countAtChoice} && /you.*found the key/i.test([...document.querySelectorAll('#dialogue-log .row')].slice(${countAtChoice}).map((r) => r.textContent).join(' '))`, 10_000);
      add(
        verdict(
          id,
          'walking down twice and pressing Enter picks the third, gated option',
          atChoice && chose,
          `transcript: ${JSON.stringify((await lines()).slice(countAtChoice))}`,
        ),
      );
    }
  }

  if (id === 'themes') {
    // A theme swap has to reach the DOM: the button that is pressed and the
    // token table's active column both follow the module, not the click.
    const before = await evaluate("document.getElementById('theme-diff').textContent");
    await evaluate("document.querySelector('#theme-buttons [data-theme=\"neon\"]').click()");
    const swapped = await waitFor(
      evaluate,
      "document.querySelector('#theme-buttons [data-theme=\"neon\"]').getAttribute('aria-pressed') === 'true'",
    );
    const table = await evaluate("document.getElementById('theme-diff').textContent");
    add(verdict(id, 'switching to neon marks the neon button and keeps the token table', swapped && table.length > 0, `pressed=${swapped}`));
    add(
      verdict(
        id,
        'the token table names a value that differs between the skins',
        /Glass/.test(before) && /CutCorners/.test(table + before),
        'the panel-role row is the one row that is different in all three',
      ),
    );
    // Put it back, so the scenes after this one are shot in the theme the
    // rest of the run used.
    await evaluate("document.querySelector('#theme-buttons [data-theme=\"glass\"]').click()");
    await sleep(800);
  }

  if (id === 'hud') {
    // Reset first. The page keeps the layout in `localStorage` and restores it
    // on the way into this scene, so a profile that has run this before opens
    // with the hotbar wherever the last run dragged it -- and a drag aimed at
    // the bottom edge then grabs nothing. `restore_hud_layout('')` is Reset:
    // every layer back on its own definition's anchor.
    await evaluate("window.slottedPlayground.restore_hud_layout('')");
    await sleep(1200);
    const stored = await evaluate('window.slottedPlayground.hud_layout()');
    await evaluate("document.getElementById('hud-edit').click()");
    const editing = await waitFor(
      evaluate,
      "document.getElementById('hud-edit').getAttribute('aria-pressed') === 'true'",
    );
    add(verdict(id, 'the edit toggle turns edit mode on', editing));

    // The hotbar sits on the bottom edge, centred, when nothing has moved it.
    const from = await canvasPoint(evaluate, 0.5, 0.93);
    const to = await canvasPoint(evaluate, 0.25, 0.35);
    await drag(cdp, sessionId, from, to);
    await sleep(800);
    const moved = await evaluate("window.slottedPlayground.hud_layout()");
    add(
      verdict(
        id,
        'dragging a layer in edit mode changes the stored layout',
        typeof moved === 'string' && moved.length > 0 && moved !== stored,
        `${String(stored).length} bytes before, ${String(moved).length} after`,
      ),
    );

    // And the page can put it back, which is what the Reset button and the
    // `localStorage` round trip both rely on.
    await evaluate(`window.slottedPlayground.restore_hud_layout(${JSON.stringify(String(stored))})`);
    await sleep(1000);
    const back = await evaluate("window.slottedPlayground.hud_layout()");
    add(
      verdict(id, 'restore_hud_layout puts the layout back where it was', back === stored, `wanted ${String(stored).length} bytes, got ${String(back).length}`),
    );
    await evaluate("document.getElementById('hud-edit').click()");
    await sleep(400);
  }

  if (id === 'multiplayer') {
    // A lossy link, then a click on the left client. The message log is the
    // scene's whole point: the client's guess and the server's answer.
    await evaluate(`(() => {
      const l = document.getElementById('net-latency');
      l.value = '200';
      l.dispatchEvent(new Event('input', { bubbles: true }));
      l.dispatchEvent(new Event('change', { bubbles: true }));
      const p = document.getElementById('net-loss');
      p.value = '30';
      p.dispatchEvent(new Event('input', { bubbles: true }));
      p.dispatchEvent(new Event('change', { bubbles: true }));
    })()`);
    await sleep(600);
    const readout = await evaluate(
      "document.getElementById('net-latency-out').textContent + ' / ' + document.getElementById('net-loss-out').textContent",
    );
    add(verdict(id, 'the latency and loss sliders read back what was set', /200/.test(readout) && /30/.test(readout), readout));

    // A stack on the left client, picked up and put down in an empty slot.
    // Both halves go through the client's prediction and the server's answer,
    // which is what the message log is showing.
    const before = await evaluate("document.querySelectorAll('#net-log > *').length");
    await click(cdp, sessionId, await canvasPoint(evaluate, 0.123, 0.427));
    await click(cdp, sessionId, await canvasPoint(evaluate, 0.164, 0.282));
    const grew = await waitFor(
      evaluate,
      `document.querySelectorAll('#net-log > *').length > ${before}`,
      10000,
    );
    const after = await evaluate("document.querySelectorAll('#net-log > *').length");
    add(
      verdict(
        id,
        'a click on a client puts client and server messages in the log',
        grew,
        `the log had ${before} lines and has ${after}`,
      ),
    );
  }

  if (id === 'testing') {
    // The mods' own Lua tests, run against the live game.
    await evaluate(`(() => {
      const b = [...document.querySelectorAll('#test-mods button')][0];
      if (b) b.click();
    })()`);
    const passed = await waitFor(
      evaluate,
      "/\\bok\\b|passed|✓/i.test(document.getElementById('test-results').textContent + document.getElementById('console').textContent)",
      20000,
    );
    const results = await evaluate("document.getElementById('test-results').textContent");
    add(verdict(id, "the sorter's Lua tests run and report a pass", passed, results.slice(0, 200)));

    // The recording: load it, scrub to the last frame, and let the walk
    // finish. `replay_status().playing` is true while a seek is in flight.
    await evaluate("document.getElementById('replay-load').click()");
    const loaded = await waitFor(
      evaluate,
      "(() => { const s = window.slottedPlayground.replay_status(); return s && JSON.parse(s).frames > 0; })()",
      15000,
    );
    const frames = loaded
      ? await evaluate('JSON.parse(window.slottedPlayground.replay_status()).frames')
      : 0;
    add(verdict(id, 'the bundled recording loads and reports its frame count', loaded, `frames=${frames}`));

    if (loaded) {
      const last = frames - 1;
      // Wait for the page, not just the module. `replay_load` is a request the
      // world applies next frame, so the click's own `replay_status` still
      // reads zero frames and leaves the scrubber's `max` at 0; the poll a
      // second later is what widens it. A value set before that is clamped to
      // 0 by the browser, and the seek that follows goes nowhere.
      const ready = await waitFor(
        evaluate,
        `document.getElementById('replay-seek').max === '${last}'`,
        10000,
      );
      if (!ready) {
        add(
          verdict(
            id,
            "the scrubber's range grew to fit the recording",
            false,
            `max is ${await evaluate("document.getElementById('replay-seek').max")}, wanted ${last}`,
          ),
        );
      }
      await evaluate(`(() => {
        const s = document.getElementById('replay-seek');
        s.value = '${last}';
        s.dispatchEvent(new Event('input', { bubbles: true }));
        s.dispatchEvent(new Event('change', { bubbles: true }));
      })()`);
      const arrived = await waitFor(
        evaluate,
        `(() => {
          const s = JSON.parse(window.slottedPlayground.replay_status());
          return s.frame === ${last} && !s.playing;
        })()`,
        20000,
      );
      // The scrubber follows on the page's own poll, not on the seek, so it
      // trails the module by up to a second. Wait for it rather than reading
      // it the instant the module says it has arrived.
      const caughtUp = await waitFor(
        evaluate,
        `document.getElementById('replay-seek').value === '${last}'`,
        5000,
      );
      const status = await evaluate('window.slottedPlayground.replay_status()');
      const scrubber = await evaluate("document.getElementById('replay-seek').value");
      add(
        verdict(
          id,
          'seeking to the last frame arrives, and the scrubber agrees',
          arrived && caughtUp,
          `status=${status} scrubber=${scrubber} wanted ${last}`,
        ),
      );
      const chest = await evaluate('window.slottedPlayground.snapshot_state()');
      add(
        verdict(
          id,
          'the replayed chest is still a chest at the end of the recording',
          typeof chest === 'string' && chest.includes('inventories:'),
          String(chest).slice(0, 120),
        ),
      );
    }
  }

  const restartsAfter = (await evaluate('window.slottedPlayground.state.restarts')) ?? 0;
  const trapped = restartsAfter !== restartsBefore;
  // The page keeps the last thing the runtime said before the trap, and its
  // console pane has the restart line. Without them a failure here says only
  // that the module died somewhere in the scene.
  const why = trapped
    ? `${await evaluate('window.slottedPlayground.state.luaError')} | ` +
      `${String(await evaluate("document.getElementById('console').textContent")).slice(-400)}`
    : '';
  add(
    verdict(
      id,
      'driving the scene did not trap the wasm module',
      !trapped,
      `the page restarted the module ${restartsAfter - restartsBefore} time(s) while this scene was driven: ${why}`,
    ),
  );

  return {
    bad: out.reduce((sum, v) => sum + v.bad, 0),
    lines: out.flatMap((v) => v.lines),
  };
}

/**
 * Every scene twice, then a look at whether anything was left behind.
 *
 * `tests/showcase.rs` asserts this in a world it can see into; here the only
 * window on it is the snapshot the world publishes, which grows with the
 * inventories that exist. A cycle that leaks a scene's menus makes it grow
 * without bound, so the assertion is that the second lap ends where the first
 * one did.
 */
async function cycleTwice(evaluate) {
  const sizes = [];
  for (let lap = 0; lap < 2; lap += 1) {
    for (const scene of SCENES) {
      await evaluate(`document.querySelector('[data-scene="${scene.id}"]').click()`);
      await sleep(900);
    }
    await evaluate(`document.querySelector('[data-scene="chest"]').click()`);
    await sleep(1500);
    sizes.push(String((await evaluate('window.slottedPlayground.snapshot_state()')) ?? '').length);
  }
  const [first, second] = sizes;
  const ok = first > 0 && second > 0 && Math.abs(second - first) <= first * 0.05;
  return {
    bad: ok ? 0 : 1,
    lines: [
      `${ok ? 'PASS' : 'FAIL'}  two laps of all nine scenes leave the world the size it was`,
      ...(ok ? [] : [`      the snapshot was ${first} bytes after one lap and ${second} after two`]),
    ],
  };
}

async function main() {
  const cdp = await open(await endpoint());
  const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
  await cdp.send('Runtime.enable', {}, sessionId);
  await cdp.send('Page.enable', {}, sessionId);
  if (THROTTLE > 1) {
    await cdp.send('Emulation.setCPUThrottlingRate', { rate: THROTTLE }, sessionId);
  }
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

  // The showcase modes. `--scene <id>` is one scene; `--showcase` is all
  // nine in one browser, which is what `just shot-showcase` runs.
  const one = opt('--scene', null);
  const all = args.includes('--showcase');
  if (one || all) {
    const shots = opt('--shots', CONFINED);
    mkdirSync(shots, { recursive: true });

    // A clean page first. The browser profile outlives the run, and the HUD
    // scene keeps the layout a visitor dragged in `localStorage` and restores
    // it on the way in -- so without this the second run of the day opens the
    // HUD scene with the hotbar wherever the first run's drag left it, and
    // the screenshot the docs embed is of a hotbar halfway up the canvas.
    await cdp.send('Page.navigate', { url: new URL('index.html', URL_).href }, sessionId);
    await ready(evaluate);
    try {
      await evaluate('localStorage.clear()');
    } catch {
      // A profile with site data switched off has nothing to clear.
    }

    // A scene the module reports as not ready is a stub the page greys out
    // (docs/design/showcase-refresh-contract.md); the lap says so and moves on
    // rather than photographing an empty canvas for the docs.
    const stubs = new Set(
      (await evaluate('window.slottedPlayground.state.scenes.filter((s) => !s.ready).map((s) => s.id)')) ?? [],
    );
    const wanted = (all ? SCENES.map((s) => s.id) : [one]).filter((id) => {
      if (!stubs.has(id)) return true;
      console.log(`SKIP  ${id}: the module says the scene is a stub`);
      return false;
    });
    const lines = [];
    let bad = 0;

    for (const [index, id] of wanted.entries()) {
      const page = new URL(`index.html?scene=${id}`, URL_).href;
      await cdp.send('Page.navigate', { url: page }, sessionId);
      await ready(evaluate);
      // The canvas draws the scene over several frames, and the item browser
      // docks a frame after the screen opens.
      await sleep(4000);
      const deep = await check(evaluate, id);
      lines.push(...deep.lines);
      bad += deep.bad;

      // The pose the docs want, before the picture is taken. Not skipped by
      // `--no-drive`, because the pose is part of the picture.
      const posed = await pose(cdp, sessionId, evaluate, id);

      // The shot first, then the drive. These screenshots are what the README
      // and the guide embed, and a chest caught mid-sweep or a HUD caught
      // mid-drag is not the picture the docs want; the drive is what proves
      // the scene works, and it can have the canvas afterwards.
      const shot = opt('--shot', join(shots, `showcase-${id}.png`));
      const png = await cdp.send('Page.captureScreenshot', { format: 'png' }, sessionId);
      writeFileSync(shot, Buffer.from(png.data, 'base64'));
      lines.push(`      shot: ${shot}`);

      // The scene's own controls, pressed. `--no-drive` skips them, for a run
      // that only wants the pictures.
      if (!args.includes('--no-drive')) {
        const driven = await drive(cdp, sessionId, evaluate, id, posed);
        lines.push(...driven.lines);
        bad += driven.bad;
      }

      // The other way in: press the next scene's rail entry and check the
      // head followed. Over the whole loop every scene is reached both ways.
      if (all) {
        const next = wanted[(index + 1) % wanted.length];
        await evaluate(`document.querySelector('[data-scene="${next}"]').click()`);
        await sleep(1200);
        const rail = await check(evaluate, next, 'rail');
        lines.push(...rail.lines);
        bad += rail.bad;
      }
    }

    // Last, because it walks the rail sixteen more times and ends on Chest.
    if (all && !args.includes('--no-drive')) {
      const cycled = await cycleTwice(evaluate);
      lines.push(...cycled.lines);
      bad += cycled.bad;
    }

    const errors = cdp.errors.filter((text) => !IGNORE.test(text));
    lines.push(
      `${errors.length === 0 ? 'PASS' : 'FAIL'}  no unexpected console errors ` +
        `(${cdp.errors.length - errors.length} were the expected "not yet:")`,
    );
    if (errors.length) lines.push(...errors.slice(0, 10).map((text) => `      ${text}`));
    bad += errors.length;

    console.log(lines.join('\n'));
    chrome.kill();
    process.exit(bad === 0 ? 0 : 1);
  }

  // `--wait <ms>` is the screenshot-only mode: no assertions, just let the
  // page settle and capture it.
  const wait = opt('--wait', null);
  let title = '';
  if (wait) {
    await sleep(Number(wait));
  } else {
    // smoke.html's own ceilings add up to about 100 s on a machine where
    // every wait runs to its limit; this is the driver giving up after that.
    const deadline = Date.now() + 150_000;
    while (Date.now() < deadline) {
      title = (await evaluate('document.title')) ?? '';
      if (title === 'smoke: pass' || title === 'smoke: fail') break;
      await sleep(500);
    }
  }

  const report = wait
    ? `captured ${URL_} after ${wait} ms`
    : ((await evaluate("document.getElementById('out').textContent")) ?? '(no output)');

  const probe = async (expr) => {
    try {
      const value = await evaluate(expr);
      return value === undefined || value === null ? '(none)' : String(value);
    } catch (error) {
      return `(unreadable: ${error})`;
    }
  };
  // How far smoke.html got and how long each stage took, read before the
  // page is navigated away from. Printed on every run, because a pass on a
  // fast machine and a fail on a slow runner differ in the timings, not in
  // the checks.
  const smokeTimeline = wait
    ? ''
    : [
        `last stage reached: ${await probe('window.__smokeStage')}`,
        `stage timeline: ${await probe('(window.__smokeStages || []).join(" -")')}`,
      ].join('\n');

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

    // -----------------------------------------------------------------
    // And once from the Multiplayer scene, which is the one scene whose
    // snapshot is not the open menu's. Its state lives in a server the
    // scene builds, so a restart there has to rebuild the server, put the
    // containers back and tell both clients; a restore that took the first
    // open menu would put one client's prediction back instead.
    // -----------------------------------------------------------------
    //
    // On a fresh page, not on this one. The restart above spent two of the
    // three the spiral guard allows in twenty seconds (the first replays the
    // visitor's text, which crashes again), so a third here would be refused
    // and the page would rightly declare itself dead. A visitor arriving at
    // `?scene=multiplayer` is also the case worth covering.
    await cdp.send(
      'Page.navigate',
      { url: new URL('index.html?scene=multiplayer', URL_).href },
      sessionId,
    );
    await ready(evaluate);
    await sleep(6000);
    const netBefore = (await evaluate('window.slottedPlayground.snapshot_state()')) ?? '';
    const restartsBefore = (await evaluate('window.slottedPlayground.state.restarts')) ?? 0;
    await evaluate(`(() => {
      const p = window.slottedPlayground;
      p.state.modId = 'copper_chest';
      p.state.file = 'control.lua';
      p.state.editor.setValue(${JSON.stringify('error("boom in multiplayer")\n')});
      p.run();
    })()`);

    let netRestarts = restartsBefore;
    for (let attempt = 0; attempt < 120; attempt += 1) {
      netRestarts = (await evaluate('window.slottedPlayground.state.restarts')) ?? 0;
      if (netRestarts > restartsBefore) break;
      await sleep(250);
    }
    // The module comes back, the scene is re-entered from the snapshot's own
    // `scene` field, and the server is rebuilt before the restore lands.
    await sleep(6000);
    const scene = (await evaluate('window.slottedPlayground.current_scene()')) ?? '';
    const netAfter = (await evaluate('window.slottedPlayground.snapshot_state()')) ?? '';
    const carriedNet = netBefore.includes('net:Some(') && netAfter.includes('net:Some(');
    const cameBack = netRestarts > restartsBefore && scene === 'multiplayer' && carriedNet;
    pageResult +=
      `\n${cameBack ? 'PASS' : 'FAIL'}  a crash in the Multiplayer scene restarts into the ` +
      `Multiplayer scene with the server's state\n      restarts=${netRestarts} scene=${scene} ` +
      `server-state-before=${netBefore.includes('net:Some(')} after=${netAfter.includes('net:Some(')}`;
    if (!cameBack) title = 'smoke: fail';
    await sleep(1000);
  }
  const shot = await cdp.send('Page.captureScreenshot', { format: 'png' }, sessionId);
  writeFileSync(SHOT, Buffer.from(shot.data, 'base64'));
  // A copy inside the workspace as well, because CI's artifact upload cannot
  // reach the browser's confined home directory and a failing run is exactly
  // when the picture is worth having.
  let workspaceShot = '';
  try {
    mkdirSync(ARTIFACTS, { recursive: true });
    workspaceShot = join(ARTIFACTS, 'smoke.png');
    writeFileSync(workspaceShot, Buffer.from(shot.data, 'base64'));
  } catch {
    workspaceShot = '';
  }

  const failed = !wait && title !== 'smoke: pass';
  console.log(report);
  if (pageResult) console.log(`\n${pageResult}`);
  // On wasm a Lua error aborts the module (ADR 0004), and the smoke page has
  // no restart machinery, so every check after a trap fails with nothing to
  // say. The timeline answers the question the check list cannot: how far
  // the page got and where the time went. The index.html block says whether
  // the real page's module was still alive at the end.
  if (smokeTimeline) console.log(`\n--- smoke.html ---\n${smokeTimeline}`);
  if (failed && pageResult) {
    const diagnosis = [
      `title=${title || '(never set)'}`,
      `playground module present: ${await probe('!!window.slottedPlayground')}`,
      `restarts: ${await probe('window.slottedPlayground && window.slottedPlayground.state.restarts')}`,
      `last lua error: ${await probe('window.slottedPlayground && window.slottedPlayground.state.luaError')}`,
      `wasm heap MiB: ${await probe('Math.round((performance.memory ? performance.memory.usedJSHeapSize : 0) / 1048576)')}`,
    ];
    console.log(`\n--- index.html ---\n${diagnosis.join('\n')}`);
  }
  if (cdp.logs.length) console.log(`\n--- browser console ---\n${cdp.logs.join('\n')}`);
  console.log(`\nscreenshot: ${SHOT}`);
  if (workspaceShot) console.log(`screenshot (workspace copy): ${workspaceShot}`);
  chrome.kill();
  process.exit(failed ? 1 : 0);
}

main().catch((error) => {
  console.error(error);
  chrome.kill();
  process.exit(2);
});
