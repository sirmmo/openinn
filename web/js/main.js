// The page: loads the engine and a ruleset, draws the inn, turns clicks into commands, runs the
// night, keeps the log and saves the game (its seed and commands) in localStorage.

import { Engine, fetchRuleset } from './engine.js';
import { View, cellCenter } from './view.js';
import { format, duration } from './numbers.js';

const $ = (id) => document.getElementById(id);
const params = new URLSearchParams(location.search);
const GAME = params.get('game') || 'dungeon-inn';
// A season (the ruleset's nights) or endless (no last night, more room, more levels); each
// keeps its own save.
const MODE = params.get('mode') === 'endless' ? 'endless' : 'season';
const SAVE_KEY = MODE === 'endless' ? `openinn:${GAME}:endless` : `openinn:${GAME}`;
const TICK_MS = 250; // a tick at normal speed

let engine, files, catalog, view, state;
let look = {}; // the ruleset's look block: words, help and style for the page
let tool = null; // { kind: 'dig', room } | { kind: 'build', unit } | { kind: 'remove' } | { kind: 'turn' } | { kind: 'paint', room, finish }
let unitRoom = ''; // the Furnish list shows units for this room ('' for all)
let speed = 1;
let timer = null;
let selected = null; // guest id
const names = new Map(); // guest id -> kind id

// ---------- start ----------

async function boot() {
  view = new View($('scene'));
  $('loadsub').textContent = 'Waking the innkeeper…';
  [engine, files] = await Promise.all([Engine.load('openinn.wasm'), fetchRuleset(GAME)]);
  let saved = null;
  try { saved = JSON.parse(localStorage.getItem(SAVE_KEY) || 'null'); } catch { saved = null; }
  // A first start reads the ruleset, whose `look` has the words new seeds are made of.
  const first = engine.start(files, 'look');
  if (first.error) {
    $('loadsub').textContent = first.error;
    window.WavedashHook?.ready(); // show the error, not Wavedash's loading screen
    return;
  }
  look = first.catalog.look || {};
  const seed = params.get('seed') || saved?.seed || newSeed();
  const commands = !params.get('seed') || params.get('seed') === saved?.seed ? saved?.commands : null;
  let res = engine.start(files, seed, commands, MODE);
  if (res.error && commands) {
    console.warn('the save does not replay:', res.error);
    res = engine.start(files, seed, null, MODE);
  }
  if (res.error) {
    $('loadsub').textContent = res.error;
    window.WavedashHook?.ready();
    return;
  }
  catalog = res.catalog;
  $('loadsub').textContent = `${look.loading || 'Setting the tables'}…`;
  await Promise.all([
    view.load(catalog, (k) => { $('loadbar').style.width = `${Math.round(k * 100)}%`; window.WavedashHook?.progress(k); }),
    dress(),
  ]);
  setupHud();
  apply(res, false);
  view.focus(state.cells);
  $('loading').hidden = true;
  $('hud').hidden = false;
  window.WavedashHook?.ready(); // on Wavedash: loaded, show the inn
  // For the console and for tests: window.openinn.send({ do: 'dig', x: 6, y: 1, room: 'hall' }).
  window.openinn = { send, state: () => state, view, setSpeed, setLevel, catalog, engine, restart };
  setLevel(0);
  if (catalog.idle) {
    setupLedger();
    // Time away: the ledger catches up (up to its offline_max; the bridge's `away` caps the money).
    const at = +localStorage.getItem(`${SAVE_KEY}:at`);
    if (saved && at && commands) {
      const away = Math.min(Date.now() - at, catalog.idle.offline_max * 1000);
      if (away > 5000) send({ do: 'away', ms: Math.round(away) });
    }
  }
  if (!localStorage.getItem(`openinn:seen-help:${GAME}`)) {
    $('help').hidden = false;
    localStorage.setItem(`openinn:seen-help:${GAME}`, '1');
  }
}

// The ruleset's own help text (`help` in its look) and stylesheet (`style`), from games/<id>/.
async function dress() {
  const base = `games/${GAME}/`;
  if (look.style) {
    const l = document.createElement('link');
    l.rel = 'stylesheet';
    l.href = base + look.style;
    document.head.appendChild(l);
  }
  if (look.help) {
    try {
      const r = await fetch(base + look.help);
      if (r.ok) $('helpBody').innerHTML = await r.text();
    } catch { /* the generic help stays */ }
  }
}

function newSeed() {
  const w = (look.seeds || 'inn tankard lantern barrel stool').split(/\s+/);
  return `${w[Math.floor(Math.random() * w.length)]}-${Math.floor(Math.random() * 9000 + 1000)}`;
}

// A new game; `commands` may start it (an imported plan).
function restart(seed = newSeed(), commands = null) {
  stopNight();
  localStorage.removeItem(SAVE_KEY);
  const u = new URL(location.href);
  u.searchParams.delete('seed');
  history.replaceState(null, '', u);
  const res = engine.start(files, seed, commands, MODE);
  if (res.error) {
    toast(res.error);
    return false;
  }
  names.clear();
  selected = null;
  $('log').innerHTML = '';
  $('over').hidden = true;
  view.guests.forEach((g) => view.scene.remove(g.obj));
  view.guests.clear();
  for (const c of view.cells.values()) view.scene.remove(c.group);
  view.cells.clear();
  for (const u2 of view.units.values()) view.scene.remove(u2.obj);
  view.units.clear();
  for (const w of view.walls.values()) view.scene.remove(w.obj);
  view.walls.clear();
  setLevel(0);
  apply(res, !!commands);
  view.focus(state.cells);
  return true;
}

// ---------- commands ----------

function send(command) {
  const res = engine.apply(command);
  if (res.error) {
    toast(res.error);
    return false;
  }
  apply(res, true);
  return true;
}

function apply(res, save) {
  const before = state;
  state = res.view;
  for (const e of res.events || []) logEvent(e);
  view.sync(state);
  render(before);
  if (save) {
    const s = engine.save();
    try {
      localStorage.setItem(SAVE_KEY, JSON.stringify(s));
      localStorage.setItem(`${SAVE_KEY}:at`, String(Date.now()));
    } catch { /* full: keep playing */ }
  }
}

// ---------- the night ----------

function startNight() {
  stopNight();
  if (speed === 0 || !state || state.build || state.over) return;
  const ms = TICK_MS / speed;
  view.tickTime = ms / 1000;
  timer = setInterval(() => {
    if (!state || state.build || state.over) return stopNight();
    send({ do: 'tick', n: 1 });
  }, ms);
}

function stopNight() {
  clearInterval(timer);
  timer = null;
}

function setSpeed(s) {
  speed = s;
  document.querySelectorAll('.speeds button').forEach((b) => b.classList.toggle('on', +b.dataset.speed === s));
  startNight();
}

// ---------- HUD ----------

function innName() {
  return state?.name || catalog.inn.name;
}

function setupHud() {
  $('innName').onclick = () => {
    const name = prompt('Name your inn (empty for its own name):', innName());
    if (name !== null) send({ do: 'rename', name });
  };
  document.body.dataset.game = GAME;
  document.querySelector('.stat[title="Gold"]').title = catalog.inn.money;
  document.querySelector('.stat[title="Reputation"]').title = catalog.inn.reputation;
  const digs = $('digButtons');
  for (const r of catalog.rooms) {
    const b = toolButton(r.name, r.cost, `<span class="sw" style="background:${r.colour}"></span>`);
    b.title = r.text ? `${r.name}: ${r.text}` : r.name;
    b.onclick = () => setTool(tool?.kind === 'dig' && tool.room === r.id ? null : { kind: 'dig', room: r.id }, b);
    b.dataset.cost = r.cost;
    digs.appendChild(b);
  }
  for (const [b, r] of [...digs.children].map((b, i) => [b, catalog.rooms[i]])) b.dataset.room = r.id;
  const units = $('unitButtons');
  for (const u of catalog.units) {
    const where = u.rooms.map((id) => roomName(id)).join(', ');
    const b = toolButton(u.name, u.cost, '');
    const combos = (catalog.combos || []).filter((c) => c.at.includes(u.id) || c.near.some((n) => n.units.includes(u.id)));
    b.title = [
      `${u.name}: in ${where || 'any room'}${u.seats ? `, seats ${u.seats}` : ''}`,
      u.text,
      ...combos.map((c) => `✦ ${c.name}: ${comboWhat(c)}`),
    ].filter(Boolean).join('\n');
    if (combos.length) b.querySelector('.n').insertAdjacentHTML('beforeend', ' <i class="gem" title="Part of a combo">✦</i>');
    b.onclick = () => setTool(tool?.kind === 'build' && tool.unit === u.id ? null : { kind: 'build', unit: u.id }, b);
    b.dataset.cost = u.cost;
    b.dataset.unit = u.id;
    b.dataset.rooms = u.rooms.join(' ');
    units.appendChild(b);
  }
  // Rooms to show the furniture of, when there is a lot of it.
  const pick = $('unitRooms');
  const roomsWithUnits = catalog.rooms.filter((r) => catalog.units.some((u) => u.rooms.includes(r.id)));
  pick.hidden = catalog.units.length <= 10;
  pick.innerHTML = [{ id: '', name: 'For every room' }, ...roomsWithUnits.map((r) => ({ id: r.id, name: `For the ${r.name.toLowerCase()}` }))]
    .map((r) => `<option value="${r.id}">${esc(r.name)}</option>`).join('');
  pick.onchange = () => { unitRoom = pick.value; filterUnits(); };
  // Choosing a room to dig shows its furniture.
  digs.addEventListener('click', (e) => {
    const b = e.target.closest('button[data-room]');
    if (!b || pick.hidden || !roomsWithUnits.some((r) => r.id === b.dataset.room)) return;
    pick.value = unitRoom = b.dataset.room;
    filterUnits();
  });
  $('removeTool').onclick = () => setTool(tool?.kind === 'remove' ? null : { kind: 'remove' }, $('removeTool'));
  $('turnTool').onclick = () => setTool(tool?.kind === 'turn' ? null : { kind: 'turn' }, $('turnTool'));
  // Floors: each room's finishes, and the plain floor back.
  const floors = $('floorButtons');
  const finishes = catalog.rooms.flatMap((r) => (r.finishes || []).map((f) => ({ room: r, f })));
  $('floorsBox').hidden = !finishes.length;
  for (const { room: r, f } of finishes) {
    const b = toolButton(f.name, f.cost, `<span class="sw" style="background:${r.colour}"></span>`);
    b.title = `${f.name}, for the ${r.name.toLowerCase()} (a cell)`;
    b.onclick = () => setTool(tool?.kind === 'paint' && tool.finish === f.id && tool.room === r.id ? null : { kind: 'paint', room: r.id, finish: f.id }, b);
    b.dataset.cost = f.cost;
    floors.appendChild(b);
  }
  if (finishes.length) {
    const b = toolButton('Plain floor', 'free', '');
    b.querySelector('.coin').remove();
    b.onclick = () => setTool(tool?.kind === 'paint' && !tool.finish ? null : { kind: 'paint', room: null, finish: '' }, b);
    floors.appendChild(b);
  }
  $('combosBox').hidden = !(catalog.combos || []).length;
  const walls = $('wallButtons');
  const wallTool = (name, cost, t) => {
    const b = toolButton(name, cost, '');
    if (cost === '') b.querySelector('.c').textContent = 'half back';
    else b.dataset.cost = cost;
    b.onclick = () => setTool(tool?.kind === t.kind ? null : t, b);
    walls.appendChild(b);
  };
  wallTool(look.wallname || 'Wall', catalog.inn.wall, { kind: 'wall', door: false });
  wallTool(look.doorname || 'Door', catalog.inn.door, { kind: 'door', door: true });
  wallTool('Take down', '', { kind: 'unwall' });

  // Levels.
  $('levelUp').onclick = () => setLevel(view.level - 1);
  $('levelDown').onclick = () => setLevel(view.level + 1);
  $('openDoors').onclick = () => {
    setTool(null);
    if (send({ do: 'next' })) setSpeed(speed || 1);
  };
  document.querySelectorAll('.speeds button').forEach((b) => { b.onclick = () => setSpeed(+b.dataset.speed); });
  setSpeed(1);

  const layers = $('layers');
  for (const l of catalog.layers) {
    const b = document.createElement('button');
    b.textContent = l.id;
    b.dataset.layer = l.id;
    layers.appendChild(b);
  }
  layers.addEventListener('click', (e) => {
    const b = e.target.closest('button');
    if (!b) return;
    layers.querySelectorAll('button').forEach((x) => x.classList.toggle('on', x === b));
    view.setLayer(b.dataset.layer || null);
  });

  $('bHelp').onclick = () => { $('help').hidden = false; };
  $('helpClose').onclick = () => { $('help').hidden = true; };
  $('bMenu').onclick = () => { $('menu').hidden = !$('menu').hidden; };
  $('mNew').onclick = () => {
    $('menu').hidden = true;
    if (confirm('Close this inn and start a new one?')) restart();
  };
  const other = new URL(location.href);
  other.searchParams.delete('seed');
  if (MODE === 'endless') other.searchParams.delete('mode');
  else other.searchParams.set('mode', 'endless');
  $('mMode').href = other.href;
  $('mMode').textContent = MODE === 'endless' ? 'Play a season instead' : 'Play endless (keep building, more levels)';
  $('mExport').onclick = () => {
    $('menu').hidden = true;
    const r = engine.call({ op: 'export' });
    if (r.error) return toast(r.error);
    download(JSON.stringify(r.dwellings, null, 2), `${innName()}.json`);
    toast('Exported in the format of watabou\'s Dwellings');
  };
  $('mImport').onclick = () => { $('menu').hidden = true; $('importFile').click(); };
  $('importFile').onchange = async (e) => {
    const file = e.target.files[0];
    e.target.value = '';
    if (!file) return;
    let doc;
    try { doc = JSON.parse(await file.text()); } catch { return toast('That file is not JSON'); }
    const r = engine.call({ op: 'import', dwellings: doc, mode: MODE });
    if (r.error) return toast(r.error);
    const note = r.warnings.length ? `\n\n${r.warnings.join('\n')}` : '';
    if (!confirm(`Start a new inn in this building?${note}`)) return;
    if (restart(newSeed(), [r.plan])) toast(`Built ${r.plan.cells.length} cells from ${file.name}`);
  };
  $('mLink').onclick = async () => {
    $('menu').hidden = true;
    const u = new URL(location.href);
    u.searchParams.set('seed', engine.save().seed);
    try { await navigator.clipboard.writeText(u.href); toast('Link copied: same seed, same guests'); } catch { prompt('Copy this link:', u.href); }
  };
  $('mSave').onclick = () => {
    $('menu').hidden = true;
    download(JSON.stringify({ game: GAME, ...engine.save() }), `openinn-${GAME}-${engine.save().seed}.json`);
  };
  $('overNew').onclick = () => restart();
  $('overLook').onclick = () => { $('over').hidden = true; };

  filterUnits();
  setupInput();
}

function roomName(id) {
  return catalog.rooms.find((r) => r.id === id)?.name || id;
}

function unitName(id) {
  return catalog.units.find((u) => u.id === id)?.name || id;
}

// What a combo needs, in words: "a Hearth within 2", "two Candles beside it".
function comboWhat(c) {
  const count = ['', '', 'two ', 'three ', 'four ', 'five '];
  const parts = c.near.map((n) => {
    const what = n.units.map(unitName).join(' or ');
    const k = n.count > 1 ? (count[n.count] || `${n.count} `) : 'a ';
    const where = n.radius <= 1 ? 'beside it' : `within ${n.radius}`;
    return `${k}${what} ${where}`;
  });
  if (c.rooms.length) parts.push(`in the ${c.rooms.map((r) => roomName(r).toLowerCase()).join(' or ')}`);
  return parts.join(', ') || 'by itself';
}

function filterUnits() {
  document.querySelectorAll('#unitButtons button').forEach((b) => {
    b.hidden = !!unitRoom && !b.dataset.rooms.split(' ').includes(unitRoom) && b.dataset.rooms !== '';
  });
}

function renderCombos() {
  if (!(catalog.combos || []).length) return;
  const active = new Map();
  for (const u of state.units) for (const c of u.combos || []) active.set(c, (active.get(c) || 0) + 1);
  const locked = new Set(state.locked || []);
  const key = JSON.stringify([...active]) + [...locked].join();
  if (key === renderCombos.key) return;
  renderCombos.key = key;
  $('comboCount').textContent = `${active.size}/${catalog.combos.length}`;
  $('comboList').innerHTML = catalog.combos.map((c) => {
    const n = active.get(c.id) || 0;
    const cls = n ? 'combo on' : locked.has(c.id) ? 'combo locked' : 'combo';
    const lock = locked.has(c.id) ? ` · <i>not unlocked yet: see the ${esc(ledgerName())}</i>` : '';
    return `<div class="${cls}"><b>${n ? '✦' : '◇'} ${esc(c.name)}</b>${n > 1 ? ` <span class="k">×${n}</span>` : ''}
      <span class="w">${esc(c.at.map(unitName).join(' or '))}: ${esc(comboWhat(c))}${lock}</span>
      ${c.text ? `<span class="t">${esc(c.text)}</span>` : ''}</div>`;
  }).join('');
}

// The combos a unit of kind `k` at (x, y, z) would have, if the units were `units` (statically:
// its furniture and room; the combos' tests are left to the engine).
function combosAt(k, x, y, z, room, units) {
  return (catalog.combos || []).filter((c) => c.at.includes(k)
    && (!c.rooms.length || c.rooms.includes(room))
    && c.near.every((n) => units.filter((v) => v.z === z && !(v.x === x && v.y === y)
      && n.units.includes(v.unit) && Math.abs(v.x - x) <= n.radius && Math.abs(v.y - y) <= n.radius).length >= n.count))
    .map((c) => c.id);
}

// Whether putting unit `k` on a cell would make a combo hold that does not now (for it, or for
// furniture near it).
function makesCombo(k, cell) {
  const rooms = new Map(state.cells.map((c) => [`${c.x},${c.y},${c.z}`, c.room]));
  const after = [...state.units, { x: cell.x, y: cell.y, z: cell.z, unit: k }];
  if (combosAt(k, cell.x, cell.y, cell.z, cell.room, after).length) return true;
  return state.units.some((u) => {
    if (u.z !== cell.z || Math.abs(u.x - cell.x) > 3 || Math.abs(u.y - cell.y) > 3) return false;
    const room = rooms.get(`${u.x},${u.y},${u.z}`);
    const before = combosAt(u.unit, u.x, u.y, u.z, room, state.units);
    return combosAt(u.unit, u.x, u.y, u.z, room, after).some((c) => !before.includes(c));
  });
}

function download(text, name) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
  a.download = name.replace(/[^\w.-]+/g, '-').toLowerCase();
  a.click();
}

// Which level is shown (and built on).
function setLevel(z) {
  const levels = state?.grid[2] ?? 1;
  z = Math.max(0, Math.min(levels - 1, z));
  view.setLevel(z);
  const name = look.levelname || 'Level';
  $('levelLabel').textContent = `${name} ${z + 1}/${levels}`;
  $('levelUp').disabled = z === 0;
  $('levelDown').disabled = z >= levels - 1;
  // In an inn that grows up, the next level is above: the arrows say so.
  const down = catalog.inn.down;
  $('levelUp').textContent = down ? '▲' : '▼';
  $('levelDown').textContent = down ? '▼' : '▲';
  $('levelUp').title = down ? 'The level above' : 'The level below';
  $('levelDown').title = down ? 'The level below' : 'The level above';
  showHints();
  renderGuests();
}

function toolButton(name, cost, extra) {
  const b = document.createElement('button');
  b.className = 'tool';
  b.innerHTML = `<span class="n">${name}</span><span class="c">${extra}<i class="coin" style="width:11px;height:11px"></i>${cost}</span>`;
  return b;
}

function setTool(t, button) {
  tool = t;
  document.querySelectorAll('#tools button.tool').forEach((b) => b.classList.toggle('on', !!t && b === button));
  showHints();
}

const WALL_TOOLS = new Set(['wall', 'door', 'unwall']);

function showHints() {
  if (!state || !tool || !state.build) return view.showHints([]);
  const z = view.level;
  const dug = new Map(state.cells.map((c) => [`${c.x},${c.y},${c.z}`, c]));
  const used = new Set(state.units.map((u) => `${u.x},${u.y},${u.z}`));
  const stairsKinds = new Set(catalog.units.filter((u) => u.stairs).map((u) => u.id));
  const below = new Set(state.units.filter((u) => stairsKinds.has(u.unit)).map((u) => `${u.x},${u.y},${u.z + 1}`));
  const [width, height] = state.grid;
  const out = [];
  if (WALL_TOOLS.has(tool.kind)) {
    const walls = new Map(state.walls.map((w) => [`${w.x},${w.y},${w.z},${w.side}`, w]));
    const edges = [];
    for (const c of state.cells) {
      if (c.z !== z) continue;
      for (const [side, dx, dy] of [['e', 1, 0], ['s', 0, 1]]) {
        if (!dug.has(`${c.x + dx},${c.y + dy},${z}`)) continue;
        const w = walls.get(`${c.x},${c.y},${z},${side}`);
        const ok = tool.kind === 'unwall' ? !!w : !w || w.door !== tool.door;
        if (ok) edges.push([c.x, c.y, side]);
      }
    }
    return view.showEdgeHints(edges, tool.kind === 'unwall' ? 0xff8a65 : 0xfff3b0);
  }
  if (tool.kind === 'dig') {
    for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
      if (dug.has(`${x},${y},${z}`)) continue;
      if (below.has(`${x},${y},${z}`) || [[1, 0], [-1, 0], [0, 1], [0, -1]].some(([dx, dy]) => dug.has(`${x + dx},${y + dy},${z}`))) out.push([x, y]);
    }
  } else if (tool.kind === 'build') {
    const u = catalog.units.find((k) => k.id === tool.unit);
    for (const c of state.cells) {
      if (c.z !== z || used.has(`${c.x},${c.y},${c.z}`)) continue;
      if (u.stairs && z + 1 >= state.grid[2]) continue;
      if (!u.rooms.length || u.rooms.includes(c.room)) out.push(makesCombo(u.id, c) ? [c.x, c.y, 0xffc93c] : [c.x, c.y]);
    }
  } else if (tool.kind === 'paint') {
    for (const c of state.cells) {
      if (c.z !== z) continue;
      if (tool.finish ? c.room === tool.room && c.finish !== tool.finish : !!c.finish) out.push([c.x, c.y]);
    }
  } else {
    for (const u of state.units) if (u.z === z) out.push([u.x, u.y]);
  }
  view.showHints(out, tool.kind === 'remove' ? 0xff8a65 : tool.kind === 'turn' ? 0x8fd3ff : 0xfff3b0);
}

function global(id) {
  return state.globals.find(([k]) => k === id)?.[1] ?? 0;
}

function render(before) {
  const ph = catalog.phases.find((p) => p.id === state.phase);
  $('innName').textContent = innName();
  document.title = `OpenInn: ${innName()}`;
  $('dayLabel').textContent = MODE === 'endless' ? `Day ${state.day} ∞` : `Day ${state.day}`;
  $('levels').hidden = state.grid[2] <= 1;
  $('phaseLabel').textContent = ph?.name || state.phase;
  const gold = global(catalog.inn.money);
  $('gold').textContent = Math.floor(gold);
  $('rep').textContent = Math.round(global(catalog.inn.reputation));
  $('guestCount').textContent = state.guests.length;
  $('buildTools').hidden = !state.build;
  $('nightTools').hidden = state.build;
  $('nightBar').hidden = !state.phase_length;
  if (state.phase_length) $('nightFill').style.width = `${(100 * state.phase_tick) / state.phase_length}%`;
  const locked = new Set(state.locked || []);
  document.querySelectorAll('#tools button.tool[data-cost]').forEach((b) => {
    const lock = locked.has(b.dataset.unit) || locked.has(b.dataset.room);
    b.disabled = lock || +b.dataset.cost > gold + 1e-9;
    b.classList.toggle('locked', lock);
    if (lock) b.title = `Not unlocked yet: see the ${ledgerName()}`;
  });
  if (!state.build && tool) setTool(null);
  if (before && before.build !== state.build && state.build) setTool(null);
  showHints();
  renderGuests();
  renderLedger();
  renderCombos();
  if (before && state.run !== before.run) {
    $('over').hidden = true;
    setLevel(0);
    view.focus(state.cells);
  }
  if (state.over && !before?.over) showOver();
}

function stars(mood) {
  const n = Math.max(0, Math.min(5, Math.round(mood * 5)));
  return '★'.repeat(n) + '☆'.repeat(5 - n);
}

function guestName(kind) {
  return catalog.guests.find((g) => g.id === kind)?.name || kind;
}

function doing(g) {
  if (g.busy) return catalog.services.find((s) => s.id === g.busy)?.name || g.busy;
  let best = null;
  g.needs.forEach((v, i) => {
    const n = catalog.needs[i];
    if (v >= n.urgent && (!best || v > best.v)) best = { v, n };
  });
  return best ? `wants: ${best.n.name.toLowerCase()}` : 'looking around';
}

function renderGuests() {
  const list = $('guestList');
  list.innerHTML = '';
  for (const g of state.guests) {
    const d = document.createElement('div');
    d.className = 'gl' + (g.id === selected ? ' sel' : '');
    const where = state.grid[2] > 1 ? ` · ${look.levelname || 'Level'} ${g.z + 1}` : '';
    d.innerHTML = `<b>${guestName(g.kind)}</b><span class="mood">${stars(g.mood)}</span><span class="doing">${doing(g)}${where}</span>`;
    d.onclick = () => select(g.id);
    list.appendChild(d);
  }
  renderCard();
}

function select(id) {
  selected = selected === id ? null : id;
  // Show the level the guest is on.
  const g = state.guests.find((x) => x.id === selected);
  if (g && g.z !== view.level) setLevel(g.z);
  renderGuests();
}

function renderCard() {
  const card = $('guestCard');
  const g = state.guests.find((x) => x.id === selected);
  if (!g) {
    card.hidden = true;
    if (selected !== null && state) selected = null;
    return;
  }
  card.hidden = false;
  const needs = g.needs.map((v, i) => {
    const n = catalog.needs[i];
    return `<div class="need"><span>${n.name}</span><span class="bar"><i class="${v >= n.urgent ? 'urgent' : ''}" style="width:${Math.min(100, (100 * v) / n.max)}%"></i></span></div>`;
  }).join('');
  card.innerHTML = `<h4>${guestName(g.kind)}</h4><div class="mood">${stars(g.mood)}</div>
    <div class="hint">${doing(g)} · purse ${Math.floor(g.purse)} ${money()}</div>${needs}
    <button id="eject">Show them the door</button>`;
  $('eject').onclick = () => { send({ do: 'eject', guest: g.id }); selected = null; renderGuests(); };
}

// ---------- log ----------

function logEvent(e) {
  let text = '';
  let cls = '';
  switch (e.type) {
    case 'phase': {
      const ph = catalog.phases.find((p) => p.id === e.phase);
      text = `Day ${e.day}: ${ph?.name || e.phase}`;
      cls = 'phase';
      break;
    }
    case 'arrive':
      names.set(e.guest, e.kind);
      text = `${a(guestName(e.kind))} ${look.arrive || 'comes in'}.`;
      break;
    case 'serve': {
      const s = catalog.services.find((x) => x.id === e.service);
      text = `${guestName(names.get(e.guest))}: ${s?.name.toLowerCase() || e.service}, ${e.price} ${money()}.`;
      if ((e.z ?? 0) === view.level) floater(`+${e.price}`, e.x, e.y, e.z);
      break;
    }
    case 'leave':
      text = `${guestName(e.kind)} leaves (${e.why}) ${stars(e.mood)}`;
      cls = e.mood >= 0.5 ? 'good' : 'bad';
      names.delete(e.guest);
      break;
    case 'over':
      text = 'The season is over.';
      cls = 'phase';
      break;
    case 'idle': {
      const ev = e.event;
      const label = ev.label || thingLabel(ev.id);
      if (ev.kind === 'milestone') { text = `★ ${label}`; cls = 'good'; }
      else if (ev.kind === 'upgrade') text = `${ledgerName()}: ${label}`;
      else if (ev.kind === 'unlock') text = `New in the ${ledgerName().toLowerCase()}: ${label}`;
      else if (ev.kind === 'prestige') { text = `${label}!`; cls = 'phase'; }
      else return;
      if (ev.kind !== 'unlock') markLedger();
      break;
    }
    case 'away': {
      // Time away: what it brought, and what the ledger's cap left behind.
      if (Math.abs(e.earned) < 0.5) return;
      const lost = e.earned - e.kept;
      text = `Away ${duration(e.ms / 1000)}: ${e.kept >= 0 ? '+' : ''}${format(e.kept)} ${money()}`;
      if (lost > 0.5) text += ` (${format(lost)} more was over the ${format(e.cap)} the ${ledgerName().toLowerCase()} can hold while you are away)`;
      toast(text);
      cls = 'good';
      break;
    }
    case 'restart':
      text = `A new ${innName()} opens (run ${e.run + 1}).`;
      cls = 'phase';
      break;
    default:
      return;
  }
  const li = document.createElement('li');
  li.textContent = text;
  if (cls) li.className = cls;
  const log = $('log');
  log.prepend(li);
  while (log.children.length > 14) log.lastChild.remove();
}

// The money's name in a sentence: "3 gold", "3 credits".
function money() {
  return catalog.inn.money.toLowerCase();
}

function a(name) {
  return /^[AEIOU]/.test(name) ? `An ${name}` : `A ${name}`;
}

function floater(text, x, y, z = 0) {
  const p = cellCenter(x, y, z).add({ x: 0, y: 3, z: 0 }).project(view.camera);
  const r = $('scene').getBoundingClientRect();
  const d = document.createElement('div');
  d.className = 'floater';
  d.textContent = text;
  d.style.left = `${((p.x + 1) / 2) * r.width}px`;
  d.style.top = `${((1 - p.y) / 2) * r.height}px`;
  $('floaters').appendChild(d);
  setTimeout(() => d.remove(), 1300);
}

let toastTimer = null;
function toast(text) {
  const t = $('toast');
  t.textContent = text;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { t.hidden = true; }, 2600);
}

// ---------- the ledger: the incremental layer ----------

let ledgerTab = false;
let lastWait = performance.now();

function ledgerName() {
  return look.idlename || 'Ledger';
}

function thingLabel(id) {
  const v = state?.idle;
  if (!v) return id;
  for (const k of ['resources', 'generators', 'upgrades', 'actions', 'milestones', 'prestiges']) {
    const t = v[k].find((x) => x.id === id);
    if (t) return t.label;
  }
  return id;
}

function setupLedger() {
  $('tabs').hidden = false;
  $('tabLedger').textContent = ledgerName();
  $('tabs').addEventListener('click', (e) => {
    const b = e.target.closest('button');
    if (!b) return;
    ledgerTab = b.dataset.tab === 'ledger';
    $('tabs').querySelectorAll('button').forEach((x) => x.classList.toggle('on', x === b));
    $('ledger').hidden = !ledgerTab;
    $('innTools').hidden = ledgerTab;
    if (ledgerTab) $('tabLedger').textContent = ledgerName();
    renderLedger();
  });
  $('ledger').addEventListener('click', (e) => {
    const b = e.target.closest('button[data-cmd]');
    if (!b) return;
    const cmd = { cmd: b.dataset.cmd, id: b.dataset.id };
    if (cmd.cmd === 'prestige' && !confirm(`${b.dataset.label}? ${b.dataset.text}`)) return;
    send({ do: 'idle', cmd });
  });
  // The ledger runs in real time while the night is not ticking (the day, a pause, a season
  // that is over); at night it runs with the inn's ticks.
  setInterval(() => {
    const now = performance.now();
    const ms = Math.round(now - lastWait);
    lastWait = now;
    if (timer !== null || ms <= 0) return;
    // A long gap (a tab in the background, a sleeping laptop) is time away: capped.
    if (ms > 60000) { send({ do: 'away', ms: Math.min(ms, catalog.idle.offline_max * 1000) }); return; }
    const res = engine.apply({ do: 'wait', ms: Math.min(ms, catalog.idle.offline_max * 1000) });
    if (!res.error) apply(res, true);
  }, 1000);
}

function markLedger() {
  if (!ledgerTab && !$('tabLedger').querySelector('.dot')) $('tabLedger').insertAdjacentHTML('beforeend', '<span class="dot"></span>');
}

const esc = (t) => String(t ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

function costText(list) {
  return (list || []).map((c) => `${format(c.amount)} ${esc(thingLabel(c.res))}`).join(', ');
}

function renderLedger() {
  const v = state?.idle;
  if (!v || !ledgerTab) return;
  const vis = (x) => x.visible;
  const out = [];
  out.push('<div class="res">');
  for (const r of v.resources.filter(vis)) {
    const rate = +r.rate;
    out.push(`<span>${esc(r.label)}</span><b>${format(r.amount)}</b>`);
    if (Math.abs(rate) > 1e-9) out.push(`<span class="rate">${rate > 0 ? '+' : ''}${format(rate)} a second</span>`);
  }
  out.push('</div>');
  if (v.away_cap != null) out.push(`<p class="hint">While you are away: up to ${format(v.away_cap)} ${money()}, for up to ${duration(v.offline_max)}.</p>`);
  const acts = v.actions.filter(vis);
  if (acts.length) {
    out.push('<h3>Now</h3>');
    for (const a of acts) {
      const on = a.active_for > 0;
      const wait = a.ready_in > 0 ? ` · ready in ${duration(a.ready_in)}` : '';
      out.push(`<button class="row${on ? ' active' : ''}" data-cmd="act" data-id="${esc(a.id)}" ${a.can ? '' : 'disabled'}>
        <span class="n">${esc(a.label)}</span><span class="k">${on ? duration(a.active_for) : ''}</span>
        <span class="t">${esc(a.text)}${wait}</span></button>`);
    }
  }
  const gens = v.generators.filter(vis);
  if (gens.length) {
    out.push('<h3>Hire</h3>');
    for (const g of gens) {
      out.push(`<button class="row" data-cmd="buy" data-id="${esc(g.id)}" ${g.can ? '' : 'disabled'}>
        <span class="n">${esc(g.label)}</span><span class="k">${g.owned}</span>
        <span class="t">${esc(g.text)}</span><span class="c">${costText(g.cost)}</span></button>`);
    }
  }
  const ups = v.upgrades.filter((u) => u.visible && !u.bought);
  if (ups.length) {
    out.push('<h3>Improve</h3>');
    for (const u of ups) {
      out.push(`<button class="row" data-cmd="buy" data-id="${esc(u.id)}" ${u.can ? '' : 'disabled'}>
        <span class="n">${esc(u.label)}</span><span class="k"></span>
        <span class="t">${esc(u.text)}</span><span class="c">${costText(u.cost)}</span></button>`);
    }
  }
  const miles = v.milestones.filter(vis);
  if (miles.length) {
    const done = miles.filter((m) => m.done);
    const next = miles.find((m) => !m.done);
    out.push(`<div class="miles">★ <b>${done.length}/${miles.length}</b>${next ? ` · next: ${esc(next.label)}` : ''}</div>`);
  }
  for (const p of v.prestiges.filter(vis)) {
    out.push(`<button class="row sell" data-cmd="prestige" data-id="${esc(p.id)}" data-label="${esc(p.label)}" data-text="${esc(p.text)}" ${p.can ? '' : 'disabled'}>
      <span class="n">${esc(p.label)}</span><span class="k">${p.count ? '×' + p.count : ''}</span>
      <span class="t">${esc(p.text)}</span><span class="c">gives ${costText(p.gain) || 'nothing yet'}</span></button>`);
  }
  $('ledger').innerHTML = out.join('');
}

function showOver() {
  stopNight();
  const s = state.stats;
  $('overStats').innerHTML = `<table>
    <tr><td>${catalog.inn.money}</td><td>${Math.floor(global(catalog.inn.money))}</td></tr>
    <tr><td>${catalog.inn.reputation}</td><td>${Math.round(global(catalog.inn.reputation))}</td></tr>
    <tr><td>Guests</td><td>${s.arrived}</td></tr>
    <tr><td>Served</td><td>${s.served}</td></tr>
    <tr><td>Left happy</td><td>${s.happy}</td></tr>
    <tr><td>Left angry</td><td>${s.angry}</td></tr>
    <tr><td>Earned</td><td>${Math.floor(s.earned)} ${money()}</td></tr></table>
    ${catalog.idle ? `<p class="small">The ${ledgerName().toLowerCase()} runs on: sell the inn there to start a grander one.</p>` : ''}`;
  $('over').hidden = false;
}

// ---------- input ----------

function setupInput() {
  const canvas = $('scene');
  const pointers = new Map();
  let drag = null;
  canvas.addEventListener('contextmenu', (e) => e.preventDefault());
  canvas.addEventListener('pointerdown', (e) => {
    canvas.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    drag = { x: e.clientX, y: e.clientY, moved: 0, button: e.button, pinch: null };
  });
  canvas.addEventListener('pointermove', (e) => {
    if (!pointers.has(e.pointerId)) {
      if (tool && state?.build && WALL_TOOLS.has(tool.kind)) view.setHoverEdge(view.pickEdge(e.clientX, e.clientY));
      else view.setHover(tool && state?.build ? view.pickCell(e.clientX, e.clientY) : null);
      showTip(e);
      return;
    }
    $('tip').hidden = true;
    const prev = pointers.get(e.pointerId);
    const dx = e.clientX - prev.x;
    const dy = e.clientY - prev.y;
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (!drag) return;
    drag.moved += Math.abs(dx) + Math.abs(dy);
    if (pointers.size === 2) {
      const [p1, p2] = [...pointers.values()];
      const dist = Math.hypot(p1.x - p2.x, p1.y - p2.y);
      const ang = Math.atan2(p2.y - p1.y, p2.x - p1.x);
      if (drag.pinch) {
        view.zoom(drag.pinch.dist / dist);
        view.turn(ang - drag.pinch.ang);
      }
      drag.pinch = { dist, ang };
    } else if (drag.button === 2 || e.shiftKey) {
      view.turn(-dx * 0.006, dy * 0.004);
    } else {
      view.pan(dx, dy);
    }
  });
  const up = (e) => {
    pointers.delete(e.pointerId);
    if (drag && pointers.size === 0) {
      if (drag.moved < 6 && drag.button === 0) click(e.clientX, e.clientY);
      drag = null;
    }
  };
  canvas.addEventListener('pointerleave', () => { $('tip').hidden = true; });
  canvas.addEventListener('pointerup', up);
  canvas.addEventListener('pointercancel', up);
  canvas.addEventListener('wheel', (e) => { e.preventDefault(); view.zoom(e.deltaY > 0 ? 1.1 : 0.9); }, { passive: false });
  window.addEventListener('keydown', (e) => {
    if (e.target.tagName === 'INPUT') return;
    if (e.key === ' ' && !state.build) { e.preventDefault(); setSpeed(speed ? 0 : 1); }
    if (e.key === 'Escape') { setTool(null); $('menu').hidden = true; $('help').hidden = true; }
    if (e.key === 'PageUp' || e.key === '[') setLevel(view.level - 1);
    if (e.key === 'PageDown' || e.key === ']') setLevel(view.level + 1);
    if ((e.key === 'r' || e.key === 'R') && state.build) setTool(tool?.kind === 'turn' ? null : { kind: 'turn' }, $('turnTool'));
    if (e.key === 'q') view.turn(0.15);
    if (e.key === 'e') view.turn(-0.15);
  });
}

function click(x, y) {
  if (!state.build) {
    const g = view.pickGuest(x, y);
    if (g !== null) select(g);
    return;
  }
  if (!tool) return;
  const z = view.level;
  if (WALL_TOOLS.has(tool.kind)) {
    const edge = view.pickEdge(x, y);
    if (!edge) return;
    const [ex, ey, side] = edge;
    if (tool.kind === 'unwall') send({ do: 'unwall', x: ex, y: ey, z, side });
    else send({ do: 'wall', x: ex, y: ey, z, side, door: tool.door });
    return;
  }
  const cell = view.pickCell(x, y);
  if (!cell) return;
  const [cx, cy] = cell;
  if (tool.kind === 'dig') send({ do: 'dig', x: cx, y: cy, z, room: tool.room });
  else if (tool.kind === 'build') send({ do: 'build', x: cx, y: cy, z, unit: tool.unit });
  else if (tool.kind === 'turn') send({ do: 'turn', x: cx, y: cy, z });
  else if (tool.kind === 'paint') send({ do: 'paint', x: cx, y: cy, z, finish: tool.finish });
  else send({ do: 'remove', x: cx, y: cy, z });
}

// What stands under the pointer: its name and the combos that hold for it.
function showTip(e) {
  const tip = $('tip');
  const cell = state && view.pickCell(e.clientX, e.clientY);
  const u = cell && state.units.find((x) => x.x === cell[0] && x.y === cell[1] && x.z === view.level);
  const c = !u && cell && state.build && state.cells.find((x) => x.x === cell[0] && x.y === cell[1] && x.z === view.level);
  if (!u && !c) { tip.hidden = true; return; }
  if (u) {
    const combos = (u.combos || []).map((id) => catalog.combos.find((k) => k.id === id)).filter(Boolean);
    tip.innerHTML = `<b>${esc(unitName(u.unit))}</b>${combos.map((k) => `<div class="on">✦ ${esc(k.name)}</div>`).join('')}`;
  } else {
    const room = catalog.rooms.find((r) => r.id === c.room);
    const fin = c.finish && room?.finishes?.find((f) => f.id === c.finish);
    tip.innerHTML = `<b>${esc(room?.name || c.room)}</b>${fin ? `<div>${esc(fin.name)}</div>` : ''}`;
  }
  const r = $('scene').getBoundingClientRect();
  tip.style.left = `${e.clientX - r.left + 14}px`;
  tip.style.top = `${e.clientY - r.top + 14}px`;
  tip.hidden = false;
}

boot().catch((e) => {
  console.error(e);
  $('loadsub').textContent = `Something went wrong: ${e.message}`;
  window.WavedashHook?.ready();
});
