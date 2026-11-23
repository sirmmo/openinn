//! The inn: a grid of dug cells (each one a GlassBox place), units standing on them, guests
//! walking between them, and phases that open and close the doors.
//!
//! Everything is deterministic: the seed and the list of commands give the same game on every
//! machine, so a save is just `{ seed, commands }`.

use glassbox::{emit, step, Scope, State};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::script::{Ruleset, Test};

/// What the player can do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "lowercase")]
pub enum Command {
    /// Dig a cell next to the inn as a room kind (in a building phase).
    Dig { x: usize, y: usize, room: String },
    /// Put a unit on a dug cell (in a building phase).
    Build { x: usize, y: usize, unit: String },
    /// Take a unit away; half its cost comes back.
    Remove { x: usize, y: usize },
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
    pub fn new(rs: Ruleset, seed: &str) -> Game {
        let n = rs.inn.width * rs.inn.height;
        let gb = State::new(&rs.rules, 0, 1);
        let mut g = Game {
            rs,
            seed: seed.to_string(),
            rng: hash(seed),
            room: vec![None; n],
            place: vec![NONE; n],
            cell_of: Vec::new(),
            graph: Vec::new(),
            gb,
            unit_at: vec![None; n],
            unit_kind: Vec::new(),
            unit_cell: Vec::new(),
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
        };
        let start = g.rs.inn.start.clone();
        for (r, v) in start {
            g.gb.accounts[0][r] = v;
        }
        let (ex, ey) = g.rs.inn.entrance;
        let er = g.rs.inn.entrance_room;
        g.open_cell(g.cell(ex, ey), er);
        g.enter_phase();
        g
    }

    /// Replays a saved game.
    pub fn replay(rs: Ruleset, seed: &str, commands: &[Command]) -> Result<Game, String> {
        let mut g = Game::new(rs, seed);
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

    pub fn cell(&self, x: usize, y: usize) -> usize {
        y * self.rs.inn.width + x
    }

    fn xy(&self, c: usize) -> (usize, usize) {
        (c % self.rs.inn.width, c / self.rs.inn.width)
    }

    fn neighbours(&self, c: usize) -> Vec<usize> {
        let (x, y) = self.xy(c);
        let (w, h) = (self.rs.inn.width, self.rs.inn.height);
        let mut out = Vec::new();
        if y > 0 {
            out.push(c - w);
        }
        if x + 1 < w {
            out.push(c + 1);
        }
        if y + 1 < h {
            out.push(c + w);
        }
        if x > 0 {
            out.push(c - 1);
        }
        out
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
            if self.place[n] != NONE {
                let q = self.place[n];
                self.graph[p].push(q);
                self.graph[q].push(p);
            }
        }
        self.gb.sync(&self.rs.rules, self.graph.len());
    }

    // ---------- commands ----------

    /// Applies a command; on success it is recorded for the save.
    pub fn apply(&mut self, c: Command) -> Result<Vec<Event>, String> {
        if self.over {
            return Err("the season is over".into());
        }
        let ph = &self.rs.phases[self.phase];
        let (build, timed) = (ph.build, ph.length > 0);
        match &c {
            Command::Dig { x, y, room } => {
                if !build {
                    return Err("you can only dig while building".into());
                }
                let r = self
                    .rs
                    .room(room)
                    .ok_or_else(|| format!("there is no room {room:?}"))?;
                if *x >= self.rs.inn.width || *y >= self.rs.inn.height {
                    return Err("that is outside the inn".into());
                }
                let cell = self.cell(*x, *y);
                if self.room[cell].is_some() {
                    return Err("that is already dug".into());
                }
                if !self
                    .neighbours(cell)
                    .iter()
                    .any(|&n| self.room[n].is_some())
                {
                    return Err("dig next to the inn".into());
                }
                self.pay(self.rs.rooms[r].cost)?;
                self.open_cell(cell, r);
            }
            Command::Build { x, y, unit } => {
                if !build {
                    return Err("you can only build while building".into());
                }
                let k = self
                    .rs
                    .unit(unit)
                    .ok_or_else(|| format!("there is no unit {unit:?}"))?;
                if *x >= self.rs.inn.width || *y >= self.rs.inn.height {
                    return Err("that is outside the inn".into());
                }
                let cell = self.cell(*x, *y);
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
                let (cost, def, start) = (kind.cost, kind.def, kind.start.clone());
                self.pay(cost)?;
                let u = self.gb.add_unit(&self.rs.rules, def, self.place[cell], 0);
                for (r, v) in start {
                    self.gb.units[u].stock[r] = v;
                }
                self.unit_at[cell] = Some(u);
                self.unit_kind.push(k);
                self.unit_cell.push(cell);
                self.seated.push(0);
            }
            Command::Remove { x, y } => {
                if !build {
                    return Err("you can only remove things while building".into());
                }
                let cell = self.cell(*x, *y);
                let u = self
                    .unit_at
                    .get(cell)
                    .copied()
                    .flatten()
                    .ok_or("nothing stands there")?;
                self.gb.units[u].alive = false;
                self.unit_at[cell] = None;
                let m = self.rs.inn.money;
                self.gb.accounts[0][m] += self.rs.units[self.unit_kind[u]].cost / 2.0;
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
        self.commands.push(c);
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
            if self.nights_done >= self.rs.inn.nights {
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
        step(&self.rs.rules, &self.graph, &mut self.gb);
        self.phase_tick += 1;
        if self.phase_tick >= ph.length {
            self.next_phase();
        }
    }

    fn arrivals(&mut self, ph: &crate::script::PhaseDef) {
        if self.guests.len() >= self.rs.inn.max_guests {
            return;
        }
        let mut chance = ph.arrive;
        if let Some((r, per)) = ph.arrive_per {
            chance += per * self.global(r).max(0.0);
        }
        if self.rand() >= chance {
            return;
        }
        let entrance = self.cell(self.rs.inn.entrance.0, self.rs.inn.entrance.1);
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
        self.events.push(Event::Arrive {
            guest: id,
            kind: def.id,
        });
    }

    /// The nearest cell (breadth first, from `from`) with a unit that can give a service for
    /// `need` now: a free seat, its stock, and a price the guest can pay. Returns the first step
    /// towards it, the unit and the service.
    fn find(&self, from: usize, need: usize, purse: f64) -> Option<(usize, usize, usize)> {
        let mut prev = vec![NONE; self.room.len()];
        let mut q = VecDeque::from([from]);
        prev[from] = from;
        while let Some(c) = q.pop_front() {
            if let Some(u) = self.unit_at[c] {
                if let Some(s) = self.offer(u, need, purse) {
                    let mut step = c;
                    while prev[step] != from && step != from {
                        step = prev[step];
                    }
                    return Some((step, u, s));
                }
            }
            for n in self.neighbours(c) {
                if self.room[n].is_some() && prev[n] == NONE {
                    prev[n] = c;
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
        self.rs.services.iter().position(|s| {
            s.need == need
                && s.units.contains(&k)
                && s.price <= purse
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
            let sv = &self.rs.services[s];
            let g = &mut self.guests[i];
            g.needs[sv.need] = (g.needs[sv.need] - sv.relief).max(0.0);
            g.mood = (g.mood + 0.1).min(1.0);
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
                for t in &sv.takes {
                    self.take(u, t.resource, t.amount);
                }
                let m = self.rs.inn.money;
                self.gb.accounts[0][m] += sv.price;
                self.stats.earned += sv.price;
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
                let (x, y) = self.xy(cell);
                let g = &mut self.guests[i];
                g.purse -= sv.price;
                g.waiting = 0;
                g.busy = Some((u, sv.time, s));
                self.events.push(Event::Serve {
                    guest: g.id,
                    service: sv.id,
                    x,
                    y,
                    price: sv.price,
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
                if g.waiting >= def.patience {
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
            } else {
                self.stats.angry += 1;
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
            let (x, y) = self.xy(c);
            cells.push(CellView {
                x,
                y,
                room: self.rs.rooms[self.room[c].unwrap()].id.clone(),
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
                let (x, y) = self.xy(self.unit_cell[u]);
                let k = &self.rs.units[self.unit_kind[u]];
                UnitView {
                    x,
                    y,
                    unit: k.id.clone(),
                    model: k.model.clone(),
                    seated: self.seated[u],
                }
            })
            .collect();
        let guests = self
            .guests
            .iter()
            .map(|g| {
                let (x, y) = self.xy(g.cell);
                GuestView {
                    id: g.id,
                    kind: self.rs.guests[g.kind].id.clone(),
                    x,
                    y,
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
            stats: self.stats.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CellView {
    pub x: usize,
    pub y: usize,
    pub room: String,
    /// Each layer's value here, in the order of `Snapshot::layers`.
    pub maps: Vec<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnitView {
    pub x: usize,
    pub y: usize,
    pub unit: String,
    pub model: String,
    pub seated: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct GuestView {
    pub id: u32,
    pub kind: String,
    pub x: usize,
    pub y: usize,
    pub mood: f64,
    pub purse: f64,
    pub needs: Vec<f64>,
    pub busy: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
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
    pub stats: Stats,
}
