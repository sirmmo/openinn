// The 3D inn: KayKit Dungeon floors, walls and furniture on a grid of 4-unit cells, Kenney
// Mini Characters as guests, an overlay for the GlassBox layers, and a camera you can drag,
// turn and zoom.

import * as THREE from 'three';
import { GLTFLoader } from '../vendor/jsm/loaders/GLTFLoader.js';
import { clone as cloneSkinned } from '../vendor/jsm/utils/SkeletonUtils.js';

export const CELL = 4;
const GUEST_SCALE = 2.6;
const TAU = Math.PI * 2;
const LAYER_COLOURS = { Noise: 0xff9f1c, Heat: 0xe4572e, Dirt: 0x7a5c3a };

export function cellCenter(x, y) {
  return new THREE.Vector3(x * CELL + CELL / 2, 0, y * CELL + CELL / 2);
}

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

    this.scene.add(new THREE.HemisphereLight(0xffe6c7, 0x2a2238, 1.1));
    const sun = new THREE.DirectionalLight(0xfff1dd, 1.6);
    sun.position.set(-20, 40, 30);
    sun.castShadow = true;
    sun.shadow.mapSize.set(2048, 2048);
    Object.assign(sun.shadow.camera, { left: -60, right: 60, top: 60, bottom: -60, near: 1, far: 140 });
    this.sun = sun;
    this.scene.add(sun, sun.target);

    this.templates = {};
    this.clips = {};
    this.cells = new Map(); // "x,y" -> { group, room }
    this.units = new Map(); // "x,y" -> { obj, unit }
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
    const loader = new GLTFLoader();
    const kay = new Set(['wall', 'wall_half', 'stairs', 'torch_mounted', 'candle_lit']);
    for (const r of catalog.rooms) if (r.floor) kay.add(r.floor);
    for (const u of catalog.units) if (u.model) kay.add(u.model);
    const guests = new Set(catalog.guests.map((g) => g.model).filter(Boolean));
    const jobs = [
      ...[...kay].map((m) => [m, `models/kaykit/${m}.gltf`]),
      ...[...guests].map((m) => [m, `models/guests/${m}.glb`]),
    ];
    let done = 0;
    await Promise.all(jobs.map(async ([name, url]) => {
      try {
        const gltf = await loader.loadAsync(url);
        gltf.scene.traverse((o) => {
          if (o.isMesh) {
            o.castShadow = !name.startsWith('floor');
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

    // Rock all around the diggable grid, and a faint grid over it.
    const w = inn.width * CELL;
    const h = inn.height * CELL;
    const rock = new THREE.Mesh(
      new THREE.PlaneGeometry(w + 80, h + 80).rotateX(-Math.PI / 2),
      new THREE.MeshStandardMaterial({ color: 0x2b2433, roughness: 1 }),
    );
    rock.position.set(w / 2, -0.15, h / 2);
    rock.receiveShadow = true;
    this.scene.add(rock);
    const grid = new THREE.GridHelper(Math.max(w, h), Math.max(inn.width, inn.height), 0x4a3f58, 0x3a3146);
    grid.position.set(Math.max(w, h) / 2, -0.1, Math.max(w, h) / 2);
    grid.scale.set(1, 1, 1);
    this.scene.add(grid);
    const edge = new THREE.LineSegments(
      new THREE.EdgesGeometry(new THREE.BoxGeometry(w, 0.01, h)),
      new THREE.LineBasicMaterial({ color: 0x6b5c80 }),
    );
    edge.position.set(w / 2, -0.08, h / 2);
    this.scene.add(edge);

    // The stairs down from the level above, north of the entrance.
    const [ex, ey] = inn.entrance;
    const stairs = this.model('stairs');
    if (stairs) {
      stairs.scale.setScalar(0.8);
      stairs.rotation.y = Math.PI;
      stairs.position.copy(cellCenter(ex, ey)).add(new THREE.Vector3(0, 0, -CELL / 2));
      this.scene.add(stairs);
    }
    this.cam.target.copy(cellCenter(ex, ey + 2));
  }

  model(name) {
    const t = this.templates[name];
    return t ? t.clone(true) : null;
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
    const dug = new Set(view.cells.map((c) => `${c.x},${c.y}`));
    for (const c of view.cells) {
      const k = `${c.x},${c.y}`;
      const walls = this.wallMask(c.x, c.y, dug);
      const cur = this.cells.get(k);
      if (cur && cur.room === c.room && cur.walls === walls) continue;
      if (cur) this.scene.remove(cur.group);
      const g = this.cellGroup(c, walls);
      this.scene.add(g);
      if (!cur) this.pop(g);
      this.cells.set(k, { group: g, room: c.room, walls });
    }
    // Units.
    const seen = new Set();
    for (const u of view.units) {
      const k = `${u.x},${u.y}`;
      seen.add(k);
      const cur = this.units.get(k);
      if (cur && cur.unit === u.unit) continue;
      if (cur) this.scene.remove(cur.obj);
      const obj = this.model(u.model) || new THREE.Mesh(new THREE.BoxGeometry(2, 2, 2), new THREE.MeshStandardMaterial({ color: 0xaa8866 }));
      obj.position.copy(cellCenter(u.x, u.y));
      obj.rotation.y = this.facing(u.x, u.y, dug);
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
    this.syncGuests(view);
    this.view = view;
    this.drawOverlay();
  }

  // Bit d set when the neighbour in direction d (0 north, 1 east, 2 south, 3 west) is rock.
  wallMask(x, y, dug) {
    let m = 0;
    [[0, -1], [1, 0], [0, 1], [-1, 0]].forEach(([dx, dy], d) => {
      if (!dug.has(`${x + dx},${y + dy}`)) m |= 1 << d;
    });
    return m;
  }

  cellGroup(c, walls) {
    const g = new THREE.Group();
    const room = this.catalog.rooms.find((r) => r.id === c.room);
    const floor = this.model(room?.floor);
    if (floor) {
      if (room.floor.includes('small')) floor.scale.set(2, 1, 2);
      g.add(floor);
    }
    const tint = new THREE.Mesh(
      new THREE.PlaneGeometry(CELL, CELL).rotateX(-Math.PI / 2),
      new THREE.MeshBasicMaterial({ color: room?.colour || '#888', transparent: true, opacity: 0.13, depthWrite: false }),
    );
    tint.position.y = 0.07;
    g.add(tint);
    const { entrance } = this.catalog.inn;
    const isEntrance = c.x === entrance[0] && c.y === entrance[1];
    for (let d = 0; d < 4; d++) {
      if (!(walls & (1 << d))) continue;
      if (isEntrance && d === 0) continue; // the stairs come in here
      const w = this.model('wall');
      if (!w) continue;
      const back = d === 0 || d === 3; // full height away from the camera, low in front
      w.scale.y = back ? 1 : 0.22;
      const off = [[0, -CELL / 2], [CELL / 2, 0], [0, CELL / 2], [-CELL / 2, 0]][d];
      w.position.set(off[0], 0, off[1]);
      w.rotation.y = d % 2 ? Math.PI / 2 : 0;
      g.add(w);
      if (back && d === 0 && (c.x + c.y) % 3 === 0) {
        const t = this.model('torch_mounted');
        if (t) {
          t.position.set(0.8, 2.6, -CELL / 2 + 0.5);
          g.add(t);
        }
      }
    }
    g.position.copy(cellCenter(c.x, c.y));
    return g;
  }

  // Furniture turns its back to a wall when it has one.
  facing(x, y, dug) {
    const order = [[0, -1, 0], [-1, 0, Math.PI / 2], [1, 0, -Math.PI / 2], [0, 1, Math.PI]];
    for (const [dx, dy, ry] of order) if (!dug.has(`${x + dx},${y + dy}`)) return ry;
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
    for (const g of view.guests) {
      seen.add(g.id);
      let v = this.guests.get(g.id);
      const spot = this.guestSpot(g, view);
      if (!v) {
        v = this.makeGuest(g);
        if (!v) continue;
        v.obj.position.copy(spot);
        v.from = spot.clone();
        this.guests.set(g.id, v);
      } else {
        v.from = v.obj.position.clone();
      }
      v.to = spot;
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
  guestSpot(g, view) {
    const p = cellCenter(g.x, g.y);
    const a = ((g.id * 2654435761) % 997) / 997 * TAU;
    const r = g.busy ? 1.3 : 1.1;
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
    const max = this.catalog.layers.find((l) => l.id === this.layer)?.max || 20;
    for (const c of this.view.cells) {
      const v = c.maps[li] || 0;
      if (v <= 0.01) continue;
      const m = new THREE.Mesh(
        new THREE.PlaneGeometry(CELL, CELL).rotateX(-Math.PI / 2),
        new THREE.MeshBasicMaterial({ color: LAYER_COLOURS[this.layer] || 0xffffff, transparent: true, opacity: Math.min(0.75, 0.1 + v / max), depthWrite: false }),
      );
      m.position.copy(cellCenter(c.x, c.y)).setY(0.2);
      m.renderOrder = 2;
      this.scene.add(m);
      this.overlay.set(`${c.x},${c.y}`, m);
    }
  }

  // Glowing squares where the current tool can go.
  showHints(cells, colour = 0xfff3b0) {
    this.hints.clear();
    const geo = new THREE.PlaneGeometry(CELL * 0.9, CELL * 0.9).rotateX(-Math.PI / 2);
    const mat = new THREE.MeshBasicMaterial({ color: colour, transparent: true, opacity: 0.22, depthWrite: false });
    for (const [x, y] of cells) {
      const m = new THREE.Mesh(geo, mat);
      m.position.copy(cellCenter(x, y)).setY(0.15);
      m.renderOrder = 2;
      this.hints.add(m);
    }
  }

  setHover(cell) {
    this.hover.visible = !!cell;
    if (cell) this.hover.position.copy(cellCenter(cell[0], cell[1])).setY(0.18);
  }

  ray(clientX, clientY) {
    const r = this.canvas.getBoundingClientRect();
    const ndc = new THREE.Vector2(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
    this.raycaster.setFromCamera(ndc, this.camera);
    return this.raycaster;
  }

  pickCell(clientX, clientY) {
    const p = new THREE.Vector3();
    if (!this.ray(clientX, clientY).ray.intersectPlane(this.ground, p)) return null;
    const x = Math.floor(p.x / CELL);
    const y = Math.floor(p.z / CELL);
    const { width, height } = this.catalog.inn;
    return x >= 0 && y >= 0 && x < width && y < height ? [x, y] : null;
  }

  pickGuest(clientX, clientY) {
    const objs = [...this.guests.values()].map((v) => v.obj);
    const hit = this.ray(clientX, clientY).intersectObjects(objs, true)[0];
    return hit ? hit.object.userData.guest ?? null : null;
  }

  // ---------- camera ----------

  // Looks at the dug part of the inn, from far enough to see all of it.
  frame(cells) {
    if (!cells.length) return;
    let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
    for (const c of cells) {
      x0 = Math.min(x0, c.x); y0 = Math.min(y0, c.y);
      x1 = Math.max(x1, c.x); y1 = Math.max(y1, c.y);
    }
    this.cam.target.set(((x0 + x1 + 1) / 2) * CELL, 0, ((y0 + y1 + 1) / 2) * CELL);
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
    for (const g of [...this.cells.values()].map((c) => c.group).concat([...this.units.values()].map((u) => u.obj))) {
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
