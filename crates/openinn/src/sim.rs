//! The inn: a grid of dug cells on one or more levels (each cell a GlassBox place, joined to its
//! neighbours unless a wall stands between them, and to the next level by stairs), units
//! standing on them, guests walking between them, and phases that open and close the doors.
//!
//! Everything is deterministic: the seed, the mode and the list of commands give the same game
//! on every machine, so a save is just `{ seed, mode, commands }`.

use glassbox::{emit, step, Scope, State};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

use openidle::{Num, World};

use crate::script::{FeedEvent, Lockable, Ruleset, Test};

/// How a game runs: a season of the ruleset's nights, or on and on with more room to grow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Season,
    Endless,
}

/// A side of a cell: `n`, `e`, `s` or `w`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Side {
    #[serde(rename = "n")]
    N,
    #[serde(rename = "e")]
    E,
    #[serde(rename = "s")]
    S,
    #[serde(rename = "w")]
    W,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::N, Side::E, Side::S, Side::W];
    pub fn delta(self) -> (i64, i64) {
        match self {
            Side::N => (0, -1),
            Side::E => (1, 0),
            Side::S => (0, 1),
            Side::W => (-1, 0),
        }
    }
    pub fn letter(self) -> &'static str {
        match self {
            Side::N => "n",
            Side::E => "e",
            Side::S => "s",
            Side::W => "w",
        }
    }
    /// A quarter turn clockwise (north becomes east).
    pub fn cw(self) -> Side {
        match self {
            Side::N => Side::E,
            Side::E => Side::S,
            Side::S => Side::W,
            Side::W => Side::N,
        }
    }
}

/// A cell of a building plan (see `Command::Plan`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanCell {
    pub x: usize,
    pub y: usize,
    #[serde(default)]
    pub z: usize,
    pub room: String,
}

/// An inner wall or door of a building plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanWall {
    pub x: usize,
    pub y: usize,
    #[serde(default)]
    pub z: usize,
    pub side: Side,
    #[serde(default)]
    pub door: bool,
}

/// Where stairs stand in a building plan: they join (x, y, z) to (x, y, z + 1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanAt {
    pub x: usize,
    pub y: usize,
    #[serde(default)]
    pub z: usize,
}

/// What the player can do. `z` is the level (0: the entrance's; it may be left out).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "lowercase")]
pub enum Command {
    /// Dig a cell next to the inn (or under or over stairs) as a room kind (in a building phase).
    Dig {
        x: usize,
        y: usize,
        #[serde(default)]
        z: usize,
        room: String,
    },
    /// Put a unit on a dug cell (in a building phase).
    Build {
        x: usize,
        y: usize,
        #[serde(default)]
        z: usize,
        unit: String,
    },
    /// Turn a unit a quarter round, clockwise (free, while building).
    Turn {
        x: usize,
        y: usize,
        #[serde(default)]
        z: usize,
    },
    /// Lay one of its room's finishes on a dug cell (`""`: the plain floor back, free).
    Paint {
        x: usize,
        y: usize,
        #[serde(default)]
        z: usize,
        finish: String,
    },
    /// Give the inn a name of the player's own.
    Rename { name: String },
    /// Take a unit away; half its cost comes back.
    Remove {
        x: usize,
        y: usize,
        #[serde(default)]
        z: usize,
    },
    /// Put a wall, or a door, on the side of a dug cell towards another dug cell.
    Wall {
        x: usize,
        y: usize,
        #[serde(default)]
        z: usize,
        side: Side,
        #[serde(default)]
        door: bool,
    },
    /// Take a wall or door away; half its cost comes back.
    Unwall {
        x: usize,
        y: usize,
        #[serde(default)]
        z: usize,
        side: Side,
    },
    /// A command for the inn's incremental layer (its OpenIdle game): buy a generator or an
    /// upgrade, use an action, take a prestige (`{"cmd": "buy", "id": "Peddler"}`).
    Idle { cmd: openidle::Command },
    /// Time passing outside the night (the day, a pause, time away): the incremental layer runs
    /// on, the inn does not. In milliseconds, so that waits add up exactly.
    Wait { ms: u64 },
    /// Time the player was away: a wait, but the inn's money it brings is capped by the bridge's
    /// `away` expression (what the ledger has bought), as it stands when the player comes back.
    Away { ms: u64 },
    /// A whole building at once, free: only as a game's first command (an imported plan).
    Plan {
        cells: Vec<PlanCell>,
        #[serde(default)]
        walls: Vec<PlanWall>,
        #[serde(default)]
        stairs: Vec<PlanAt>,
    },
    /// End a phase that waits for the player.
    Next,
    /// Run a timed phase on for some ticks.
    Tick { n: u32 },
    /// Show a guest the door, without a review.
    Eject { guest: u32 },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Event {
    /// The player came back: `earned` money while away, `kept` of it (at most `cap`).
    Away {
        ms: u64,
        earned: f64,
        kept: f64,
        cap: Option<f64>,
    },
    Phase {
        phase: String,
        day: u32,
    },
    Arrive {
        guest: u32,
        kind: String,
    },
    Serve {
        guest: u32,
        service: String,
        x: usize,
        y: usize,
        z: usize,
        price: f64,
    },
    Leave {
        guest: u32,
        kind: String,
        mood: f64,
        why: String,
    },
    Over {
        money: f64,
        reputation: f64,
        served: u32,
    },
    /// Something happened in the incremental layer: an OpenIdle event (`kind`: `unlock`,
    /// `milestone`, `upgrade`, `prestige`; `id`, `label`).
    Idle {
        event: serde_json::Value,
    },
    /// The inn was sold (a `restart` prestige): the building starts over; this is the new run.
    Restart {
        run: u32,
    },
}

/// The incremental layer, running: its OpenIdle world, with the inn as its one player.
struct Idle {
    world: World,
    /// The inn's shared globals as last told to the idle game.
    synced: Vec<f64>,
    /// Milliseconds of idle time not yet run.
    carry: u64,
}

impl std::fmt::Debug for Idle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Idle(tick {})", self.world.tick())
    }
}

/// What the incremental layer does to the inn, worked out after every step.
#[derive(Clone, Debug)]
struct Effects {
    /// Each service's price, times this.
    price: Vec<f64>,
    /// How strongly each combo works (1: as written).
    combo: Vec<f64>,
    arrive: f64,
    patience: f64,
    locked: Vec<Lockable>,
}

#[derive(Clone, Debug, PartialEq)]
struct Guest {
    id: u32,
    kind: usize,
    cell: usize,
    needs: Vec<f64>,
    purse: f64,
    mood: f64,
    age: u32,
    stay: u32,
    /// Ticks spent wanting something nobody gives.
    waiting: u32,
    /// Being served: (unit, ticks left, service).
    busy: Option<(usize, u32, usize)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Stats {
    pub arrived: u32,
    pub served: u32,
    pub happy: u32,
    pub angry: u32,
    pub earned: f64,
}

#[derive(Debug)]
pub struct Game {
    pub rs: Ruleset,
    pub seed: String,
    pub mode: Mode,
    /// The grid for this mode: width, height, levels; and the most guests at once.
    pub width: usize,
    pub height: usize,
    pub levels: usize,
    max_guests: usize,
    /// Inner walls between two dug cells, `(lower cell, higher cell)` -> is it a door.
    walls: BTreeMap<(usize, usize), bool>,
    rng: u64,
    /// Room kind of each cell of the grid, if dug.
    room: Vec<Option<usize>>,
    /// GlassBox place of each dug cell, and back.
    place: Vec<usize>,
    cell_of: Vec<usize>,
    graph: Vec<Vec<usize>>,
    gb: State,
    /// Unit standing on each cell (index into `gb.units`), and each unit's kind and cell.
    unit_at: Vec<Option<usize>>,
    unit_kind: Vec<usize>,
    unit_cell: Vec<usize>,
    /// Quarter turns of each unit, clockwise.
    unit_rot: Vec<u8>,
    /// Combos whose furniture and room hold for each unit (their tests are checked when used).
    combo_on: Vec<Vec<usize>>,
    /// The finish laid on each cell, if any (an index into its room's finishes).
    finish: Vec<Option<usize>>,
    /// The inn's name, if the player gave it one.
    pub name: Option<String>,
    seated: Vec<u32>,
    guests: Vec<Guest>,
    next_guest: u32,
    pub phase: usize,
    pub phase_tick: u32,
    pub day: u32,
    pub nights_done: u32,
    pub over: bool,
    pub stats: Stats,
    pub commands: Vec<Command>,
    events: Vec<Event>,
    idle: Option<Idle>,
    fx: Effects,
    /// How many times the inn has been sold and started over.
    pub runs: u32,
}

const NONE: usize = usize::MAX;

fn hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h | 1
}

impl Game {
    /// A season of the ruleset.
    pub fn new(rs: Ruleset, seed: &str) -> Game {
        Game::with_mode(rs, seed, Mode::Season)
    }

    pub fn with_mode(rs: Ruleset, seed: &str, mode: Mode) -> Game {
        let mut g = Game::build(rs, seed, mode);
        g.attach_idle();
        g
    }

    /// The inn without its incremental layer.
    fn build(rs: Ruleset, seed: &str, mode: Mode) -> Game {
        let (width, height, levels, max_guests) = match mode {
            Mode::Season => (
                rs.inn.width,
                rs.inn.height,
                rs.inn.levels,
                rs.inn.max_guests,
            ),
            Mode::Endless => rs.inn.endless,
        };
        let n = width * height * levels;
        let rs_services = rs.services.len();
        let rs_combos = rs.combos.len();
        let gb = State::new(&rs.rules, 0, 1);
        let mut g = Game {
            rs,
            seed: seed.to_string(),
            mode,
            width,
            height,
            levels,
            max_guests,
            walls: BTreeMap::new(),
            rng: hash(seed),
            room: vec![None; n],
            place: vec![NONE; n],
            cell_of: Vec::new(),
            graph: Vec::new(),
            gb,
            unit_at: vec![None; n],
            unit_kind: Vec::new(),
            unit_cell: Vec::new(),
            unit_rot: Vec::new(),
            combo_on: Vec::new(),
            finish: vec![None; n],
            name: None,
            seated: Vec::new(),
            guests: Vec::new(),
            next_guest: 1,
            phase: 0,
            phase_tick: 0,
            day: 1,
            nights_done: 0,
            over: false,
            stats: Stats::default(),
            commands: Vec::new(),
            events: Vec::new(),
            idle: None,
            fx: Effects {
                price: vec![1.0; rs_services],
                combo: vec![1.0; rs_combos],
                arrive: 1.0,
                patience: 1.0,
                locked: Vec::new(),
            },
            runs: 0,
        };
        let start = g.rs.inn.start.clone();
        for (r, v) in start {
            g.gb.accounts[0][r] = v;
        }
        let er = g.rs.inn.entrance_room;
        g.open_cell(g.entrance(), er);
        g.enter_phase();
        g
    }

    /// Replays a saved season.
    pub fn replay(rs: Ruleset, seed: &str, commands: &[Command]) -> Result<Game, String> {
        Game::replay_mode(rs, seed, Mode::Season, commands)
    }

    /// Replays a saved game.
    pub fn replay_mode(
        rs: Ruleset,
        seed: &str,
        mode: Mode,
        commands: &[Command],
    ) -> Result<Game, String> {
        let mut g = Game::with_mode(rs, seed, mode);
        for c in commands {
            g.apply(c.clone())?;
        }
        Ok(g)
    }

    // ---------- small helpers ----------

    fn rand(&mut self) -> f64 {
        // xorshift64*
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        (x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    fn between(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.rand()
    }

    pub fn cell(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.height + y) * self.width + x
    }

    pub fn xyz(&self, c: usize) -> (usize, usize, usize) {
        let plane = self.width * self.height;
        (c % self.width, (c % plane) / self.width, c / plane)
    }

    fn entrance(&self) -> usize {
        self.cell(self.rs.inn.entrance.0, self.rs.inn.entrance.1, 0)
    }

    /// The cell inside the grid, or an error.
    fn at(&self, x: usize, y: usize, z: usize) -> Result<usize, String> {
        if x >= self.width || y >= self.height || z >= self.levels {
            return Err("that is outside the inn".into());
        }
        Ok(self.cell(x, y, z))
    }

    /// The cell next to `c` on the same level, on side `s`.
    fn beside(&self, c: usize, s: Side) -> Option<usize> {
        let (x, y, z) = self.xyz(c);
        let (dx, dy) = s.delta();
        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
        if nx < 0 || ny < 0 || nx >= self.width as i64 || ny >= self.height as i64 {
            return None;
        }
        Some(self.cell(nx as usize, ny as usize, z))
    }

    /// Neighbouring cells on the same level.
    fn neighbours(&self, c: usize) -> Vec<usize> {
        Side::ALL
            .iter()
            .filter_map(|&s| self.beside(c, s))
            .collect()
    }

    fn is_stairs(&self, c: usize) -> bool {
        self.unit_at[c].is_some_and(|u| self.rs.units[self.unit_kind[u]].stairs)
    }

    /// The cell one level further than `c` (the level its stairs lead to).
    fn next_level(&self, c: usize) -> Option<usize> {
        let (x, y, z) = self.xyz(c);
        (z + 1 < self.levels).then(|| self.cell(x, y, z + 1))
    }

    fn link(&mut self, a: usize, b: usize) {
        let (p, q) = (self.place[a], self.place[b]);
        if p == NONE || q == NONE || self.graph[p].contains(&q) {
            return;
        }
        self.graph[p].push(q);
        self.graph[q].push(p);
    }

    fn unlink(&mut self, a: usize, b: usize) {
        let (p, q) = (self.place[a], self.place[b]);
        if p == NONE || q == NONE {
            return;
        }
        self.graph[p].retain(|&n| n != q);
        self.graph[q].retain(|&n| n != p);
    }

    fn edge(a: usize, b: usize) -> (usize, usize) {
        (a.min(b), a.max(b))
    }

    // ---------- the incremental layer ----------

    fn attach_idle(&mut self) {
        let (Some(b), Some(game)) = (&self.rs.idle, &self.rs.idle_game) else {
            return;
        };
        let mut world = World::new(game.0.clone(), hash(&self.seed));
        world.join(&b.player, &self.rs.inn.name);
        world.take_events();
        let synced = b
            .share
            .iter()
            .map(|(_, res)| {
                world
                    .amount(Some(&b.player), res)
                    .map_or(0.0, |n| n.to_f64())
            })
            .collect();
        self.idle = Some(Idle {
            world,
            synced,
            carry: 0,
        });
        // The idle game's shared resources start as the inn's.
        self.idle_push();
        self.idle_pull();
    }

    /// Tells the idle game what the inn's shared globals have become since the last time.
    fn idle_push(&mut self) {
        let (Some(b), Some(idle)) = (&self.rs.idle, &mut self.idle) else {
            return;
        };
        for (i, (g, res)) in b.share.iter().enumerate() {
            let now = self.gb.accounts[0][*g];
            let d = now - idle.synced[i];
            if d != 0.0 {
                let _ = idle.world.credit(Some(&b.player), res, Num::from_f64(d));
            }
            idle.synced[i] = now;
        }
    }

    /// Brings the idle game's side back: the shared globals, the `set` globals, its effects on
    /// the inn and its events (a `restart` prestige sells the inn).
    fn idle_pull(&mut self) {
        let (Some(b), Some(idle)) = (&self.rs.idle, &mut self.idle) else {
            return;
        };
        let p = Some(b.player.as_str());
        for (i, (g, res)) in b.share.iter().enumerate() {
            let v = idle.world.amount(p, res).map_or(0.0, |n| n.to_f64());
            self.gb.accounts[0][*g] = v;
            idle.synced[i] = v;
        }
        let world = &idle.world;
        let eval = |e: &str, or: f64| world.eval(p, e).map_or(or, |n| n.to_f64());
        for (g, e) in &b.sets {
            self.gb.accounts[0][*g] = eval(e, 0.0);
        }
        let mut price = vec![1.0; self.rs.services.len()];
        for (s, e) in &b.prices {
            let k = eval(e, 1.0);
            match s {
                Some(s) => price[*s] *= k,
                None => price.iter_mut().for_each(|p| *p *= k),
            }
        }
        let mut combo = vec![1.0; self.rs.combos.len()];
        for (c, e) in &b.boosts {
            combo[*c] *= eval(e, 1.0);
        }
        self.fx = Effects {
            price,
            combo,
            arrive: b.arrive.iter().map(|e| eval(e, 1.0)).product(),
            patience: b.patience.iter().map(|e| eval(e, 1.0)).product(),
            locked: b
                .unlocks
                .iter()
                .filter(|(_, e)| eval(e, 1.0) == 0.0)
                .map(|(w, _)| *w)
                .collect(),
        };
        let mut restart = false;
        for e in idle.world.take_events() {
            if e["kind"] == "prestige" && b.restart.iter().any(|r| e["id"] == r.as_str()) {
                restart = true;
            }
            self.events.push(Event::Idle { event: e });
        }
        if restart {
            self.restart_inn();
        }
    }

    /// Runs the idle game on for some milliseconds (it keeps the rest for next time).
    fn idle_time(&mut self, ms: u64) {
        if self.idle.is_none() {
            return;
        }
        self.idle_push();
        if let Some(idle) = self.idle.as_mut() {
            let tick = ((idle.world.game.tick * 1000.0).round() as u64).max(1);
            idle.carry += ms;
            let n = idle.carry / tick;
            if n > 0 {
                idle.world.run(n);
                idle.carry -= n * tick;
            }
        }
        self.idle_pull();
    }

    /// The most money time away can bring, if the bridge caps it.
    pub fn away_cap(&self) -> Option<f64> {
        let (b, idle) = (self.rs.idle.as_ref()?, self.idle.as_ref()?);
        let e = b.away.as_ref()?;
        Some(idle.world.eval(Some(&b.player), e).ok()?.to_f64().max(0.0))
    }

    /// Time away: the idle game runs on, and what it adds to the inn's money beyond the cap is
    /// lost (taken back from the shared idle resource).
    fn away(&mut self, ms: u64) {
        let money = self.rs.inn.money;
        let before = self.gb.accounts[0][money];
        self.idle_time(ms);
        let earned = self.gb.accounts[0][money] - before;
        let cap = self.away_cap();
        let kept = cap.map_or(earned, |c| earned.min(c));
        if kept < earned {
            if let (Some(b), Some(idle)) = (&self.rs.idle, &mut self.idle) {
                if let Some((_, res)) = b.share.iter().find(|(g, _)| *g == money) {
                    let _ = idle
                        .world
                        .credit(Some(&b.player), res, Num::from_f64(kept - earned));
                }
            }
            self.idle_pull();
        }
        self.events.push(Event::Away {
            ms,
            earned,
            kept: self.gb.accounts[0][money] - before,
            cap,
        });
    }

    /// Something happened in the inn that feeds the idle game.
    fn feed(&mut self, ev: FeedEvent, amount: f64) {
        let (Some(b), Some(idle)) = (&self.rs.idle, &mut self.idle) else {
            return;
        };
        for (res, e, k) in &b.feeds {
            if *e == ev {
                let _ = idle
                    .world
                    .credit(Some(&b.player), res, Num::from_f64(k * amount));
            }
        }
    }

    /// The inn is sold: the building starts over (a new run, with its own dice), the incremental
    /// layer goes on.
    fn restart_inn(&mut self) {
        let runs = self.runs + 1;
        let mut fresh = Game::build(self.rs.clone(), &format!("{}#{runs}", self.seed), self.mode);
        fresh.seed = std::mem::take(&mut self.seed);
        fresh.runs = runs;
        fresh.name = self.name.take();
        fresh.idle = self.idle.take();
        fresh.commands = std::mem::take(&mut self.commands);
        let mut events = std::mem::take(&mut self.events);
        events.push(Event::Restart { run: runs });
        events.append(&mut fresh.events);
        fresh.events = events;
        *self = fresh;
        // The prestige reset the idle game's shared resources: the inn's starting ones go in.
        if let (Some(b), Some(idle)) = (&self.rs.idle, &mut self.idle) {
            for (i, (_, res)) in b.share.iter().enumerate() {
                idle.synced[i] = idle
                    .world
                    .amount(Some(&b.player), res)
                    .map_or(0.0, |n| n.to_f64());
            }
        }
        self.idle_push();
        self.idle_pull();
    }

    /// A service's price now.
    pub fn price(&self, s: usize) -> f64 {
        (self.rs.services[s].price * self.fx.price[s] * 100.0).round() / 100.0
    }

    /// The combos that hold for a unit now.
    fn active(&self, u: usize) -> impl Iterator<Item = usize> + '_ {
        let cell = self.unit_cell[u];
        self.combo_on[u].iter().copied().filter(move |&c| {
            !self.locked(Lockable::Combo(c))
                && self.rs.combos[c].tests.iter().all(|t| self.test(t, cell))
        })
    }

    /// A combo's multiplier, scaled by how strongly the incremental layer makes it work.
    fn scaled(&self, c: usize, k: f64) -> f64 {
        (1.0 + (k - 1.0) * self.fx.combo[c]).max(0.0)
    }

    /// A service's price at a unit now: its price, times the unit's combos.
    pub fn unit_price(&self, u: usize, s: usize) -> f64 {
        let k: f64 = self
            .active(u)
            .map(|c| self.scaled(c, self.rs.combos[c].price))
            .product();
        (self.rs.services[s].price * self.fx.price[s] * k * 100.0).round() / 100.0
    }

    /// Works out again which combos each unit's furniture and room allow.
    fn refresh_combos(&mut self) {
        let n = self.unit_kind.len();
        let mut on = vec![Vec::new(); n];
        for (u, list) in on.iter_mut().enumerate() {
            if !self.gb.units[u].alive {
                continue;
            }
            let k = self.unit_kind[u];
            let cell = self.unit_cell[u];
            let (x, y, z) = self.xyz(cell);
            for (c, def) in self.rs.combos.iter().enumerate() {
                if !def.units.contains(&k) {
                    continue;
                }
                if !def.rooms.is_empty() && !self.room[cell].is_some_and(|r| def.rooms.contains(&r))
                {
                    continue;
                }
                let ok = def.near.iter().all(|(kinds, radius, count)| {
                    let r = *radius as i64;
                    let found = (0..n)
                        .filter(|&v| {
                            v != u
                                && self.gb.units[v].alive
                                && kinds.contains(&self.unit_kind[v])
                                && {
                                    let (vx, vy, vz) = self.xyz(self.unit_cell[v]);
                                    vz == z
                                        && (vx as i64 - x as i64).abs() <= r
                                        && (vy as i64 - y as i64).abs() <= r
                                }
                        })
                        .count();
                    found as u32 >= *count
                });
                if ok {
                    list.push(c);
                }
            }
        }
        self.combo_on = on;
    }

    fn locked(&self, w: Lockable) -> bool {
        self.fx.locked.contains(&w)
    }

    pub fn global(&self, r: usize) -> f64 {
        self.gb.accounts[0][r]
    }

    pub fn money(&self) -> f64 {
        self.global(self.rs.inn.money)
    }

    fn pay(&mut self, amount: f64) -> Result<(), String> {
        let m = self.rs.inn.money;
        if self.gb.accounts[0][m] + 1e-9 < amount {
            return Err(format!(
                "not enough {}: {amount} needed",
                self.rs.rules.resources[m].id
            ));
        }
        self.gb.accounts[0][m] -= amount;
        Ok(())
    }

    fn test(&self, t: &Test, cell: usize) -> bool {
        match *t {
            Test::Map(m, cmp, v) => cmp.test(self.gb.maps[m][self.place[cell]], v),
            Test::Global(r, cmp, v) => cmp.test(self.global(r), v),
        }
    }

    fn open_cell(&mut self, c: usize, room: usize) {
        self.room[c] = Some(room);
        let p = self.cell_of.len();
        self.place[c] = p;
        self.cell_of.push(c);
        self.graph.push(Vec::new());
        for n in self.neighbours(c) {
            if !self.walls.get(&Game::edge(c, n)).is_some_and(|door| !door) {
                self.link(c, n);
            }
        }
        // Stairs on the level before lead here.
        let (x, y, z) = self.xyz(c);
        if z > 0 {
            let up = self.cell(x, y, z - 1);
            if self.is_stairs(up) {
                self.link(up, c);
            }
        }
        self.gb.sync(&self.rs.rules, self.graph.len());
    }

    /// Whether a cell may be dug: next to a dug cell on its level, or where stairs lead.
    fn diggable(&self, c: usize) -> bool {
        let (x, y, z) = self.xyz(c);
        self.neighbours(c).iter().any(|&n| self.room[n].is_some())
            || (z > 0 && self.is_stairs(self.cell(x, y, z - 1)))
    }

    fn put_unit(&mut self, cell: usize, k: usize) {
        let kind = &self.rs.units[k];
        let (def, start) = (kind.def, kind.start.clone());
        let u = self.gb.add_unit(&self.rs.rules, def, self.place[cell], 0);
        for (r, v) in start {
            self.gb.units[u].stock[r] = v;
        }
        self.unit_at[cell] = Some(u);
        self.unit_kind.push(k);
        self.unit_cell.push(cell);
        self.unit_rot.push(0);
        self.combo_on.push(Vec::new());
        self.seated.push(0);
        if self.rs.units[k].stairs {
            if let Some(next) = self.next_level(cell) {
                self.link(cell, next);
            }
        }
    }

    /// The other cell of an inner edge, checking both are dug.
    fn inner_edge(
        &self,
        x: usize,
        y: usize,
        z: usize,
        side: Side,
    ) -> Result<(usize, usize), String> {
        let a = self.at(x, y, z)?;
        let b = self
            .beside(a, side)
            .ok_or("there is nothing on that side")?;
        if self.room[a].is_none() || self.room[b].is_none() {
            return Err("walls go between two dug cells".into());
        }
        Ok((a, b))
    }

    fn set_wall(&mut self, a: usize, b: usize, door: bool) {
        self.walls.insert(Game::edge(a, b), door);
        if door {
            self.link(a, b);
        } else {
            self.unlink(a, b);
        }
    }

    // ---------- commands ----------

    /// Applies a command; on success it is recorded for the save.
    pub fn apply(&mut self, c: Command) -> Result<Vec<Event>, String> {
        if self.over
            && !matches!(
                c,
                Command::Idle { .. }
                    | Command::Wait { .. }
                    | Command::Away { .. }
                    | Command::Rename { .. }
            )
        {
            return Err("the season is over".into());
        }
        let ph = &self.rs.phases[self.phase];
        let (build, timed) = (ph.build, ph.length > 0);
        match &c {
            Command::Dig { x, y, z, room } => {
                if !build {
                    return Err("you can only dig while building".into());
                }
                let r = self
                    .rs
                    .room(room)
                    .ok_or_else(|| format!("there is no room {room:?}"))?;
                let cell = self.at(*x, *y, *z)?;
                if self.room[cell].is_some() {
                    return Err("that is already dug".into());
                }
                if !self.diggable(cell) {
                    return Err(if *z > 0 {
                        "dig next to the inn, or where stairs lead".into()
                    } else {
                        "dig next to the inn".into()
                    });
                }
                if self.locked(Lockable::Room(r)) {
                    return Err(format!("the {} is not unlocked yet", self.rs.rooms[r].name));
                }
                self.pay(self.rs.rooms[r].cost)?;
                self.open_cell(cell, r);
            }
            Command::Build { x, y, z, unit } => {
                if !build {
                    return Err("you can only build while building".into());
                }
                let k = self
                    .rs
                    .unit(unit)
                    .ok_or_else(|| format!("there is no unit {unit:?}"))?;
                let cell = self.at(*x, *y, *z)?;
                let room = self.room[cell].ok_or("dig there first")?;
                if self.unit_at[cell].is_some() {
                    return Err("something already stands there".into());
                }
                let kind = &self.rs.units[k];
                if !kind.rooms.is_empty() && !kind.rooms.contains(&room) {
                    let names: Vec<_> = kind
                        .rooms
                        .iter()
                        .map(|&r| self.rs.rooms[r].name.as_str())
                        .collect();
                    return Err(format!("a {} belongs in: {}", kind.name, names.join(", ")));
                }
                if self.locked(Lockable::Unit(k)) {
                    return Err(format!("the {} is not unlocked yet", kind.name));
                }
                if kind.stairs && self.next_level(cell).is_none() {
                    return Err(format!(
                        "there is no level {} this one",
                        if self.rs.inn.down { "below" } else { "above" }
                    ));
                }
                self.pay(kind.cost)?;
                self.put_unit(cell, k);
                self.refresh_combos();
            }
            Command::Turn { x, y, z } => {
                if !build {
                    return Err("you can only turn things while building".into());
                }
                let cell = self.at(*x, *y, *z)?;
                let u = self.unit_at[cell].ok_or("nothing stands there")?;
                self.unit_rot[u] = (self.unit_rot[u] + 1) % 4;
            }
            Command::Paint { x, y, z, finish } => {
                if !build {
                    return Err("you can only lay floors while building".into());
                }
                let cell = self.at(*x, *y, *z)?;
                let r = self.room[cell].ok_or("dig there first")?;
                let room = &self.rs.rooms[r];
                if finish.is_empty() {
                    if self.finish[cell].is_none() {
                        return Err("the floor is plain already".into());
                    }
                    self.finish[cell] = None;
                } else {
                    let f = room
                        .finishes
                        .iter()
                        .position(|x| x.id == *finish)
                        .ok_or_else(|| format!("a {} has no finish {finish:?}", room.name))?;
                    if self.finish[cell] == Some(f) {
                        return Err("that floor is laid already".into());
                    }
                    self.pay(room.finishes[f].cost)?;
                    self.finish[cell] = Some(f);
                }
            }
            Command::Rename { name } => {
                let name = name.trim();
                if name.chars().count() > 40 {
                    return Err("that name is too long".into());
                }
                self.name = (!name.is_empty()).then(|| name.to_string());
            }
            Command::Remove { x, y, z } => {
                if !build {
                    return Err("you can only remove things while building".into());
                }
                let cell = self.at(*x, *y, *z)?;
                let u = self.unit_at[cell].ok_or("nothing stands there")?;
                let k = self.unit_kind[u];
                if self.rs.units[k].stairs {
                    if let Some(next) = self.next_level(cell) {
                        self.unlink(cell, next);
                    }
                }
                self.gb.units[u].alive = false;
                self.unit_at[cell] = None;
                let m = self.rs.inn.money;
                self.gb.accounts[0][m] += self.rs.units[k].cost / 2.0;
                self.refresh_combos();
            }
            Command::Wall {
                x,
                y,
                z,
                side,
                door,
            } => {
                if !build {
                    return Err("you can only build walls while building".into());
                }
                let (a, b) = self.inner_edge(*x, *y, *z, *side)?;
                if self.walls.get(&Game::edge(a, b)) == Some(door) {
                    return Err(format!(
                        "there is already a {} there",
                        if *door { "door" } else { "wall" }
                    ));
                }
                self.pay(if *door {
                    self.rs.inn.door_cost
                } else {
                    self.rs.inn.wall_cost
                })?;
                self.set_wall(a, b, *door);
            }
            Command::Unwall { x, y, z, side } => {
                if !build {
                    return Err("you can only take walls down while building".into());
                }
                let (a, b) = self.inner_edge(*x, *y, *z, *side)?;
                let door = self
                    .walls
                    .remove(&Game::edge(a, b))
                    .ok_or("there is no wall there")?;
                self.link(a, b);
                let m = self.rs.inn.money;
                self.gb.accounts[0][m] += if door {
                    self.rs.inn.door_cost
                } else {
                    self.rs.inn.wall_cost
                } / 2.0;
            }
            Command::Idle { cmd } => {
                let player = self
                    .rs
                    .idle
                    .as_ref()
                    .ok_or("this inn has no incremental layer")?
                    .player
                    .clone();
                self.idle_push();
                if let Some(idle) = self.idle.as_mut() {
                    idle.world.apply(&player, cmd)?;
                }
                self.idle_pull();
            }
            Command::Wait { ms } => self.idle_time(*ms),
            Command::Away { ms } => self.away(*ms),
            Command::Plan {
                cells,
                walls,
                stairs,
            } => {
                if !self.commands.is_empty() || !build {
                    return Err("a plan can only be a game's first command".into());
                }
                self.plan(cells, walls, stairs)?;
            }
            Command::Next => {
                if timed {
                    return Err("this phase ends by itself".into());
                }
                self.next_phase();
            }
            Command::Tick { n } => {
                if !timed {
                    return Err("this phase waits for you: use next".into());
                }
                for _ in 0..*n {
                    if self.over {
                        break;
                    }
                    self.tick();
                }
            }
            Command::Eject { guest } => {
                let i = self
                    .guests
                    .iter()
                    .position(|g| g.id == *guest)
                    .ok_or("there is no such guest")?;
                self.leave(i, "shown the door", false);
            }
        }
        // Waits in a row are one wait (they add up exactly).
        match (&c, self.commands.last_mut()) {
            (Command::Wait { ms }, Some(Command::Wait { ms: before })) => *before += ms,
            _ => self.commands.push(c),
        }
        Ok(std::mem::take(&mut self.events))
    }

    // ---------- phases ----------

    fn enter_phase(&mut self) {
        let ph = self.rs.phases[self.phase].clone();
        for (r, v) in &ph.set {
            self.gb.accounts[0][*r] = *v;
        }
        self.phase_tick = 0;
        self.events.push(Event::Phase {
            phase: ph.id.clone(),
            day: self.day,
        });
    }

    fn next_phase(&mut self) {
        let (open, night) = (
            self.rs.phases[self.phase].open,
            self.rs.phases[self.phase].night,
        );
        if open {
            // The doors close: everyone still here leaves with what they have had.
            while !self.guests.is_empty() {
                self.leave(0, "closing time", true);
            }
        }
        if night {
            self.nights_done += 1;
            self.feed(FeedEvent::Night, 1.0);
            if self.mode == Mode::Season && self.nights_done >= self.rs.inn.nights {
                self.over = true;
                self.events.push(Event::Over {
                    money: self.money(),
                    reputation: self.global(self.rs.inn.reputation),
                    served: self.stats.served,
                });
                return;
            }
        }
        self.phase += 1;
        if self.phase == self.rs.phases.len() {
            self.phase = 0;
            self.day += 1;
        }
        self.enter_phase();
    }

    // ---------- the tick ----------

    fn tick(&mut self) {
        let ph = self.rs.phases[self.phase].clone();
        if ph.open {
            self.arrivals(&ph);
            self.move_guests();
        }
        // Combos that hold give off what they give off.
        for u in 0..self.unit_kind.len() {
            if self.combo_on[u].is_empty() {
                continue;
            }
            let at = self.place[self.unit_cell[u]];
            let active: Vec<usize> = self.active(u).collect();
            for c in active {
                for &(m, amount, radius) in &self.rs.combos[c].emits {
                    let k = self.fx.combo[c];
                    emit(&self.graph, &mut self.gb.maps[m], at, amount * k, radius);
                }
            }
        }
        step(&self.rs.rules, &self.graph, &mut self.gb);
        if let Some(b) = &self.rs.idle {
            let ms = (b.tick * 1000.0).round() as u64;
            self.idle_time(ms);
        }
        self.phase_tick += 1;
        if self.phase_tick >= ph.length {
            self.next_phase();
        }
    }

    fn arrivals(&mut self, ph: &crate::script::PhaseDef) {
        if self.guests.len() >= self.max_guests {
            return;
        }
        let mut chance = ph.arrive;
        if let Some((r, per)) = ph.arrive_per {
            chance += per * self.global(r).max(0.0);
        }
        chance *= self.fx.arrive;
        if self.rand() >= chance {
            return;
        }
        let entrance = self.entrance();
        let ok: Vec<usize> = (0..self.rs.guests.len())
            .filter(|&k| {
                self.rs.guests[k]
                    .when
                    .iter()
                    .all(|t| self.test(t, entrance))
            })
            .collect();
        let total: f64 = ok.iter().map(|&k| self.rs.guests[k].weight).sum();
        if total <= 0.0 {
            return;
        }
        let mut pick = self.rand() * total;
        let mut kind = ok[ok.len() - 1];
        for &k in &ok {
            pick -= self.rs.guests[k].weight;
            if pick < 0.0 {
                kind = k;
                break;
            }
        }
        let def = self.rs.guests[kind].clone();
        let mut needs = vec![0.0; self.rs.needs.len()];
        for &(n, _, start) in &def.needs {
            needs[n] = start;
        }
        let purse = self.between(def.purse.0, def.purse.1).round();
        let stay = self
            .between(f64::from(def.stay.0), f64::from(def.stay.1))
            .round() as u32;
        let id = self.next_guest;
        self.next_guest += 1;
        self.guests.push(Guest {
            id,
            kind,
            cell: entrance,
            needs,
            purse,
            mood: 0.6,
            age: 0,
            stay,
            waiting: 0,
            busy: None,
        });
        self.stats.arrived += 1;
        self.feed(FeedEvent::Arrived, 1.0);
        self.events.push(Event::Arrive {
            guest: id,
            kind: def.id,
        });
    }

    /// The nearest cell (breadth first, from `from`) with a unit that can give a service for
    /// `need` now: a free seat, its stock, and a price the guest can pay. Returns the first step
    /// towards it, the unit and the service.
    /// Guests walk the GlassBox graph: through open edges and doors, and up and down stairs.
    fn find(&self, from: usize, need: usize, purse: f64) -> Option<(usize, usize, usize)> {
        let start = self.place[from];
        let mut prev = vec![NONE; self.graph.len()];
        let mut q = VecDeque::from([start]);
        prev[start] = start;
        while let Some(p) = q.pop_front() {
            let c = self.cell_of[p];
            if let Some(u) = self.unit_at[c] {
                if let Some(s) = self.offer(u, need, purse) {
                    let mut step = p;
                    while prev[step] != start && step != start {
                        step = prev[step];
                    }
                    return Some((self.cell_of[step], u, s));
                }
            }
            for &n in &self.graph[p] {
                if prev[n] == NONE {
                    prev[n] = p;
                    q.push_back(n);
                }
            }
        }
        None
    }

    fn offer(&self, u: usize, need: usize, purse: f64) -> Option<usize> {
        let k = self.unit_kind[u];
        if !self.gb.units[u].alive || self.seated[u] >= self.rs.units[k].seats {
            return None;
        }
        self.rs.services.iter().enumerate().position(|(i, s)| {
            s.need == need
                && s.units.contains(&k)
                && self.unit_price(u, i) <= purse
                && s.takes
                    .iter()
                    .all(|t| self.held(u, t.resource) + 1e-9 >= t.amount)
        })
    }

    fn held(&self, u: usize, r: usize) -> f64 {
        match self.rs.rules.resources[r].scope {
            Scope::Global => self.gb.accounts[0][r],
            _ => self.gb.units[u].stock[r],
        }
    }

    fn take(&mut self, u: usize, r: usize, amount: f64) {
        match self.rs.rules.resources[r].scope {
            Scope::Global => self.gb.accounts[0][r] -= amount,
            _ => self.gb.units[u].stock[r] -= amount,
        }
    }

    fn move_guests(&mut self) {
        let mut i = 0;
        while i < self.guests.len() {
            if self.guest_tick(i) {
                i += 1;
            }
        }
    }

    /// One guest's tick. Returns false if the guest left (so the same index is the next guest).
    fn guest_tick(&mut self, i: usize) -> bool {
        let def = self.rs.guests[self.guests[i].kind].clone();
        let cell = self.guests[i].cell;
        {
            let g = &mut self.guests[i];
            g.age += 1;
            for &(n, grow, _) in &def.needs {
                g.needs[n] = (g.needs[n] + grow).min(self.rs.needs[n].max);
            }
        }
        for &(m, amount, radius) in &def.emits {
            emit(
                &self.graph,
                &mut self.gb.maps[m],
                self.place[cell],
                amount,
                radius,
            );
        }
        let mut gloom = 0.0;
        for (t, amount) in &def.dislikes {
            if self.test(t, cell) {
                gloom += amount;
            }
        }
        self.guests[i].mood = (self.guests[i].mood - gloom).clamp(0.0, 1.0);

        // Being served.
        if let Some((u, left, s)) = self.guests[i].busy {
            if left > 1 {
                self.guests[i].busy = Some((u, left - 1, s));
                return true;
            }
            let (mut relief, mut mood) = (1.0, 0.1);
            for c in self.active(u) {
                relief *= self.scaled(c, self.rs.combos[c].relief);
                mood += self.rs.combos[c].mood * self.fx.combo[c];
            }
            let sv = &self.rs.services[s];
            let g = &mut self.guests[i];
            g.needs[sv.need] = (g.needs[sv.need] - sv.relief * relief).max(0.0);
            g.mood = (g.mood + mood).clamp(0.0, 1.0);
            g.busy = None;
            self.seated[u] = self.seated[u].saturating_sub(1);
            return true;
        }
        if self.guests[i].age >= self.guests[i].stay {
            self.leave(i, "time to go", true);
            return false;
        }
        // The most pressing need, if any is urgent.
        let g = &self.guests[i];
        let mut want = None;
        for (n, &v) in g.needs.iter().enumerate() {
            if v >= self.rs.needs[n].urgent && want.map_or(true, |(_, w)| v > w) {
                want = Some((n, v));
            }
        }
        let Some((need, _)) = want else {
            self.guests[i].waiting = 0;
            return true;
        };
        match self.find(cell, need, self.guests[i].purse) {
            Some((step, u, s)) if step == cell => {
                let sv = self.rs.services[s].clone();
                let price = self.unit_price(u, s);
                let active: Vec<usize> = self.active(u).collect();
                let time: f64 = active
                    .iter()
                    .map(|&c| self.scaled(c, self.rs.combos[c].time).max(0.05))
                    .product();
                let time = ((f64::from(sv.time) * time).round() as u32).max(1);
                for t in &sv.takes {
                    self.take(u, t.resource, t.amount);
                }
                let m = self.rs.inn.money;
                self.gb.accounts[0][m] += price;
                self.stats.earned += price;
                self.feed(FeedEvent::Served, 1.0);
                self.feed(FeedEvent::Earned, price);
                self.feed(FeedEvent::Service(s), 1.0);
                for &c in &active {
                    self.feed(FeedEvent::Combo(c), 1.0);
                }
                self.stats.served += 1;
                self.seated[u] += 1;
                for &(map, amount, radius) in &sv.emits {
                    emit(
                        &self.graph,
                        &mut self.gb.maps[map],
                        self.place[cell],
                        amount,
                        radius,
                    );
                }
                let (x, y, z) = self.xyz(cell);
                let g = &mut self.guests[i];
                g.purse -= price;
                g.waiting = 0;
                g.busy = Some((u, time, s));
                self.events.push(Event::Serve {
                    guest: g.id,
                    service: sv.id,
                    x,
                    y,
                    z,
                    price,
                });
            }
            Some((step, _, _)) => {
                self.guests[i].cell = step;
                self.guests[i].waiting = 0;
            }
            None => {
                let g = &mut self.guests[i];
                g.waiting += 1;
                g.mood = (g.mood - 0.01).max(0.0);
                if f64::from(g.waiting) >= f64::from(def.patience) * self.fx.patience {
                    self.leave(
                        i,
                        &format!("no {} to be had", self.rs.needs[need].name.to_lowercase()),
                        true,
                    );
                    return false;
                }
            }
        }
        true
    }

    fn leave(&mut self, i: usize, why: &str, review: bool) {
        let g = self.guests.remove(i);
        if let Some((u, _, _)) = g.busy {
            self.seated[u] = self.seated[u].saturating_sub(1);
        }
        if review {
            let rep = self.rs.inn.reputation;
            self.gb.accounts[0][rep] += (g.mood - 0.5) * 2.0 * self.rs.inn.review;
            if g.mood >= 0.5 {
                self.stats.happy += 1;
                self.feed(FeedEvent::Happy, 1.0);
            } else {
                self.stats.angry += 1;
                self.feed(FeedEvent::Angry, 1.0);
            }
        }
        self.events.push(Event::Leave {
            guest: g.id,
            kind: self.rs.guests[g.kind].id.clone(),
            mood: (g.mood * 100.0).round() / 100.0,
            why: why.to_string(),
        });
    }

    // ---------- what the view reads ----------

    pub fn snapshot(&self) -> Snapshot {
        let ph = &self.rs.phases[self.phase];
        let mut cells = Vec::new();
        for (p, &c) in self.cell_of.iter().enumerate() {
            let (x, y, z) = self.xyz(c);
            cells.push(CellView {
                x,
                y,
                z,
                room: self.rs.rooms[self.room[c].unwrap()].id.clone(),
                finish: self.finish[c]
                    .map(|f| self.rs.rooms[self.room[c].unwrap()].finishes[f].id.clone()),
                maps: self
                    .gb
                    .maps
                    .iter()
                    .map(|m| (m[p] * 100.0).round() / 100.0)
                    .collect(),
            });
        }
        let units = (0..self.unit_kind.len())
            .filter(|&u| self.gb.units[u].alive)
            .map(|u| {
                let (x, y, z) = self.xyz(self.unit_cell[u]);
                let k = &self.rs.units[self.unit_kind[u]];
                UnitView {
                    x,
                    y,
                    z,
                    unit: k.id.clone(),
                    model: k.model.clone(),
                    seated: self.seated[u],
                    rot: self.unit_rot[u],
                    combos: self
                        .active(u)
                        .map(|c| self.rs.combos[c].id.clone())
                        .collect(),
                }
            })
            .collect();
        let guests = self
            .guests
            .iter()
            .map(|g| {
                let (x, y, z) = self.xyz(g.cell);
                GuestView {
                    id: g.id,
                    kind: self.rs.guests[g.kind].id.clone(),
                    x,
                    y,
                    z,
                    mood: (g.mood * 100.0).round() / 100.0,
                    purse: g.purse,
                    needs: g.needs.iter().map(|v| v.round()).collect(),
                    busy: g.busy.map(|(_, _, s)| self.rs.services[s].id.clone()),
                }
            })
            .collect();
        let globals = self
            .rs
            .rules
            .resources
            .iter()
            .enumerate()
            .filter(|(_, r)| r.scope == Scope::Global)
            .map(|(i, r)| {
                (
                    r.id.clone(),
                    (self.gb.accounts[0][i] * 100.0).round() / 100.0,
                )
            })
            .collect();
        Snapshot {
            name: self
                .name
                .clone()
                .unwrap_or_else(|| self.rs.inn.name.clone()),
            day: self.day,
            phase: ph.id.clone(),
            phase_tick: self.phase_tick,
            phase_length: ph.length,
            build: ph.build,
            over: self.over,
            globals,
            layers: self.rs.rules.maps.iter().map(|m| m.id.clone()).collect(),
            cells,
            units,
            guests,
            walls: self.wall_views(),
            idle: self.idle.as_ref().and_then(|i| {
                let b = self.rs.idle.as_ref()?;
                let mut v = i.world.view(&b.player).ok()?;
                v["offline_max"] = serde_json::json!(i.world.game.offline_max);
                v["away_cap"] = serde_json::json!(self.away_cap());
                Some(v)
            }),
            prices: (0..self.rs.services.len()).map(|s| self.price(s)).collect(),
            locked: self
                .fx
                .locked
                .iter()
                .map(|w| match *w {
                    Lockable::Unit(k) => self.rs.units[k].id.clone(),
                    Lockable::Room(r) => self.rs.rooms[r].id.clone(),
                    Lockable::Combo(c) => self.rs.combos[c].id.clone(),
                })
                .collect(),
            run: self.runs,
            mode: self.mode,
            grid: [self.width, self.height, self.levels],
            stats: self.stats.clone(),
        }
    }

    /// Inner walls and doors, each on the east or south side of its cell.
    pub fn wall_views(&self) -> Vec<WallView> {
        self.walls
            .iter()
            .map(|(&(a, b), &door)| {
                let (x, y, z) = self.xyz(a);
                let side = if b == a + 1 { Side::E } else { Side::S };
                WallView {
                    x,
                    y,
                    z,
                    side,
                    door,
                }
            })
            .collect()
    }

    /// Builds a plan at no cost: digs its cells (the entrance stays as it is), then puts up its
    /// walls and doors and its stairs.
    fn plan(
        &mut self,
        cells: &[PlanCell],
        walls: &[PlanWall],
        stairs: &[PlanAt],
    ) -> Result<(), String> {
        let entrance = self.entrance();
        for c in cells {
            let cell = self.at(c.x, c.y, c.z)?;
            let r = self
                .rs
                .room(&c.room)
                .ok_or_else(|| format!("there is no room {:?}", c.room))?;
            if cell != entrance && self.room[cell].is_none() {
                self.open_cell(cell, r);
            }
        }
        for w in walls {
            let (a, b) = self.inner_edge(w.x, w.y, w.z, w.side)?;
            self.set_wall(a, b, w.door);
        }
        if !stairs.is_empty() {
            let k = self
                .rs
                .units
                .iter()
                .position(|u| u.stairs)
                .ok_or("this ruleset has no stairs")?;
            for s in stairs {
                let cell = self.at(s.x, s.y, s.z)?;
                if self.room[cell].is_none()
                    || self.unit_at[cell].is_some()
                    || self.next_level(cell).is_none()
                {
                    return Err(format!("stairs cannot stand at {} {} {}", s.x, s.y, s.z));
                }
                self.put_unit(cell, k);
            }
            self.refresh_combos();
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CellView {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub room: String,
    /// The finish laid here, if any.
    pub finish: Option<String>,
    /// Each layer's value here, in the order of `Snapshot::layers`.
    pub maps: Vec<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnitView {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub unit: String,
    pub model: String,
    pub seated: u32,
    /// Quarter turns, clockwise.
    pub rot: u8,
    /// The combos that hold for it now.
    pub combos: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GuestView {
    pub id: u32,
    pub kind: String,
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub mood: f64,
    pub purse: f64,
    pub needs: Vec<f64>,
    pub busy: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    /// The inn's name: the player's own, or the ruleset's.
    pub name: String,
    pub day: u32,
    pub phase: String,
    pub phase_tick: u32,
    pub phase_length: u32,
    pub build: bool,
    pub over: bool,
    pub globals: Vec<(String, f64)>,
    pub layers: Vec<String>,
    pub cells: Vec<CellView>,
    pub units: Vec<UnitView>,
    pub guests: Vec<GuestView>,
    pub walls: Vec<WallView>,
    /// The incremental layer, as OpenIdle's view for the inn's player (and `offline_max`).
    pub idle: Option<serde_json::Value>,
    /// Each service's price now.
    pub prices: Vec<f64>,
    /// Units and rooms the incremental layer has not unlocked yet.
    pub locked: Vec<String>,
    /// How many times the inn has been sold and started over.
    pub run: u32,
    pub mode: Mode,
    /// Width, height and levels of this game's grid.
    pub grid: [usize; 3],
    pub stats: Stats,
}

#[derive(Clone, Debug, Serialize)]
pub struct WallView {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub side: Side,
    pub door: bool,
}
