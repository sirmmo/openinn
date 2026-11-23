// The 3D inn: floors, walls and furniture on a grid of 4-unit cells, animated guests, an overlay
// for the GlassBox layers, and a camera you can drag, turn and zoom. Which models and colours,
// the ruleset says in its `look` block (docs/RULES.md); the rest is the same for every inn.

import * as THREE from 'three';
import { GLTFLoader } from '../vendor/jsm/loaders/GLTFLoader.js';
import { clone as cloneSkinned } from '../vendor/jsm/utils/SkeletonUtils.js';

export const CELL = 4;
const GUEST_SCALE = 2.6;
const TAU = Math.PI * 2;
const LAYER_COLOURS = [0xff9f1c, 0x4fc3f7, 0xe4572e, 0x9ccc65, 0xba68c8];

// A model hint: `name` or `name:x:z:degrees:scale` parts, separated by spaces (x and z in cells
// from the cell's centre), e.g. `table chair:0:-0.3:0 chair:0:0.3:180`.
function parts(hint) {
  return (hint || '').split(/\s+/).filter(Boolean).map((t) => {
    const [name, x, z, deg, k] = t.split(':');
    return { name, x: +x || 0, z: +z || 0, rot: ((+deg || 0) * Math.PI) / 180, scale: +k || 1 };
  });
}

// The `look` block, with neutral defaults.
function readLook(raw = {}) {
  const w = (k) => (raw[k] || '').split(/\s+/).filter(Boolean);
  const num = (v, d) => (v === undefined || Number.isNaN(+v) ? d : +v);
  const hex = (v, d) => (v ? parseInt(v.replace('#', ''), 16) : d);
  const [dir = 'kaykit', ext = 'gltf'] = w('models');
  const [cdir = 'guests', cext = 'glb'] = w('characters');
  const [win, winEvery] = w('window');
  const [decor, decorEvery, dx, dy, dz] = w('decor');
  const [ent, entScale, entDeg] = w('entrance');
  const [sky, low] = w('light');
  const [grid1, grid2] = w('grid');
  return {
    dir, ext, cdir, cext,
    scale: num(w('scale')[0], 1),
    furniture: num(w('furniture')[0], 1),
    floorY: num(w('floor-y')[0], 0),
    wall: w('wall')[0],
    window: win ? { name: win, every: num(winEvery, 3) } : null,
    decor: decor ? { name: decor, every: num(decorEvery, 3), x: num(dx, 0), y: num(dy, 2), z: num(dz, 0.5) } : null,
    entrance: ent ? { name: ent, scale: num(entScale, 1), rot: (num(entDeg, 0) * Math.PI) / 180 } : null,
    background: hex(w('background')[0], 0x17131f),
    ground: w('ground')[0] === 'none' ? null : hex(w('ground')[0], 0x2b2433),
    grid: [hex(grid1, 0x4a3f58), hex(grid2 || grid1, 0x3a3146)],
    edge: hex(w('edge')[0], 0x6b5c80),
    light: [hex(sky, 0xffe6c7), hex(low, 0x2a2238)],
    stars: num(w('stars')[0], 0),
    inner: num(w('inner')[0], 0.55),
    door: w('door')[0],
    level: num(w('level')[0], 7),
  };
}

// How far apart levels are drawn, and which way they go (set from the ruleset by View.load).
let levelStep = 0;

export function levelY(z = 0) {
  return z * levelStep;
}

export function cellCenter(x, y, z = 0) {
  return new THREE.Vector3(x * CELL + CELL / 2, levelY(z), y * CELL + CELL / 2);
}

const SIDES = { n: [0, -1], e: [1, 0], s: [0, 1], w: [-1, 0] };

export class View {
  constructor(canvas) {
    this.canvas = canvas;
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: false });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x17131f);
    this.scene.fog = new THREE.Fog(0x17131f, 70, 140);
    this.camera = new THREE.PerspectiveCamera(40, 1, 0.5, 400);
    this.cam = { target: new THREE.Vector3(24, 0, 12), yaw: 0.35, pitch: 0.95, dist: 52 };

    this.hemi = new THREE.HemisphereLight(0xffe6c7, 0x2a2238, 1.1);
    this.scene.add(this.hemi);
    const sun = new THREE.DirectionalLight(0xfff1dd, 1.6);
    sun.position.set(-20, 40, 30);
    sun.castShadow = true;
    sun.shadow.mapSize.set(2048, 2048);
    Object.assign(sun.shadow.camera, { left: -60, right: 60, top: 60, bottom: -60, near: 1, far: 140 });
    this.sun = sun;
    this.scene.add(sun, sun.target);

    this.templates = {};
    this.clips = {};
    this.cells = new Map(); // "x,y,z" -> { group, room, walls }
    this.units = new Map(); // "x,y,z" -> { obj, unit }
    this.walls = new Map(); // "x,y,z,side" -> { obj, door }
    this.landings = new Map(); // "x,y,z" -> where stairs from the level before come out
    this.level = 0; // the level shown
    this.guests = new Map(); // id -> { obj, mixer, actions, from, to, ... }
    this.overlay = new Map();
    this.layer = null;
    this.hints = new THREE.Group();
    this.scene.add(this.hints);
    this.clock = new THREE.Clock();
    this.tickTime = 0.25; // seconds per engine tick, set by the page
    this.sinceTick = 0;

    const hover = new THREE.Mesh(
      new THREE.PlaneGeometry(CELL * 0.96, CELL * 0.96).rotateX(-Math.PI / 2),
      new THREE.MeshBasicMaterial({ color: 0xfff3b0, transparent: true, opacity: 0.35, depthWrite: false }),
    );
    hover.visible = false;
    hover.renderOrder = 3;
    this.hover = hover;
    this.scene.add(hover);

    this.raycaster = new THREE.Raycaster();
    this.ground = new THREE.Plane(new THREE.Vector3(0, 1, 0), 0);
    this.resize();
    window.addEventListener('resize', () => this.resize());
    this.renderer.setAnimationLoop(() => this.frame());
  }

  async load(catalog, onProgress) {
    this.catalog = catalog;
    const { inn } = catalog;
    const look = readLook(catalog.look);
    this.look = look;
    this.scene.background.set(look.background);
    this.scene.fog.color.set(look.background);
    this.hemi.color.set(look.light[0]);
    this.hemi.groundColor.set(look.light[1]);
    const loader = new GLTFLoader();
    levelStep = look.level * (inn.down ? -1 : 1);
    this.grid = [inn.width, inn.height, inn.levels];
    const names = new Set([look.wall, look.window?.name, look.decor?.name, look.entrance?.name, look.door].filter(Boolean));
    for (const r of catalog.rooms) for (const p of parts(r.floor)) names.add(p.name);
    for (const u of catalog.units) for (const p of parts(u.model)) names.add(p.name);
    const guests = new Set(catalog.guests.map((g) => g.model).filter(Boolean));
    const jobs = [
      ...[...names].map((m) => [m, `models/${look.dir}/${m}.${look.ext}`]),
      ...[...guests].map((m) => [m, `models/${look.cdir}/${m}.${look.cext}`]),
    ];
    let done = 0;
    await Promise.all(jobs.map(async ([name, url]) => {
      try {
        const gltf = await loader.loadAsync(url);
        gltf.scene.traverse((o) => {
          if (o.isMesh) {
            o.castShadow = !catalog.rooms.some((r) => parts(r.floor).some((p) => p.name === name));
            o.receiveShadow = true;
          }
        });
        this.templates[name] = gltf.scene;
        if (gltf.animations.length) this.clips[name] = gltf.animations;
      } catch (e) {
        console.warn('missing model', name, e);
      }
      onProgress?.(++done / jobs.length);
    }));

    this.base = new THREE.Group(); // the ground and the grid, moved to the level shown
    this.scene.add(this.base);
    this.setGrid(this.grid);
    if (look.stars) this.scene.add(starfield(look.stars, inn.width * CELL, inn.height * CELL));

    // The way in (stairs, a door), on the north edge of the entrance cell.
    const [ex, ey] = inn.entrance;
    const door = look.entrance && this.model(look.entrance.name);
    if (door) {
      door.scale.setScalar(look.scale * look.entrance.scale);
      door.rotation.y = look.entrance.rot;
      door.position.copy(cellCenter(ex, ey)).add(new THREE.Vector3(0, 0, -CELL / 2));
      this.scene.add(door);
      this.entranceObj = door;
    }
    this.cam.target.copy(cellCenter(ex, ey + 2));
  }

  // The ground all around the diggable grid (rock, or nothing), and a faint grid over it; the
  // grid's size is the game's (the endless mode's is larger).
  setGrid([gw, gh, levels]) {
    const { look } = this;
    this.grid = [gw, gh, levels];
    this.base.clear();
    const w = gw * CELL;
    const h = gh * CELL;
    if (look.ground !== null) {
      const rock = new THREE.Mesh(
        new THREE.PlaneGeometry(w + 80, h + 80).rotateX(-Math.PI / 2),
        new THREE.MeshStandardMaterial({ color: look.ground, roughness: 1 }),
      );
      rock.position.set(w / 2, -0.15, h / 2);
      rock.receiveShadow = true;
      this.base.add(rock);
    }
    const grid = new THREE.GridHelper(Math.max(w, h), Math.max(gw, gh), look.grid[0], look.grid[1]);
    grid.position.set(Math.max(w, h) / 2, -0.1, Math.max(w, h) / 2);
    this.base.add(grid);
    const edge = new THREE.LineSegments(
      new THREE.EdgesGeometry(new THREE.BoxGeometry(w, 0.01, h)),
      new THREE.LineBasicMaterial({ color: look.edge }),
    );
    edge.position.set(w / 2, -0.08, h / 2);
    this.base.add(edge);
  }

  // Shows one level: the others are hidden, the ground and the camera move to it.
  setLevel(z) {
    this.level = z;
    this.base.position.y = levelY(z);
    if (this.entranceObj) this.entranceObj.visible = z === 0;
    const on = (k) => +k.split(',')[2] === z;
    for (const [k, c] of this.cells) c.group.visible = on(k);
    for (const [k, u] of this.units) u.obj.visible = on(k);
    for (const [k, w] of this.walls) w.obj.visible = on(k);
    for (const [k, m] of this.landings) m.visible = on(k);
    for (const v of this.guests.values()) v.obj.visible = v.data?.z === z;
    this.cam.target.y = levelY(z);
    this.drawOverlay();
  }

  model(name) {
    const t = this.templates[name];
    return t ? t.clone(true) : null;
  }

  // A model hint's parts in one group, scaled; null when none of them is loaded.
  assemble(hint, scale) {
    const g = new THREE.Group();
    for (const p of parts(hint)) {
      const m = this.model(p.name);
      if (!m) continue;
      m.position.set(p.x * CELL, 0, p.z * CELL);
      m.rotation.y = p.rot;
      m.scale.setScalar(scale * p.scale);
      g.add(m);
    }
    return g.children.length ? g : null;
  }

  resize() {
    const w = this.canvas.clientWidth || window.innerWidth;
    const h = this.canvas.clientHeight || window.innerHeight;
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }

  // ---------- the inn ----------

  sync(view) {
    if (view.grid.join() !== this.grid.join()) this.setGrid(view.grid);
    const dug = new Set(view.cells.map((c) => `${c.x},${c.y},${c.z}`));
    const seenCells = new Set();
    for (const c of view.cells) {
      const k = `${c.x},${c.y},${c.z}`;
      seenCells.add(k);
      const walls = this.wallMask(c.x, c.y, c.z, dug);
      const cur = this.cells.get(k);
      if (cur && cur.room === c.room && cur.walls === walls) continue;
      if (cur) this.scene.remove(cur.group);
      const g = this.cellGroup(c, walls);
      g.visible = c.z === this.level;
      this.scene.add(g);
      if (!cur) this.pop(g);
      this.cells.set(k, { group: g, room: c.room, walls });
    }
    for (const [k, v] of this.cells) {
      if (!seenCells.has(k)) {
        this.scene.remove(v.group);
        this.cells.delete(k);
      }
    }
    // Inner walls and doors.
    const seenWalls = new Set();
    for (const w of view.walls) {
      const k = `${w.x},${w.y},${w.z},${w.side}`;
      seenWalls.add(k);
      const cur = this.walls.get(k);
      if (cur && cur.door === w.door) continue;
      if (cur) this.scene.remove(cur.obj);
      const obj = this.innerWall(w);
      obj.visible = w.z === this.level;
      this.scene.add(obj);
      this.pop(obj);
      this.walls.set(k, { obj, door: w.door });
    }
    for (const [k, v] of this.walls) {
      if (!seenWalls.has(k)) {
        this.scene.remove(v.obj);
        this.walls.delete(k);
      }
    }
    // Units, and where stairs come out on the next level.
    const stairs = new Set(this.catalog.units.filter((u) => u.stairs).map((u) => u.id));
    const seen = new Set();
    const landings = new Set();
    for (const u of view.units) {
      const k = `${u.x},${u.y},${u.z}`;
      seen.add(k);
      if (stairs.has(u.unit)) landings.add(`${u.x},${u.y},${u.z + 1}`);
      const cur = this.units.get(k);
      if (cur && cur.unit === u.unit) continue;
      if (cur) this.scene.remove(cur.obj);
      const obj = this.assemble(u.model, this.look.furniture) || new THREE.Mesh(new THREE.BoxGeometry(2, 2, 2), new THREE.MeshStandardMaterial({ color: 0xaa8866 }));
      obj.position.copy(cellCenter(u.x, u.y, u.z));
      obj.rotation.y = this.facing(u.x, u.y, u.z, dug);
      obj.visible = u.z === this.level;
      this.scene.add(obj);
      this.pop(obj);
      this.units.set(k, { obj, unit: u.unit });
    }
    for (const [k, v] of this.units) {
      if (!seen.has(k)) {
        this.scene.remove(v.obj);
        this.units.delete(k);
      }
    }
    for (const k of landings) {
      if (this.landings.has(k)) continue;
      const [x, y, z] = k.split(',').map(Number);
      const m = landingMark();
      m.position.copy(cellCenter(x, y, z)).setY(levelY(z) + 0.12);
      m.visible = z === this.level;
      this.scene.add(m);
      this.landings.set(k, m);
    }
    for (const [k, m] of this.landings) {
      if (!landings.has(k)) {
        this.scene.remove(m);
        this.landings.delete(k);
      }
    }
    this.syncGuests(view);
    this.view = view;
    this.drawOverlay();
  }

  // Bit d set when the neighbour in direction d (0 north, 1 east, 2 south, 3 west) is undug.
  wallMask(x, y, z, dug) {
    let m = 0;
    [[0, -1], [1, 0], [0, 1], [-1, 0]].forEach(([dx, dy], d) => {
      if (!dug.has(`${x + dx},${y + dy},${z}`)) m |= 1 << d;
    });
    return m;
  }

  // An inner wall or door, on the east or south side of its cell.
  innerWall(w) {
    const { look } = this;
    const obj = new THREE.Group();
    const m = this.model(w.door ? look.door || look.wall : look.wall);
    if (m) {
      m.scale.setScalar(look.scale);
      m.scale.y = look.scale * look.inner;
      obj.add(m);
    }
    const [dx, dy] = SIDES[w.side];
    obj.position.copy(cellCenter(w.x, w.y, w.z)).add(new THREE.Vector3((dx * CELL) / 2, 0, (dy * CELL) / 2));
    obj.rotation.y = dx ? Math.PI / 2 : 0;
    return obj;
  }

  cellGroup(c, walls) {
    const g = new THREE.Group();
    const room = this.catalog.rooms.find((r) => r.id === c.room);
    const { look } = this;
    const floor = this.assemble(room?.floor, look.scale);
    if (floor) {
      floor.position.y = look.floorY;
      g.add(floor);
    }
    const tint = new THREE.Mesh(
      new THREE.PlaneGeometry(CELL, CELL).rotateX(-Math.PI / 2),
      new THREE.MeshBasicMaterial({ color: room?.colour || '#888', transparent: true, opacity: 0.13, depthWrite: false }),
    );
    tint.position.y = 0.07;
    g.add(tint);
    const { entrance } = this.catalog.inn;
    const isEntrance = c.x === entrance[0] && c.y === entrance[1] && c.z === 0;
    for (let d = 0; d < 4; d++) {
      if (!(walls & (1 << d))) continue;
      if (isEntrance && d === 0) continue; // the way in
      const back = d === 0 || d === 3; // full height away from the camera, low in front
      const win = look.window && back && (c.x * 5 + c.y * 3 + d) % look.window.every === 0;
      const w = this.model(win ? look.window.name : look.wall);
      if (!w) continue;
      w.scale.setScalar(look.scale);
      w.scale.y = look.scale * (back ? 1 : 0.22);
      const off = [[0, -CELL / 2], [CELL / 2, 0], [0, CELL / 2], [-CELL / 2, 0]][d];
      w.position.set(off[0], 0, off[1]);
      w.rotation.y = d % 2 ? Math.PI / 2 : 0;
      g.add(w);
      const dec = look.decor;
      if (dec && back && d === 0 && (c.x + c.y) % dec.every === 0) {
        const t = this.model(dec.name);
        if (t) {
          t.position.set(dec.x, dec.y, -CELL / 2 + dec.z);
          g.add(t);
        }
      }
    }
    g.position.copy(cellCenter(c.x, c.y, c.z));
    return g;
  }

  // Furniture turns its back to a wall when it has one.
  facing(x, y, z, dug) {
    const order = [[0, -1, 0], [-1, 0, Math.PI / 2], [1, 0, -Math.PI / 2], [0, 1, Math.PI]];
    for (const [dx, dy, ry] of order) if (!dug.has(`${x + dx},${y + dy},${z}`)) return ry;
    return 0;
  }

  pop(obj) {
    obj.userData.pop = 0;
    obj.scale.multiplyScalar(0.01);
    obj.userData.popScale = obj.scale.clone().multiplyScalar(100);
  }

  // ---------- guests ----------

  syncGuests(view) {
    const seen = new Set();
    // Guests sharing a cell stand evenly around it.
    const slots = new Map();
    const count = new Map();
    for (const g of view.guests) {
      const k = `${g.x},${g.y},${g.z}`;
      slots.set(g.id, count.get(k) || 0);
      count.set(k, (count.get(k) || 0) + 1);
    }
    for (const g of view.guests) {
      seen.add(g.id);
      let v = this.guests.get(g.id);
      const spot = this.guestSpot(g, slots.get(g.id), count.get(`${g.x},${g.y},${g.z}`));
      if (!v) {
        v = this.makeGuest(g);
        if (!v) continue;
        v.obj.position.copy(spot);
        v.from = spot.clone();
        this.guests.set(g.id, v);
      } else {
        v.from = v.obj.position.clone();
      }
      // Taking the stairs: no gliding between levels.
      if (v.data && v.data.z !== g.z) v.from = spot.clone();
      v.to = spot;
      v.obj.visible = g.z === this.level;
      v.busy = g.busy;
      v.mood = g.mood;
      v.data = g;
      const moving = v.from.distanceTo(v.to) > 0.2;
      this.play(v, g.busy ? 'sit' : moving ? 'walk' : 'idle');
      if (moving) v.obj.rotation.y = Math.atan2(v.to.x - v.from.x, v.to.z - v.from.z);
      v.ring.material.color.setHSL(0.33 * g.mood, 0.75, 0.5);
    }
    for (const [id, v] of this.guests) {
      if (!seen.has(id)) {
        this.scene.remove(v.obj);
        this.guests.delete(id);
      }
    }
    this.sinceTick = 0;
  }

  makeGuest(g) {
    const kind = this.catalog.guests.find((k) => k.id === g.kind);
    const t = this.templates[kind?.model];
    if (!t) return null;
    const obj = new THREE.Group();
    const body = cloneSkinned(t);
    body.scale.setScalar(GUEST_SCALE);
    obj.add(body);
    const ring = new THREE.Mesh(
      new THREE.RingGeometry(0.75, 1.0, 24).rotateX(-Math.PI / 2),
      new THREE.MeshBasicMaterial({ color: 0x66cc66, transparent: true, opacity: 0.85, depthWrite: false }),
    );
    ring.position.y = 0.12;
    obj.add(ring);
    obj.userData.guest = g.id;
    body.traverse((o) => { o.userData.guest = g.id; });
    this.scene.add(obj);
    const mixer = new THREE.AnimationMixer(body);
    const actions = {};
    for (const clip of this.clips[kind.model] || []) actions[clip.name] = mixer.clipAction(clip);
    return { obj, body, ring, mixer, actions, current: null };
  }

  play(v, name) {
    if (v.current === name) return;
    const next = v.actions[name] || v.actions.idle;
    if (!next) return;
    next.reset().fadeIn(0.2).play();
    if (v.current && v.actions[v.current]) v.actions[v.current].fadeOut(0.2);
    v.current = name;
  }

  // Where a guest stands: spread around the cell so several do not overlap; next to the unit
  // when being served.
  guestSpot(g, slot, of) {
    const p = cellCenter(g.x, g.y, g.z);
    const a = (slot / of) * TAU + (g.x * 7 + g.y * 3) * 0.7;
    const r = of > 1 || g.busy ? 1.25 : 0.5;
    p.x += Math.cos(a) * r;
    p.z += Math.sin(a) * r;
    return p;
  }

  // ---------- overlay, hints, picking ----------

  setLayer(id) {
    this.layer = id;
    this.drawOverlay();
  }

  drawOverlay() {
    for (const m of this.overlay.values()) this.scene.remove(m);
    this.overlay.clear();
    if (!this.layer || !this.view) return;
    const li = this.view.layers.indexOf(this.layer);
    const def = this.catalog.layers.find((l) => l.id === this.layer);
    const max = def?.max || 20;
    const colour = def?.colour ? new THREE.Color(def.colour) : LAYER_COLOURS[this.catalog.layers.indexOf(def) % LAYER_COLOURS.length];
    for (const c of this.view.cells) {
      if (c.z !== this.level) continue;
      const v = c.maps[li] || 0;
      if (v <= 0.01) continue;
      const m = new THREE.Mesh(
        new THREE.PlaneGeometry(CELL, CELL).rotateX(-Math.PI / 2),
        new THREE.MeshBasicMaterial({ color: colour, transparent: true, opacity: Math.min(0.7, 0.12 + (2.5 * v) / max), depthWrite: false }),
      );
      m.position.copy(cellCenter(c.x, c.y, c.z)).add(new THREE.Vector3(0, 0.2, 0));
      m.renderOrder = 2;
      this.scene.add(m);
      this.overlay.set(`${c.x},${c.y},${c.z}`, m);
    }
  }

  // Glowing squares where the current tool can go.
  showHints(cells, colour = 0xfff3b0) {
    this.hints.clear();
    const geo = new THREE.PlaneGeometry(CELL * 0.9, CELL * 0.9).rotateX(-Math.PI / 2);
    const mat = new THREE.MeshBasicMaterial({ color: colour, transparent: true, opacity: 0.22, depthWrite: false });
    for (const [x, y] of cells) {
      const m = new THREE.Mesh(geo, mat);
      m.position.copy(cellCenter(x, y, this.level)).add(new THREE.Vector3(0, 0.15, 0));
      m.renderOrder = 2;
      this.hints.add(m);
    }
  }

  // Glowing bars on the edges where a wall can go (or come down): [x, y, side] on this level.
  showEdgeHints(edges, colour = 0xfff3b0) {
    this.hints.clear();
    const mat = new THREE.MeshBasicMaterial({ color: colour, transparent: true, opacity: 0.45, depthWrite: false });
    const geo = new THREE.BoxGeometry(CELL * 0.8, 0.25, 0.35);
    for (const [x, y, side] of edges) {
      const m = new THREE.Mesh(geo, mat);
      this.placeEdge(m, x, y, side);
      m.renderOrder = 2;
      this.hints.add(m);
    }
  }

  placeEdge(obj, x, y, side) {
    const [dx, dy] = SIDES[side];
    obj.position.copy(cellCenter(x, y, this.level)).add(new THREE.Vector3((dx * CELL) / 2, 0.2, (dy * CELL) / 2));
    obj.rotation.y = dx ? Math.PI / 2 : 0;
  }

  setHover(cell) {
    this.hover.visible = !!cell;
    this.hover.scale.set(1, 1, 1);
    this.hover.rotation.y = 0;
    if (cell) this.hover.position.copy(cellCenter(cell[0], cell[1], this.level)).add(new THREE.Vector3(0, 0.18, 0));
  }

  setHoverEdge(edge) {
    this.hover.visible = !!edge;
    if (!edge) return;
    this.placeEdge(this.hover, edge[0], edge[1], edge[2]);
    this.hover.scale.set(0.9, 1, 0.12);
  }

  ray(clientX, clientY) {
    const r = this.canvas.getBoundingClientRect();
    const ndc = new THREE.Vector2(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
    this.raycaster.setFromCamera(ndc, this.camera);
    return this.raycaster;
  }

  groundPoint(clientX, clientY) {
    const p = new THREE.Vector3();
    this.ground.constant = -levelY(this.level);
    return this.ray(clientX, clientY).ray.intersectPlane(this.ground, p) ? p : null;
  }

  pickCell(clientX, clientY) {
    const p = this.groundPoint(clientX, clientY);
    if (!p) return null;
    const x = Math.floor(p.x / CELL);
    const y = Math.floor(p.z / CELL);
    const [width, height] = this.grid;
    return x >= 0 && y >= 0 && x < width && y < height ? [x, y] : null;
  }

  // The cell side nearest the pointer: [x, y, side].
  pickEdge(clientX, clientY) {
    const cell = this.pickCell(clientX, clientY);
    if (!cell) return null;
    const p = this.groundPoint(clientX, clientY);
    const fx = p.x / CELL - cell[0];
    const fy = p.z / CELL - cell[1];
    const d = { n: fy, s: 1 - fy, w: fx, e: 1 - fx };
    const side = Object.keys(d).reduce((a, b) => (d[a] <= d[b] ? a : b));
    return [cell[0], cell[1], side];
  }

  pickGuest(clientX, clientY) {
    const objs = [...this.guests.values()].map((v) => v.obj);
    const hit = this.ray(clientX, clientY).intersectObjects(objs, true)[0];
    return hit ? hit.object.userData.guest ?? null : null;
  }

  // ---------- camera ----------

  // Looks at the dug part of the inn, from far enough to see all of it.
  focus(cells) {
    cells = cells.filter((c) => (c.z ?? 0) === this.level);
    if (!cells.length) return;
    let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
    for (const c of cells) {
      x0 = Math.min(x0, c.x); y0 = Math.min(y0, c.y);
      x1 = Math.max(x1, c.x); y1 = Math.max(y1, c.y);
    }
    this.cam.target.set(((x0 + x1 + 1) / 2) * CELL, levelY(this.level), ((y0 + y1 + 1) / 2) * CELL);
    this.cam.dist = Math.min(110, Math.max(26, Math.max(x1 - x0 + 1, y1 - y0 + 1) * CELL * 1.15 + 8));
  }

  pan(dx, dy) {
    const k = this.cam.dist / 600;
    const s = Math.sin(this.cam.yaw);
    const c = Math.cos(this.cam.yaw);
    this.cam.target.x -= (dx * c + dy * s) * k;
    this.cam.target.z -= (-dx * s + dy * c) * k;
  }

  turn(dyaw, dpitch = 0) {
    this.cam.yaw += dyaw;
    this.cam.pitch = Math.min(1.45, Math.max(0.45, this.cam.pitch + dpitch));
  }

  zoom(f) {
    this.cam.dist = Math.min(130, Math.max(14, this.cam.dist * f));
  }

  updateCamera() {
    const { target, yaw, pitch, dist } = this.cam;
    this.camera.position.set(
      target.x + Math.sin(yaw) * Math.cos(pitch) * dist,
      target.y + Math.sin(pitch) * dist,
      target.z + Math.cos(yaw) * Math.cos(pitch) * dist,
    );
    this.camera.lookAt(target);
    this.sun.position.copy(target).add(new THREE.Vector3(-20, 40, 30));
    this.sun.target.position.copy(target);
  }

  frame() {
    const dt = Math.min(this.clock.getDelta(), 0.1);
    this.sinceTick += dt;
    const t = Math.min(1, this.sinceTick / this.tickTime);
    for (const v of this.guests.values()) {
      v.obj.position.lerpVectors(v.from, v.to, t);
      v.mixer.update(dt);
    }
    const popping = [...this.cells.values()].map((c) => c.group)
      .concat([...this.units.values()].map((u) => u.obj), [...this.walls.values()].map((w) => w.obj));
    for (const g of popping) {
      if (g.userData.pop === undefined) continue;
      g.userData.pop = Math.min(1, g.userData.pop + dt * 4);
      const e = 1 - Math.pow(1 - g.userData.pop, 3);
      g.scale.copy(g.userData.popScale).multiplyScalar(Math.max(0.01, e));
      if (g.userData.pop >= 1) delete g.userData.pop;
    }
    this.updateCamera();
    this.renderer.render(this.scene, this.camera);
  }
}

// Stars below and around the grid, for inns with nothing underneath.
function starfield(n, w, h) {
  const pos = new Float32Array(n * 3);
  let seed = 7;
  const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
  for (let i = 0; i < n; i++) {
    pos[i * 3] = w / 2 + (rnd() - 0.5) * 260;
    pos[i * 3 + 1] = -8 - rnd() * 70;
    pos[i * 3 + 2] = h / 2 + (rnd() - 0.5) * 260;
  }
  const geo = new THREE.BufferGeometry();
  geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
  return new THREE.Points(geo, new THREE.PointsMaterial({ color: 0xdfe8ff, size: 0.5, sizeAttenuation: true, fog: false }));
}

// Where stairs come out on the next level: a ring on the floor.
function landingMark() {
  const m = new THREE.Mesh(
    new THREE.RingGeometry(1.0, 1.4, 4, 1).rotateX(-Math.PI / 2).rotateY(Math.PI / 4),
    new THREE.MeshBasicMaterial({ color: 0xfff3b0, transparent: true, opacity: 0.6, depthWrite: false }),
  );
  m.renderOrder = 2;
  return m;
}
