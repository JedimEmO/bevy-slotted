import { THEME_DIFF } from './themes.js';

// The page half of the showcase. Everything that touches the game goes
// through the wasm exports; nothing here reaches into Bevy.
//
// The editor is CodeMirror 6, loaded as ES modules from a CDN. If the CDN is
// blocked the page detects it and falls back to a plain <textarea>, because a
// playground that will not open at all on a locked-down network is worse than
// one without syntax colours.

// The module is imported by URL rather than by a static `import`, because the
// page has to be able to instantiate it more than once. On wasm32 a Lua error
// raised by a mod is a trap that kills the module (ADR 0004), and the answer
// is to build a new one; a static import would hand back the same dead
// instance every time. A cache-busting query gives each restart a fresh module
// scope and a fresh wasm instance.
const GLUE = './web_playground.js';

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
/** How often the page asks the world for a snapshot, at most. */
const SNAPSHOT_EVERY_MS = 1000;
/** Restarts allowed inside SPIRAL_MS before the page stops trying. */
const MAX_RESTARTS = 3;
const SPIRAL_MS = 20_000;
/** The prefix `bridge.rs` puts on the Lua error it reports before a trap. */
const LUA_ERROR_PREFIX = 'slotted-lua-error: ';
/** What every stubbed export throws (`bridge::NOT_YET_PREFIX`). */
const NOT_YET_PREFIX = 'not yet: ';

/**
 * The rail before the module is up, and the rail if it never comes up: the
 * same eight entries `list_scenes` returns, copy and all
 * (`examples/showcase/src/lib.rs` is the source; keep the two in step).
 */
const FALLBACK_SCENES = [
  { id: 'chest', title: 'Chest', caption: 'The Minecraft interaction model with a modern skin. Seven click modes, a sweep, a phantom preview and a tooltip, all over one list of slots.', tries: ['Left-click a stack, then right-click to split it', 'Hold right and drag across empty slots', 'Hover an item and hold Shift'], ready: true },
  { id: 'browser', title: 'Browser', caption: 'Every item and recipe in the game, docked beside any screen. Search has a grammar, and a recipe knows whether it can be transferred.', tries: ['Type #ingots, then -iron', 'Press R over a card', 'Press A to bookmark it'], ready: true },
  { id: 'machine', title: 'Machine', caption: 'A furnace with a tank, an energy bar and two side tabs, driven by menu properties the simulation writes. The Sort button was injected by a mod that has never seen this screen.', tries: ['Put coal in the fuel slot and watch the arrow', 'Open the redstone tab', 'Press Sort'], ready: true },
  { id: 'themes', title: 'Themes', caption: 'One screen tree, three skins. A theme is a RON file of tokens and materials, and swapping it repaints the open screen in place.', tries: ['Switch to paper', 'Switch to neon', 'Open the Machine scene and switch again'], ready: true },
  { id: 'mods', title: 'Mods', caption: 'Four Lua mods, editable here, hot-reloaded into the running game. The chest keeps its contents across a reload, and a crash restarts the runtime.', tries: ['Edit control.lua and press Run', 'Open Tests and run them', 'Type error("boom") and watch the restart'], ready: true },
  { id: 'hud', title: 'HUD', caption: 'Layers anchored to the screen edges, registered from Rust or from a mod, and a position editor a player can use.', tries: ['Press the edit button and drag the hotbar', 'Drag the clock', 'Reload the page and find them where you left them'], ready: true },
  { id: 'multiplayer', title: 'Multiplayer', caption: 'Two clients and a server in this tab, joined by a lossy loopback link. The left client predicts; the server corrects; both share one chest.', tries: ['Click a stack on the left and watch the right', 'Raise the loss slider and click again', 'Read the message log'], ready: true },
  { id: 'testing', title: 'Testing', caption: "The mods' tests/*.lua run against the live game, one action per frame, and a recorded session replays through the real input path.", tries: ["Run the sorter's tests", 'Scrub the recording', 'Press play'], ready: true },
];
/** The scene the page opens on when the URL names none. */
const DEFAULT_SCENE = 'chest';

/** The three bundled themes, in the order the buttons show them. */
const THEMES = THEME_DIFF.themes;
/** Where the HUD scene keeps the layout a visitor dragged into place. */
const HUD_KEY = 'slotted.showcase.hud';
/** How often the HUD and Testing scenes ask the world where things are. */
const POLL_MS = 1000;

/**
 * The Browser scene's chips. Each one is a whole query, because the grammar's
 * point is that the parts combine: a tag narrows, a minus excludes, a bare
 * word matches the name.
 */
const BROWSER_EXAMPLES = [
  ['#ingots', 'everything tagged as an ingot'],
  ['#ingots -iron', 'the same, without iron'],
  ['#c:ingots', 'the cross-mod tag, so other mods\u2019 ingots come too'],
  ['copper', 'a plain name match'],
  ['@copper_chest', 'everything one mod owns'],
  ['#tools #c:ingots', 'two tags, both required'],
];

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

  // The showcase: the scene table and which scene the canvas shows.
  scenes: FALLBACK_SCENES,
  sceneId: DEFAULT_SCENE,
  theme: THEMES[0],
  // Exports that answered `not yet`. A name in here is never called again
  // this session, so a stub cannot fill the console with the same complaint
  // once a frame.
  notYet: new Set(),
  // The per-scene poll (HUD layout, replay position), cleared on the way out.
  scenePoll: null,
  replay: { frame: 0, frames: 0, playing: false },

  idle: null,
  // Filling the editor is a document change too, and an unguarded idle timer
  // would reload a mod every time the user switched tabs.
  quiet: false,

  // The module, and what it takes to build another one.
  wasm: null, // the current module namespace
  restarts: 0,
  restartTimes: [], // when each restart happened, for the spiral guard
  restarting: false,
  // True between `state.wasm = module` and the end of `module.default()`.
  // The exports exist on the namespace in that window but the wasm instance
  // behind them does not, so calling one throws a TypeError out of the glue.
  booting: false,
  dead: false, // gave up: the page is showing a corpse
  snapshot: '', // the last state the world published
  nextSnapshot: 0, // Date.now() before which we do not ask again
  luaError: '', // the last thing the runtime said before a trap
};

// ---------------------------------------------------------------------------
// The module, and surviving its death
// ---------------------------------------------------------------------------

// The runtime reports a mod's Lua error through `console.error` at the moment
// it is raised, which on wasm is the last instruction before the trap. Keeping
// the text here is what lets the restart say *what* crashed rather than only
// that something did.
const realConsoleError = console.error.bind(console);
console.error = (...args) => {
  const text = args.map((a) => String(a)).join(' ');
  if (text.startsWith(LUA_ERROR_PREFIX)) state.luaError = text.slice(LUA_ERROR_PREFIX.length);
  realConsoleError(...args);
};

/**
 * Whether `error` is the module trapping rather than a plain JS exception.
 *
 * `WebAssembly.RuntimeError` covers it in every browser that follows the
 * spec; the string check is for the ones that report an unreachable as a
 * bare `Error`, and for the `unreachable executed` wrappers wasm-bindgen puts
 * around a panic.
 */
function isTrap(error) {
  if (typeof WebAssembly !== 'undefined' && error instanceof WebAssembly.RuntimeError) return true;
  return /RuntimeError|unreachable|out of bounds|table index/i.test(String(error));
}

/**
 * Calls an export by name. Every call into the module goes through here, so
 * that a trap becomes a restart instead of a dead page.
 *
 * Returns `fallback` when the module is not up, or when the call trapped.
 */
function call(name, args = [], fallback = undefined) {
  if (!state.wasm || state.dead || state.booting) return fallback;
  try {
    return state.wasm[name](...args);
  } catch (error) {
    if (!isTrap(error)) throw error;
    void restart(error);
    return fallback;
  }
}

/** Instantiates the module, wiring `state.wasm` to whatever came back. */
// The game turns off Bevy's default-event suppression so the editor keeps the
// keyboard, which also leaves the browser's context menu on the canvas. Right
// click is an inventory action, so swallow it there and only there. Delegated
// at the document so it survives the canvas swap a restart does.
document.addEventListener('contextmenu', (event) => {
  if (event.target && event.target.id === 'slotted-canvas') event.preventDefault();
});

async function boot() {
  const url = state.restarts === 0 ? GLUE : `${GLUE}?restart=${state.restarts}`;
  const module = await import(url);
  // Before `default()`, not after: Bevy's winit loop unwinds out of it by
  // design and never returns, so the exports have to be reachable already.
  //
  // Which leaves a window: the names are on the namespace but the instance
  // behind them is not built yet, and calling one throws a TypeError out of
  // the glue rather than anything the page can recognise. The page's own
  // one-second polls run through a restart and land in exactly that window,
  // so `booting` closes it: an export asked for too early answers with its
  // fallback, the same as one that is not built at all.
  state.booting = true;
  state.wasm = module;
  try {
    await module.default();
  } catch (error) {
    // Bevy's winit loop on the web unwinds out of `start` by design; anything
    // else is a real failure and belongs on screen.
    if (!String(error).includes('control flow')) throw error;
  } finally {
    state.booting = false;
  }
}

/**
 * The module trapped. Say so, build another one, and put the chest back.
 *
 * The editor's current text goes into the new module, because that is what the
 * visitor believes is running. If the same text traps again immediately the
 * spiral guard stops after `MAX_RESTARTS` and the page says what to do.
 */
async function restart(error) {
  if (state.restarting || state.dead) return;
  state.restarting = true;

  const now = Date.now();
  state.restartTimes = state.restartTimes.filter((at) => now - at < SPIRAL_MS);
  state.restartTimes.push(now);

  const why = state.luaError ? `: ${state.luaError}` : '';
  appendLines([
    { level: 'error', who: 'playground', text: `the mod crashed the Lua runtime; restarting${why}` },
  ]);
  console.warn('the wasm module trapped, restarting:', error);

  if (state.restartTimes.length > MAX_RESTARTS) {
    state.dead = true;
    state.restarting = false;
    setStatus('the mod keeps crashing the runtime; edit it and reload the page', 'bad');
    appendLines([
      {
        level: 'error',
        who: 'playground',
        text:
          `restarted ${MAX_RESTARTS} times in ${SPIRAL_MS / 1000}s and it crashed again. ` +
          'Fix the script and reload the page.',
      },
    ]);
    return;
  }

  setStatus('restarting the runtime…', 'busy');

  // A fresh canvas. The trapped module still owns the old one's WebGL context
  // and its event listeners, and a clone carries neither.
  const canvas = el('slotted-canvas');
  if (canvas) {
    const fresh = canvas.cloneNode(false);
    canvas.replaceWith(fresh);
  }

  state.restarts += 1;
  state.luaError = '';
  try {
    await boot();
  } catch (bootError) {
    state.dead = true;
    state.restarting = false;
    setStatus('the runtime could not be restarted', 'bad');
    console.error('restarting the module:', bootError);
    return;
  }

  if (state.snapshot) call('restore_state', [state.snapshot]);
  state.restarting = false;

  // The visitor's text, not the bundle's: the editor is what they think is
  // running, and a restart that silently reverted it would be a lie.
  //
  // Once, though. A chunk that crashes while it is *loading* crashes again the
  // moment it is put back, and a page that keeps doing that never comes up at
  // all. The second time round the new module keeps the bundled scripts, which
  // are known to work, and the editor keeps the text so Run is still one
  // keystroke away.
  if (state.restartTimes.length === 1) {
    run({ quiet: true });
  } else {
    appendLines([
      {
        level: 'warn',
        who: 'playground',
        text:
          'that script crashed the runtime again, so this restart kept the bundled ' +
          'scripts. Your edit is still in the editor; fix it and press Run.',
      },
    ]);
  }
  setStatus('runtime restarted', 'ok');
}

/**
 * Asks the world for the open menu's contents, at most every
 * `SNAPSHOT_EVERY_MS`. This is the value handed back after a restart.
 */
function takeSnapshot(force = false) {
  const now = Date.now();
  if (!force && now < state.nextSnapshot) return;
  state.nextSnapshot = now + SNAPSHOT_EVERY_MS;
  const text = call('snapshot_state', [], '');
  if (typeof text === 'string' && text.length > 0) state.snapshot = text;
}

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
    call('run_tests', [state.modId]);
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
    return call('get_mod_file', [modId, file], '');
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

/**
 * Sends the editor's current text to the world and reloads the mod.
 *
 * `quiet` is the restart's own call: the mod is being put back into a module
 * the visitor did not ask for, so the status line stays as the restart left
 * it. A snapshot is taken first either way, because a reload is the moment
 * the chest is most likely to be about to change.
 */
function run({ quiet = false } = {}) {
  stash();
  const modId = state.modId;
  if (!modId) return;
  const data = state.buffers.get(key(modId, 'data.lua')) ?? '';
  const control = state.buffers.get(key(modId, 'control.lua')) ?? '';
  if (!quiet) setStatus(`reloading ${modId}…`, 'busy');
  takeSnapshot(true);
  call('reload_mod', [modId, data, control]);
  writeHash();
  // The world answers on its next frame; the console poll picks up whatever it
  // says, and an error there turns the status red.
  setTimeout(() => {
    takeSnapshot(true);
    if (!quiet && statusEl.classList.contains('busy')) setStatus(`${modId} reloaded`, 'ok');
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

/**
 * The console pane, plus the two panes that take lines off it.
 *
 * `who: "net"` is the Multiplayer scene's message log, which the contract
 * keeps out of the console because the two are read for different reasons.
 * `who: "test"` is mirrored rather than moved: the Testing scene wants the
 * pass marks, and every other scene still wants the text.
 */
function appendLines(lines) {
  for (const line of lines) {
    if (line.who === 'net') {
      appendNet(line);
      continue;
    }
    if (line.who === 'test') appendTestLine(line);
    if (state.sceneId === 'browser' && /transfer/i.test(line.text)) {
      el('browser-readout').textContent = line.text;
    }
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
    const drained = call('drain_console', [], '[]');
    const lines = JSON.parse(drained ?? '[]');
    if (lines.length) appendLines(lines);
    // After the batch, not before: a line the world just wrote may be the one
    // that says the chest changed.
    takeSnapshot();
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
// Scenes
// ---------------------------------------------------------------------------

/** Whether `error` is a stubbed export saying so, rather than a failure. */
function isNotYet(error) {
  const text = String(error?.message ?? error);
  // Two shapes mean the same thing while package A is landing scene by scene:
  // an export that exists and refuses (`not yet: ...`), and one that is not in
  // the module at all, which JavaScript reports as "is not a function".
  return text.includes(NOT_YET_PREFIX) || /is not a function/.test(text);
}

/** The control panel for each scene, and the element its note lives in. */
const PANELS = {
  chest: 'controls-chest',
  browser: 'controls-browser',
  machine: 'controls-machine',
  themes: 'controls-themes',
  mods: 'controls-mods',
  hud: 'controls-hud',
  multiplayer: 'controls-multiplayer',
  testing: 'controls-testing',
};
const NOTES = {
  browser: 'browser-soon',
  machine: 'machine-soon',
  themes: 'themes-soon',
  hud: 'hud-soon',
  multiplayer: 'net-soon',
  testing: 'testing-soon',
};

/** The panel showing right now, or the stub when the scene has none. */
function activePanel() {
  return el(PANELS[state.sceneId] ?? 'controls-stub');
}

/**
 * Says that something here is not built yet, and stops offering it.
 *
 * This is the whole of the page's answer to an unfinished export: a control
 * that cannot do anything says so and stops responding, rather than throwing
 * into the console every time a visitor presses it.
 *
 * With an export name only that control goes quiet, so a scene missing one of
 * its four exports keeps the other three. Without one the scene itself is
 * missing and the whole block goes quiet.
 */
function comingUp(name = null) {
  const note = el(NOTES[state.sceneId] ?? '');
  if (note) note.textContent = name ? `${name} is coming up` : 'coming up';
  if (!name) {
    activePanel()?.classList.add('is-soon');
    return;
  }
  for (const node of document.querySelectorAll(`[data-export~="${name}"]`)) {
    node.classList.add('is-soon');
    if ('disabled' in node) node.disabled = true;
  }
}

function notComingUp() {
  activePanel()?.classList.remove('is-soon');
  const note = el(NOTES[state.sceneId] ?? '');
  if (note) note.textContent = '';
}

/**
 * Calls a scene export. Unlike `call`, a stubbed or missing export is a state
 * the page renders rather than an error it reports: the control greys out and
 * the panel's note says what is missing.
 */
function sceneCall(name, args = [], fallback = undefined) {
  if (!state.wasm || state.dead || state.booting) return fallback;
  if (state.notYet.has(name)) {
    comingUp(name);
    return fallback;
  }
  try {
    if (typeof state.wasm[name] !== 'function') throw new TypeError(`${name} is not a function`);
    return state.wasm[name](...args);
  } catch (error) {
    if (isNotYet(error)) {
      state.notYet.add(name);
      comingUp(name);
      return fallback;
    }
    if (!isTrap(error)) throw error;
    void restart(error);
    return fallback;
  }
}

/** The rail: one entry per scene, stubs greyed, the active one selected. */
function renderRail() {
  const list = el('rail-list');
  list.replaceChildren();
  state.scenes.forEach((scene, index) => {
    const item = document.createElement('li');
    item.setAttribute('role', 'tab');
    item.dataset.scene = scene.id;
    item.setAttribute('aria-selected', String(scene.id === state.sceneId));
    item.setAttribute('aria-disabled', String(!scene.ready));
    item.title = scene.caption;
    const num = document.createElement('span');
    num.className = 'rail-num';
    num.textContent = String(index + 1).padStart(2, '0');
    const title = document.createElement('span');
    title.className = 'rail-title';
    title.textContent = scene.title;
    item.append(num, title);
    if (!scene.ready) {
      const soon = document.createElement('span');
      soon.className = 'rail-soon';
      soon.textContent = 'soon';
      item.appendChild(soon);
    }
    item.addEventListener('click', () => selectScene(scene.id));
    list.appendChild(item);
  });
}

/** The strip over the canvas: number, title, caption, the three tries. */
function renderSceneHead() {
  const index = state.scenes.findIndex((s) => s.id === state.sceneId);
  const scene = state.scenes[index] ?? state.scenes[0];
  el('scene-num').textContent = String(index + 1).padStart(2, '0');
  el('scene-title').textContent = scene.title;
  el('scene-caption').textContent = scene.caption;
  const tries = el('scene-tries');
  tries.replaceChildren();
  for (const text of scene.tries) {
    const item = document.createElement('li');
    item.textContent = text;
    tries.appendChild(item);
  }
}

/**
 * Switches the canvas to `id`.
 *
 * The head and the controls follow the rail whatever the module says, so a
 * `?scene=` link lands on the right page even for a scene whose Rust is not
 * written; only `set_scene` is held back until the scene reports itself ready.
 */
function selectScene(id) {
  const scene = state.scenes.find((s) => s.id === id);
  if (!scene) return;
  state.sceneId = id;
  renderRail();
  renderSceneHead();
  renderControls();
  writeSceneQuery();
  if (!scene.ready) {
    comingUp();
    setStatus(`the ${scene.title} scene is not built yet`, '');
    return;
  }
  if (!state.wasm || state.dead) return;
  try {
    state.wasm.set_scene(id);
  } catch (error) {
    if (isNotYet(error)) {
      // The table said ready and the module disagrees. The module wins.
      scene.ready = false;
      renderRail();
      comingUp();
      setStatus(`the ${scene.title} scene is not built yet`, '');
      return;
    }
    if (!isTrap(error)) throw error;
    void restart(error);
  }
}

/** `?scene=<id>&theme=<name>`, kept beside the hash the editor uses. */
function writeSceneQuery() {
  const url = new URL(location.href);
  if (state.sceneId === DEFAULT_SCENE) url.searchParams.delete('scene');
  else url.searchParams.set('scene', state.sceneId);
  if (state.theme === THEMES[0]) url.searchParams.delete('theme');
  else url.searchParams.set('theme', state.theme);
  history.replaceState(null, '', url);
}

function readSceneQuery() {
  const params = new URL(location.href).searchParams;
  const scene = state.scenes.find((s) => s.id === params.get('scene'));
  if (scene) state.sceneId = scene.id;
  const theme = params.get('theme');
  if (THEMES.includes(theme)) state.theme = theme;
}

// ---------------------------------------------------------------------------
// Scene controls
//
// One block per scene in the right column, above the console. Each `enter`
// hook runs when its scene is selected and each `leave` hook when it is left;
// between them a scene may keep a poll running, and `renderControls` is what
// guarantees it is stopped.
// ---------------------------------------------------------------------------

/** Shows the active scene's block and hides every other one. */
function renderControls() {
  if (state.scenePoll) {
    clearInterval(state.scenePoll);
    state.scenePoll = null;
  }
  const wanted = PANELS[state.sceneId] ?? 'controls-stub';
  for (const id of [...Object.values(PANELS), 'controls-stub']) {
    const panel = el(id);
    if (panel) panel.hidden = id !== wanted;
  }
  const scene = state.scenes.find((s) => s.id === state.sceneId);
  if (wanted === 'controls-stub') {
    el('controls-stub-text').textContent = `The ${scene?.title ?? 'scene'} scene has no controls.`;
  }
  if (scene?.ready) notComingUp();
  else comingUp();
  ENTER[state.sceneId]?.();
}

/** A poll that runs only while its scene is the one on screen. */
function pollWhileHere(fn) {
  fn();
  state.scenePoll = setInterval(fn, POLL_MS);
}

// --- Browser ---------------------------------------------------------------

function buildBrowserChips() {
  const host = el('browser-chips');
  host.replaceChildren();
  for (const [query, why] of BROWSER_EXAMPLES) {
    const chip = document.createElement('button');
    chip.type = 'button';
    chip.className = 'chip';
    chip.dataset.export = 'browser_search';
    chip.textContent = query;
    chip.title = why;
    chip.addEventListener('click', () => {
      sceneCall('browser_search', [query]);
      el('browser-readout').textContent = `searched ${query}`;
    });
    host.appendChild(chip);
  }
}

// --- Machine ---------------------------------------------------------------

function setRedstone(on) {
  sceneCall('machine_redstone', [on]);
  // The class paints it and `aria-pressed` says it. Both, because the two
  // buttons are a toggle with a state, and a class alone tells a screen
  // reader nothing about which of the pair is the one in force.
  el('redstone-on').classList.toggle('on', on);
  el('redstone-on').setAttribute('aria-pressed', String(on));
  el('redstone-off').classList.toggle('on', !on);
  el('redstone-off').setAttribute('aria-pressed', String(!on));
}

// --- Themes ----------------------------------------------------------------

function buildThemeButtons() {
  const host = el('theme-buttons');
  host.replaceChildren();
  for (const name of THEMES) {
    const button = document.createElement('button');
    button.type = 'button';
    button.dataset.theme = name;
    button.dataset.export = 'set_theme';
    button.textContent = name;
    button.addEventListener('click', () => setTheme(name));
    host.appendChild(button);
  }
  buildThemeDiff();
  markTheme();
}

/** The token table: what actually differs between the three RON files. */
function buildThemeDiff() {
  const table = el('theme-diff');
  table.replaceChildren();
  const head = document.createElement('thead');
  const headRow = document.createElement('tr');
  headRow.appendChild(document.createElement('th'));
  for (const name of THEMES) {
    const cell = document.createElement('th');
    cell.textContent = name;
    cell.dataset.theme = name;
    headRow.appendChild(cell);
  }
  head.appendChild(headRow);
  const body = document.createElement('tbody');
  for (const row of THEME_DIFF.rows) {
    const line = document.createElement('tr');
    const label = document.createElement('th');
    label.scope = 'row';
    label.textContent = row.token;
    label.title = row.note;
    line.appendChild(label);
    row.values.forEach((value, index) => {
      const cell = document.createElement('td');
      cell.textContent = value;
      cell.dataset.theme = THEMES[index];
      line.appendChild(cell);
    });
    body.appendChild(line);
  }
  table.append(head, body);
}

function markTheme() {
  for (const node of document.querySelectorAll('#theme-diff [data-theme]')) {
    node.dataset.active = String(node.dataset.theme === state.theme);
    if (node.tagName === 'TH') node.setAttribute('aria-selected', String(node.dataset.theme === state.theme));
  }
  for (const button of document.querySelectorAll('#theme-buttons button')) {
    const active = button.dataset.theme === state.theme;
    button.classList.toggle('on', active);
    button.setAttribute('aria-pressed', String(active));
  }
}

function setTheme(name) {
  if (!THEMES.includes(name)) return;
  sceneCall('set_theme', [name]);
  state.theme = name;
  markTheme();
  writeSceneQuery();
}

// --- HUD -------------------------------------------------------------------

/** The layout the visitor last dragged into place, or '' when there is none. */
function storedHud() {
  try {
    return localStorage.getItem(HUD_KEY) ?? '';
  } catch {
    // A browser with site data switched off. The scene still works; the
    // layout just does not outlive the tab.
    return '';
  }
}

function storeHud(ron) {
  try {
    if (ron) localStorage.setItem(HUD_KEY, ron);
    else localStorage.removeItem(HUD_KEY);
  } catch {
    /* nothing to do: the scene degrades to a layout that lasts one visit */
  }
}

function showHudStored() {
  const ron = storedHud();
  el('hud-stored').textContent = ron
    ? `${ron.length} bytes of RON in localStorage under ${HUD_KEY}`
    : 'nothing stored yet';
}

function setHudEdit(on) {
  sceneCall('hud_edit', [on]);
  const button = el('hud-edit');
  button.setAttribute('aria-pressed', String(on));
  button.textContent = on ? 'Done' : 'Edit layout';
}

function enterHud() {
  const ron = storedHud();
  if (ron) sceneCall('restore_hud_layout', [ron]);
  setHudEdit(false);
  showHudStored();
  // The layout is written back on a timer rather than on a drag, because the
  // page never sees the drag: it happens inside the canvas.
  pollWhileHere(() => {
    const ron = sceneCall('hud_layout', [], '');
    if (typeof ron === 'string' && ron.length > 0 && ron !== storedHud()) {
      storeHud(ron);
      showHudStored();
    }
  });
}

function resetHud() {
  storeHud('');
  // The empty string is "forget what you were given and use the registered
  // defaults" (agreed with package A).
  sceneCall('restore_hud_layout', ['']);
  setHudEdit(false);
  showHudStored();
}

// --- Multiplayer -----------------------------------------------------------

function netConfig() {
  const latency = Number(el('net-latency').value);
  const loss = Number(el('net-loss').value);
  el('net-latency-out').textContent = `${latency} ms`;
  el('net-loss-out').textContent = `${loss} %`;
  sceneCall('net_config', [latency, loss]);
}

/** Which end of the link a log line came from, for the colour. */
function netPeer(text) {
  if (/\bserver\b/i.test(text)) return ['server', 'peer-server'];
  if (/\b(client|peer)\s*2\b/i.test(text)) return ['client 2', 'peer-2'];
  if (/\b(client|peer)\s*1\b/i.test(text)) return ['client 1', 'peer-1'];
  return ['link', ''];
}

/** A `who: "net"` line, which goes here and not into the console pane. */
function appendNet(line) {
  const log = el('net-log');
  const [label, kind] = netPeer(line.text);
  const row = document.createElement('div');
  row.className = 'row';
  const peer = document.createElement('span');
  peer.className = `peer ${kind}`;
  peer.textContent = label;
  const body = document.createElement('span');
  body.className = 'body';
  body.textContent = line.text;
  row.append(peer, body);
  log.appendChild(row);
  while (log.childElementCount > MAX_ROWS) log.firstElementChild.remove();
  log.scrollTop = log.scrollHeight;
}

// --- Testing ---------------------------------------------------------------

function buildTestMods() {
  const host = el('test-mods');
  host.replaceChildren();
  const withTests = state.mods.filter((mod) => (mod.tests ?? []).length > 0);
  if (withTests.length === 0) {
    const note = document.createElement('p');
    note.className = 'muted';
    note.textContent = 'No bundled mod has tests/*.lua.';
    host.appendChild(note);
    return;
  }
  for (const mod of withTests) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'primary';
    button.dataset.export = 'run_tests';
    button.textContent = `Run ${mod.id}`;
    button.title = (mod.tests ?? []).map((name) => `tests/${name}`).join('\n');
    button.addEventListener('click', () => {
      el('test-results').replaceChildren();
      appendTestLine({ level: 'info', text: `running ${mod.id}…` });
      setStatus(`running ${mod.id} tests…`, 'busy');
      sceneCall('run_tests', [mod.id]);
    });
    host.appendChild(button);
  }
}

/** A `who: "test"` line, mirrored from the console into the results list. */
function appendTestLine(line) {
  const list = el('test-results');
  if (!list) return;
  const failed = line.level === 'error' || /\bfail|\berror\b/i.test(line.text);
  const passed = !failed && /\bok\b|\bpass(ed)?\b/i.test(line.text);
  const item = document.createElement('li');
  item.className = failed ? 'fail' : passed ? 'pass' : '';
  const mark = document.createElement('span');
  mark.className = 'mark';
  mark.textContent = failed ? '✕' : passed ? '✓' : '·';
  const text = document.createElement('span');
  text.textContent = line.text;
  item.append(mark, text);
  list.appendChild(item);
  list.scrollTop = list.scrollHeight;
}

function showReplay(status) {
  const seek = el('replay-seek');
  const frames = Number(status?.frames ?? 0);
  const frame = Number(status?.frame ?? 0);
  const playing = Boolean(status?.playing);
  state.replay = { frame, frames, playing };
  seek.max = String(Math.max(0, frames - 1));
  // Never fight the pointer: while the visitor is dragging the scrubber the
  // poll leaves it alone.
  if (document.activeElement !== seek) seek.value = String(frame);
  el('replay-count').textContent = frames
    ? `frame ${frame} of ${frames}`
    : 'no recording loaded';
  const play = el('replay-play');
  play.setAttribute('aria-pressed', String(playing));
  play.textContent = playing ? 'Pause' : 'Play';
}

function replayStatus() {
  const raw = sceneCall('replay_status', [], null);
  if (raw == null) return showReplay(null);
  try {
    return showReplay(typeof raw === 'string' ? JSON.parse(raw) : raw);
  } catch (error) {
    console.warn('replay_status:', error);
    return showReplay(null);
  }
}

function enterTesting() {
  buildTestMods();
  pollWhileHere(replayStatus);
}

/** What each scene does the moment it becomes the one on screen. */
const ENTER = {
  browser: () => {
    el('browser-readout').textContent = 'nothing yet';
  },
  machine: () => setRedstone(false),
  themes: () => markTheme(),
  hud: enterHud,
  multiplayer: () => netConfig(),
  testing: enterTesting,
};

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

/**
 * Watches for a trap that happened inside the module's own animation frame
 * rather than inside a call the page made.
 *
 * This is the common case and the one `call` cannot see: Bevy drives its loop
 * from a `requestAnimationFrame` the module registered itself, so a mod that
 * raises during a frame surfaces here as an uncaught error and nowhere else.
 */
function watchForTraps() {
  window.addEventListener('error', (event) => {
    const error = event.error ?? event.message;
    if (!isTrap(error)) return;
    event.preventDefault();
    void restart(error);
  });
  window.addEventListener('unhandledrejection', (event) => {
    if (!isTrap(event.reason)) return;
    event.preventDefault();
    void restart(event.reason);
  });
}

/**
 * Keeps the rail on screen while the module takes the canvas.
 *
 * Bevy focuses the canvas as it comes up, and on a stacked layout the browser
 * answers a focus by scrolling the element into view, which throws the rail
 * off the top of the page before the visitor has seen it. Undo that, until
 * the visitor scrolls somewhere themselves.
 */
function keepTheRailInView() {
  const split = document.querySelector('.split');
  if (!split) return;
  let theirs = false;
  for (const event of ['wheel', 'pointerdown', 'keydown']) {
    split.addEventListener(event, () => {
      theirs = true;
    }, { passive: true });
  }
  document.addEventListener('focusin', (event) => {
    if (!theirs && event.target?.id === 'slotted-canvas') split.scrollTo({ top: 0 });
  });
  split.scrollTo({ top: 0 });
}

async function main() {
  watchForTraps();
  // The rail shows straight away, from the fallback table, so the page has a
  // shape while the module compiles.
  readSceneQuery();
  renderRail();
  renderSceneHead();
  renderControls();
  setStatus('compiling wasm…', 'busy');
  try {
    await boot();
  } catch (error) {
    setStatus('the module failed to start', 'bad');
    console.error(error);
  }
  el('boot').classList.add('gone');
  setStatus('running', 'ok');
  keepTheRailInView();

  state.mods = JSON.parse(call('list_mods', [], '[]') ?? '[]');
  readHash();
  buildTestMods();

  // The module's own scene table replaces the fallback, and the scene the
  // URL asked for is applied now that there is a world to apply it to.
  try {
    const scenes = JSON.parse(call('list_scenes', [], '[]') ?? '[]');
    if (Array.isArray(scenes) && scenes.length) state.scenes = scenes;
  } catch (error) {
    console.warn('list_scenes:', error);
  }
  readSceneQuery();
  selectScene(state.sceneId);

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

  wireSceneControls();
  el('run').addEventListener('click', () => run());
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
    // Alt+1..8 switches scenes; plain digits stay the game's hotbar keys.
    if (event.altKey && /^[1-8]$/.test(event.key)) {
      const scene = state.scenes[Number(event.key) - 1];
      if (scene) {
        event.preventDefault();
        selectScene(scene.id);
      }
    }
  });

  // The ring holds everything the world said while the module was booting;
  // show that, then drop the pending copies of the same lines so the poll
  // below does not print them twice.
  appendLines(JSON.parse(call('console_history', [], '[]') ?? '[]'));
  call('drain_console', []);
  takeSnapshot(true);
  requestAnimationFrame(pollConsole);
}

/**
 * Binds every scene control once, at boot. The blocks themselves are in
 * `index.html`; a hidden one is simply not on screen, so binding them all up
 * front costs nothing and keeps the scene switch to showing and hiding.
 */
function wireSceneControls() {
  buildBrowserChips();
  buildThemeButtons();
  el('redstone-on').addEventListener('click', () => setRedstone(true));
  el('redstone-off').addEventListener('click', () => setRedstone(false));

  el('hud-edit').addEventListener('click', () => {
    setHudEdit(el('hud-edit').getAttribute('aria-pressed') !== 'true');
  });
  el('hud-reset').addEventListener('click', resetHud);

  el('net-latency').addEventListener('input', netConfig);
  el('net-loss').addEventListener('input', netConfig);
  el('net-clear').addEventListener('click', () => el('net-log').replaceChildren());

  el('replay-load').addEventListener('click', () => {
    sceneCall('replay_load', []);
    replayStatus();
  });
  el('replay-play').addEventListener('click', () => {
    const playing = el('replay-play').getAttribute('aria-pressed') === 'true';
    sceneCall('replay_play', [!playing]);
    replayStatus();
  });
  el('replay-seek').addEventListener('input', () => {
    sceneCall('replay_seek', [Number(el('replay-seek').value)]);
  });
  el('replay-seek').addEventListener('change', replayStatus);

  // The theme the URL asked for, applied once the module can hear it.
  if (state.theme !== THEMES[0]) setTheme(state.theme);
}

// The smoke test drives these, and so does anyone poking at the page in a
// devtools console.
window.slottedPlayground = {
  run,
  state,
  // The exports, through the same guard the page uses, so a poke from a
  // devtools console cannot leave the page holding a dead module either.
  call,
  restart,
  takeSnapshot,
  selectScene,
  list_scenes: (...args) => call('list_scenes', args, '[]'),
  set_scene: (...args) => call('set_scene', args),
  current_scene: (...args) => call('current_scene', args, ''),
  reload_mod: (...args) => call('reload_mod', args),
  run_tests: (...args) => call('run_tests', args),
  list_mods: (...args) => call('list_mods', args, '[]'),
  get_mod_file: (...args) => call('get_mod_file', args, ''),
  drain_console: (...args) => call('drain_console', args, '[]'),
  console_history: (...args) => call('console_history', args, '[]'),
  snapshot_state: (...args) => call('snapshot_state', args, ''),
  restore_state: (...args) => call('restore_state', args),
  // The scene exports, through the guard that turns an unfinished one into a
  // greyed control rather than an exception.
  set_theme: setTheme,
  machine_redstone: setRedstone,
  browser_search: (...args) => sceneCall('browser_search', args),
  hud_edit: setHudEdit,
  hud_layout: (...args) => sceneCall('hud_layout', args, ''),
  restore_hud_layout: (...args) => sceneCall('restore_hud_layout', args),
  net_config: (...args) => sceneCall('net_config', args),
  replay_status: (...args) => sceneCall('replay_status', args, null),
  replay_seek: (...args) => sceneCall('replay_seek', args),
  replay_play: (...args) => sceneCall('replay_play', args),
  replay_load: (...args) => sceneCall('replay_load', args),
};

main();
