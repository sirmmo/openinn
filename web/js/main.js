// The page: loads the engine and a ruleset, draws the inn, turns clicks into commands, runs the
// night, keeps the log and saves the game (its seed and commands) in localStorage.

import { Engine, fetchRuleset } from './engine.js';
import { View, cellCenter } from './view.js';

const $ = (id) => document.getElementById(id);
const params = new URLSearchParams(location.search);
const GAME = params.get('game') || 'dungeon-inn';
const SAVE_KEY = `openinn:${GAME}`;
const TICK_MS = 250; // a tick at normal speed

let engine, files, catalog, view, state;
let tool = null; // { kind: 'dig', room } | { kind: 'build', unit } | { kind: 'remove' }
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
  const seed = params.get('seed') || saved?.seed || newSeed();
  const commands = !params.get('seed') || params.get('seed') === saved?.seed ? saved?.commands : null;
  let res = engine.start(files, seed, commands);
  if (res.error && commands) {
    console.warn('the save does not replay:', res.error);
    res = engine.start(files, seed);
  }
  if (res.error) {
    $('loadsub').textContent = res.error;
    return;
  }
  catalog = res.catalog;
  $('loadsub').textContent = 'Lighting the torches…';
  await view.load(catalog, (k) => { $('loadbar').style.width = `${Math.round(k * 100)}%`; });
  setupHud();
  apply(res, false);
  view.frame(state.cells);
  $('loading').hidden = true;
  $('hud').hidden = false;
  // For the console and for tests: window.openinn.send({ do: 'dig', x: 6, y: 1, room: 'hall' }).
  window.openinn = { send, state: () => state, view, setSpeed, catalog };
  if (!localStorage.getItem('openinn:seen-help')) {
    $('help').hidden = false;
    localStorage.setItem('openinn:seen-help', '1');
  }
}

function newSeed() {
  const w = ['mimic', 'kobold', 'lantern', 'barrel', 'goblet', 'torch', 'cellar', 'dragon', 'tankard', 'rune'];
  return `${w[Math.floor(Math.random() * w.length)]}-${Math.floor(Math.random() * 9000 + 1000)}`;
}

function restart(seed = newSeed()) {
  stopNight();
  localStorage.removeItem(SAVE_KEY);
  const u = new URL(location.href);
  u.searchParams.delete('seed');
  history.replaceState(null, '', u);
  const res = engine.start(files, seed);
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
  apply(res, false);
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
    try { localStorage.setItem(SAVE_KEY, JSON.stringify(s)); } catch { /* full: keep playing */ }
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

function setupHud() {
  $('innName').textContent = catalog.inn.name;
  document.title = `OpenInn: ${catalog.inn.name}`;
  const digs = $('digButtons');
  for (const r of catalog.rooms) {
    const b = toolButton(r.name, r.cost, `<span class="sw" style="background:${r.colour}"></span>`);
    b.onclick = () => setTool(tool?.kind === 'dig' && tool.room === r.id ? null : { kind: 'dig', room: r.id }, b);
    b.dataset.cost = r.cost;
    digs.appendChild(b);
  }
  const units = $('unitButtons');
  for (const u of catalog.units) {
    const where = u.rooms.map((id) => catalog.rooms.find((r) => r.id === id)?.name).join(', ');
    const b = toolButton(u.name, u.cost, '');
    b.title = `${u.name}: in ${where || 'any room'}${u.seats ? `, seats ${u.seats}` : ''}`;
    b.onclick = () => setTool(tool?.kind === 'build' && tool.unit === u.id ? null : { kind: 'build', unit: u.id }, b);
    b.dataset.cost = u.cost;
    units.appendChild(b);
  }
  $('removeTool').onclick = () => setTool(tool?.kind === 'remove' ? null : { kind: 'remove' }, $('removeTool'));
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
  $('mLink').onclick = async () => {
    $('menu').hidden = true;
    const u = new URL(location.href);
    u.searchParams.set('seed', engine.save().seed);
    try { await navigator.clipboard.writeText(u.href); toast('Link copied: same seed, same guests'); } catch { prompt('Copy this link:', u.href); }
  };
  $('mSave').onclick = () => {
    $('menu').hidden = true;
    const blob = new Blob([JSON.stringify({ game: GAME, ...engine.save() })], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `openinn-${GAME}-${engine.save().seed}.json`;
    a.click();
  };
  $('overNew').onclick = () => restart();
  $('overLook').onclick = () => { $('over').hidden = true; };

  setupInput();
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

function showHints() {
  if (!state || !tool || !state.build) return view.showHints([]);
  const dug = new Map(state.cells.map((c) => [`${c.x},${c.y}`, c]));
  const used = new Set(state.units.map((u) => `${u.x},${u.y}`));
  const { width, height } = catalog.inn;
  const out = [];
  if (tool.kind === 'dig') {
    for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
      if (dug.has(`${x},${y}`)) continue;
      if ([[1, 0], [-1, 0], [0, 1], [0, -1]].some(([dx, dy]) => dug.has(`${x + dx},${y + dy}`))) out.push([x, y]);
    }
  } else if (tool.kind === 'build') {
    const u = catalog.units.find((k) => k.id === tool.unit);
    for (const c of state.cells) {
      if (!used.has(`${c.x},${c.y}`) && (!u.rooms.length || u.rooms.includes(c.room))) out.push([c.x, c.y]);
    }
  } else {
    for (const u of state.units) out.push([u.x, u.y]);
  }
  view.showHints(out, tool.kind === 'remove' ? 0xff8a65 : 0xfff3b0);
}

function global(id) {
  return state.globals.find(([k]) => k === id)?.[1] ?? 0;
}

function render(before) {
  const ph = catalog.phases.find((p) => p.id === state.phase);
  $('dayLabel').textContent = `Day ${state.day}`;
  $('phaseLabel').textContent = ph?.name || state.phase;
  const gold = global(catalog.inn.money);
  $('gold').textContent = Math.floor(gold);
  $('rep').textContent = Math.round(global(catalog.inn.reputation));
  $('guestCount').textContent = state.guests.length;
  $('buildTools').hidden = !state.build;
  $('nightTools').hidden = state.build;
  $('nightBar').hidden = !state.phase_length;
  if (state.phase_length) $('nightFill').style.width = `${(100 * state.phase_tick) / state.phase_length}%`;
  document.querySelectorAll('#tools button.tool[data-cost]').forEach((b) => { b.disabled = +b.dataset.cost > gold + 1e-9; });
  if (!state.build && tool) setTool(null);
  if (before && before.build !== state.build && state.build) setTool(null);
  showHints();
  renderGuests();
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
    d.innerHTML = `<b>${guestName(g.kind)}</b><span class="mood">${stars(g.mood)}</span><span class="doing">${doing(g)}</span>`;
    d.onclick = () => select(g.id);
    list.appendChild(d);
  }
  renderCard();
}

function select(id) {
  selected = selected === id ? null : id;
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
    <div class="hint">${doing(g)} · purse ${Math.floor(g.purse)} gold</div>${needs}
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
      text = `${a(guestName(e.kind))} comes down the stairs.`;
      break;
    case 'serve': {
      const s = catalog.services.find((x) => x.id === e.service);
      text = `${guestName(names.get(e.guest))}: ${s?.name.toLowerCase() || e.service}, ${e.price} gold.`;
      floater(`+${e.price}`, e.x, e.y);
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

function a(name) {
  return /^[AEIOU]/.test(name) ? `An ${name}` : `A ${name}`;
}

function floater(text, x, y) {
  const p = cellCenter(x, y).setY(3).project(view.camera);
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

function showOver() {
  stopNight();
  const s = state.stats;
  $('overStats').innerHTML = `<table>
    <tr><td>Gold</td><td>${Math.floor(global(catalog.inn.money))}</td></tr>
    <tr><td>Reputation</td><td>${Math.round(global(catalog.inn.reputation))}</td></tr>
    <tr><td>Guests</td><td>${s.arrived}</td></tr>
    <tr><td>Served</td><td>${s.served}</td></tr>
    <tr><td>Left happy</td><td>${s.happy}</td></tr>
    <tr><td>Left angry</td><td>${s.angry}</td></tr>
    <tr><td>Earned</td><td>${Math.floor(s.earned)} gold</td></tr></table>`;
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
      view.setHover(tool && state?.build ? view.pickCell(e.clientX, e.clientY) : null);
      return;
    }
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
  canvas.addEventListener('pointerup', up);
  canvas.addEventListener('pointercancel', up);
  canvas.addEventListener('wheel', (e) => { e.preventDefault(); view.zoom(e.deltaY > 0 ? 1.1 : 0.9); }, { passive: false });
  window.addEventListener('keydown', (e) => {
    if (e.target.tagName === 'INPUT') return;
    if (e.key === ' ' && !state.build) { e.preventDefault(); setSpeed(speed ? 0 : 1); }
    if (e.key === 'Escape') { setTool(null); $('menu').hidden = true; $('help').hidden = true; }
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
  const cell = view.pickCell(x, y);
  if (!cell || !tool) return;
  const [cx, cy] = cell;
  if (tool.kind === 'dig') send({ do: 'dig', x: cx, y: cy, room: tool.room });
  else if (tool.kind === 'build') send({ do: 'build', x: cx, y: cy, unit: tool.unit });
  else send({ do: 'remove', x: cx, y: cy });
}

boot().catch((e) => {
  console.error(e);
  $('loadsub').textContent = `Something went wrong: ${e.message}`;
});
