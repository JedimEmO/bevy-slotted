// The page half of the playground. Everything that touches the game goes
// through the four wasm exports; nothing here reaches into Bevy.
//
// The editor is CodeMirror 6, loaded as ES modules from a CDN. If the CDN is
// blocked the page detects it and falls back to a plain <textarea>, because a
// playground that will not open at all on a locked-down network is worse than
// one without syntax colours.

import init, {
  list_mods,
  get_mod_file,
  reload_mod,
  run_tests,
  drain_console,
  console_history,
} from './web_playground.js';

// esm.sh rather than cdnjs or jsDelivr, and the reason is not taste:
// CodeMirror 6 is half a dozen packages that must share one copy of
// @codemirror/state, and a `+esm` bundle inlines its own, which makes every
// `instanceof` check fail ("Unrecognized extension value"). esm.sh resolves a
// shared dependency to one URL, so the copies are one copy.
const CM = 'https://esm.sh';
const IDLE_MS = 800;
/** The mod whose screen the canvas opens on, and the file worth showing. */
const OPENING_MOD = 'copper_chest';
const OPENING_FILE = 'control.lua';
const MAX_ROWS = 400;

const el = (id) => document.getElementById(id);
const statusEl = el('status');
const consoleEl = el('console');

/** What the editor is showing, and every buffer the user has touched. */
const state = {
  mods: [],
  modId: null,
  file: null,
  buffers: new Map(), // "mod/file" -> text
  original: new Map(), // "mod/file" -> the bundled text
  editor: null, // { getValue, setValue, focus }
  view: 'editor', // 'editor' or 'tests'

  idle: null,
  // Filling the editor is a document change too, and an unguarded idle timer
  // would reload a mod every time the user switched tabs.
  quiet: false,
};

/** Replaces the editor's text without arming the idle run. */
function setText(text) {
  state.quiet = true;
  state.editor.setValue(text);
  state.quiet = false;
}

const key = (modId, file) => `${modId}/${file}`;

function setStatus(text, kind = '') {
  statusEl.textContent = text;
  statusEl.className = `status ${kind}`;
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

/** CodeMirror 6, or `null` when the CDN is unreachable. */
async function loadCodeMirror() {
  const timeout = new Promise((_, reject) =>
    setTimeout(() => reject(new Error('CDN timed out')), 6000),
  );
  const modules = Promise.all([
    import(`${CM}/codemirror@6.0.1`),
    import(`${CM}/@codemirror/language@6.10.2`),
    import(`${CM}/@codemirror/legacy-modes@6.4.1/mode/lua`),
    import(`${CM}/@codemirror/theme-one-dark@6.1.2`),
    import(`${CM}/@codemirror/view@6.28.6`),
  ]);
  return Promise.race([modules, timeout]);
}

async function makeEditor(host, initial, onChange) {
  try {
    const [core, language, lua, dark, view] = await loadCodeMirror();
    const listener = view.EditorView.updateListener.of((update) => {
      if (update.docChanged) onChange();
    });
    const editor = new view.EditorView({
      doc: initial,
      parent: host,
      extensions: [
        core.basicSetup,
        language.StreamLanguage.define(lua.lua),
        dark.oneDark,
        listener,
        // Wrapped, because the pane is half a laptop wide and the demo mods
        // have comment lines longer than that: without it the first thing a
        // visitor sees is a horizontal scrollbar over a cut-off sentence.
        view.EditorView.lineWrapping,
        view.EditorView.theme({ '&': { height: '100%' } }),
      ],
    });
    return {
      kind: 'codemirror',
      getValue: () => editor.state.doc.toString(),
      setValue: (text) =>
        editor.dispatch({
          changes: { from: 0, to: editor.state.doc.length, insert: text },
        }),
      focus: () => editor.focus(),
    };
  } catch (error) {
    console.warn('CodeMirror is unavailable, falling back to a textarea:', error);
    const area = document.createElement('textarea');
    area.className = 'fallback';
    area.spellcheck = false;
    area.value = initial;
    area.addEventListener('input', onChange);
    host.appendChild(area);
    return {
      kind: 'textarea',
      getValue: () => area.value,
      setValue: (text) => {
        area.value = text;
      },
      focus: () => area.focus(),
    };
  }
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

function renderTabs() {
  const tabs = el('file-tabs');
  tabs.replaceChildren();
  const mod = state.mods.find((m) => m.id === state.modId);
  if (!mod) return;
  for (const file of mod.files) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = file.name;
    button.setAttribute('aria-selected', String(state.view === 'editor' && file.name === state.file));
    const buffer = state.buffers.get(key(mod.id, file.name));
    if (buffer !== undefined && buffer !== state.original.get(key(mod.id, file.name))) {
      button.classList.add('dirty');
    }
    button.addEventListener('click', () => selectFile(file.name));
    tabs.appendChild(button);
  }
  // The Tests tab sits beside the files: same mod, a different thing to do
  // with it. Results arrive on the console as `[test]` lines.
  const tests = document.createElement('button');
  tests.type = 'button';
  tests.textContent = 'Tests';
  tests.setAttribute('aria-selected', String(state.view === 'tests'));
  tests.addEventListener('click', showTests);
  tabs.appendChild(tests);
}

/** Shows the editor or the tests pane, whichever `state.view` names. */
function renderView() {
  el('editor-host').hidden = state.view !== 'editor';
  el('tests-host').hidden = state.view !== 'tests';
  if (state.view === 'tests') renderTests();
  renderTabs();
}

function showTests() {
  stash();
  state.view = 'tests';
  renderView();
}

/** The selected mod's bundled tests, and the button that runs them. */
function renderTests() {
  const host = el('tests-host');
  host.replaceChildren();
  const mod = state.mods.find((m) => m.id === state.modId);
  const names = mod?.tests ?? [];
  const blurb = document.createElement('p');
  if (names.length === 0) {
    blurb.textContent = `${state.modId ?? 'this mod'} bundles no tests/*.lua.`;
    host.appendChild(blurb);
    return;
  }
  blurb.textContent =
    'These run against the running game, one action per frame, and report on the console.';
  const list = document.createElement('ul');
  for (const name of names) {
    const item = document.createElement('li');
    item.textContent = `tests/${name}`;
    list.appendChild(item);
  }
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'primary';
  button.textContent = 'Run tests';
  button.addEventListener('click', () => {
    setStatus(`running ${state.modId} tests…`, 'busy');
    run_tests(state.modId);
  });
  host.append(blurb, list, button);
}

function stash() {
  if (state.modId && state.file && state.editor) {
    state.buffers.set(key(state.modId, state.file), state.editor.getValue());
  }
}

/**
 * The bundled text for a file, or '' when the bundle has no such mod or file.
 *
 * `get_mod_file` throws on an unknown name rather than returning '', so that
 * the caller can tell an empty file from a wrong one. Every caller here is
 * happy with '', because the name may have come out of a stale share link.
 */
function bundledText(modId, file) {
  try {
    return get_mod_file(modId, file);
  } catch (error) {
    console.warn(`no bundled ${modId}/${file}:`, error);
    return '';
  }
}

function textFor(modId, file) {
  const id = key(modId, file);
  if (!state.buffers.has(id)) {
    const bundled = bundledText(modId, file);
    state.original.set(id, bundled);
    state.buffers.set(id, bundled);
  }
  if (!state.original.has(id)) state.original.set(id, bundledText(modId, file));
  return state.buffers.get(id);
}

function selectFile(file) {
  stash();
  state.view = 'editor';
  state.file = file;
  setText(textFor(state.modId, file));
  renderView();
}

function selectMod(modId, preferred = null) {
  stash();
  state.modId = modId;
  const mod = state.mods.find((m) => m.id === modId);
  const wanted = mod?.files.find((f) => f.name === preferred);
  state.file = (wanted ?? mod?.files[0])?.name ?? null;
  setText(state.file ? textFor(modId, state.file) : '');
  renderView();
}

// ---------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------

function run() {
  stash();
  const modId = state.modId;
  if (!modId) return;
  const data = state.buffers.get(key(modId, 'data.lua')) ?? '';
  const control = state.buffers.get(key(modId, 'control.lua')) ?? '';
  setStatus(`reloading ${modId}…`, 'busy');
  reload_mod(modId, data, control);
  writeHash();
  // The world answers on its next frame; the console poll picks up whatever it
  // says, and an error there turns the status red.
  setTimeout(() => {
    if (statusEl.classList.contains('busy')) setStatus(`${modId} reloaded`, 'ok');
  }, 400);
}

function scheduleIdleRun() {
  if (state.quiet) return;
  renderTabs();
  if (!el('auto').checked) return;
  clearTimeout(state.idle);
  state.idle = setTimeout(run, IDLE_MS);
}

// ---------------------------------------------------------------------------
// Console
// ---------------------------------------------------------------------------

function appendLines(lines) {
  for (const line of lines) {
    const row = document.createElement('div');
    row.className = 'row';
    const who = document.createElement('span');
    who.className = 'who';
    who.textContent = `[${line.who}]`;
    const text = document.createElement('span');
    text.className = line.level;
    text.textContent = line.text;
    row.append(who, text);
    consoleEl.appendChild(row);
    if (line.level === 'error') setStatus(line.text.slice(0, 80), 'bad');
  }
  while (consoleEl.childElementCount > MAX_ROWS) consoleEl.firstElementChild.remove();
  consoleEl.scrollTop = consoleEl.scrollHeight;
}

function pollConsole() {
  try {
    const lines = JSON.parse(drain_console());
    if (lines.length) appendLines(lines);
  } catch (error) {
    console.error('draining the console:', error);
  }
  requestAnimationFrame(pollConsole);
}

// ---------------------------------------------------------------------------
// Sharing
// ---------------------------------------------------------------------------

function writeHash() {
  const edited = {};
  for (const [id, text] of state.buffers) {
    if (text !== state.original.get(id)) edited[id] = text;
  }
  if (Object.keys(edited).length === 0) {
    history.replaceState(null, '', location.pathname);
    return;
  }
  const packed = btoa(String.fromCharCode(...new TextEncoder().encode(JSON.stringify(edited))));
  history.replaceState(null, '', `#${packed}`);
}

function readHash() {
  if (!location.hash.length) return;
  try {
    const json = new TextDecoder().decode(
      Uint8Array.from(atob(location.hash.slice(1)), (c) => c.charCodeAt(0)),
    );
    for (const [id, text] of Object.entries(JSON.parse(json))) {
      const [modId, file] = [id.slice(0, id.indexOf('/')), id.slice(id.indexOf('/') + 1)];
      state.original.set(id, bundledText(modId, file));
      state.buffers.set(id, text);
    }
  } catch (error) {
    console.warn('the link carried something unreadable:', error);
  }
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

async function main() {
  setStatus('compiling wasm…', 'busy');
  try {
    await init();
  } catch (error) {
    // Bevy's winit loop on the web unwinds out of `start` by design; anything
    // else is a real failure and belongs on screen.
    if (!String(error).includes('control flow')) {
      setStatus('the module failed to start', 'bad');
      console.error(error);
    }
  }
  el('boot').classList.add('gone');
  setStatus('running', 'ok');

  state.mods = JSON.parse(list_mods());
  readHash();

  const host = el('editor-host');
  state.editor = await makeEditor(host, '', scheduleIdleRun);

  const select = el('mod-select');
  for (const mod of state.mods) {
    const option = document.createElement('option');
    option.value = mod.id;
    option.textContent = mod.id;
    select.appendChild(option);
  }
  select.addEventListener('change', () => selectMod(select.value));
  // The mod whose screen is the one on the canvas, and its control.lua: the
  // file where an edit changes something the visitor can see happen.
  const opening = state.mods.find((m) => m.id === OPENING_MOD) ?? state.mods[0];
  selectMod(opening?.id ?? null, OPENING_FILE);
  select.value = state.modId ?? '';

  el('run').addEventListener('click', run);
  el('clear').addEventListener('click', () => consoleEl.replaceChildren());
  el('reset').addEventListener('click', () => {
    const id = key(state.modId, state.file);
    state.buffers.set(id, state.original.get(id) ?? bundledText(state.modId, state.file));
    setText(state.buffers.get(id));
    renderTabs();
    writeHash();
  });
  el('share').addEventListener('click', async () => {
    stash();
    writeHash();
    try {
      await navigator.clipboard.writeText(location.href);
      setStatus('link copied', 'ok');
    } catch {
      setStatus(location.href, '');
    }
  });
  document.addEventListener('keydown', (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
      event.preventDefault();
      run();
    }
  });

  // The ring holds everything the world said while the module was booting;
  // show that, then drop the pending copies of the same lines so the poll
  // below does not print them twice.
  appendLines(JSON.parse(console_history()));
  drain_console();
  requestAnimationFrame(pollConsole);
}

// The smoke test drives these, and so does anyone poking at the page in a
// devtools console.
window.slottedPlayground = {
  run,
  state,
  reload_mod,
  run_tests,
  list_mods,
  get_mod_file,
  drain_console,
  console_history,
};

main();
