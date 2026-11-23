//! A ruleset: GlassBox blocks (`unitRule`, `mapRule`...) compiled by the `glassbox` crate, plus
//! OpenInn's own blocks, all in the same `kind name ... end` form:
//!
//! ```text
//! inn dungeon          # the house: grid, entrance, money, how a season ends
//! phase night          # the day's phases, in order: what they allow, what they set
//! layer Noise          # a GlassBox map with decay, diffusion and bounds
//! room taproom         # a kind of room you dig, cell by cell
//! unit bar             # a piece of furniture or a station: where it may stand, what it costs
//! need Thirst          # something guests feel
//! service ale          # what a unit gives a guest for a need, and what it takes
//! guest adventurer     # a kind of guest: purse, needs, dislikes, when they come
//! look dungeon         # how a client draws the inn: models, colours, words (the engine ignores it)
//! idle ledger          # the inn's incremental layer: an OpenIdle game, and how it meets the inn
//! ```
//!
//! The incremental layer is an [OpenIdle](https://github.com/sirmmo/openidle) game in the same
//! folder (`idle.toml`, which is OpenIdle's `game.toml`, `params.toml` and `*.idle` scripts); the
//! `idle` block says how it and the inn meet (see [`IdleBridge`]).
//!
//! Every name is declared before it is used, and every error names its file and line.

use glassbox::{compile, number, split_blocks, tokenize, Block, Cmp, Error, Line, Rules, Scope};

/// OpenInn's block kinds, compiled here; everything else goes to GlassBox.
pub const HOST_KINDS: [&str; 10] = [
    "inn", "phase", "layer", "room", "unit", "need", "service", "guest", "look", "idle",
];

/// What, in the inn, feeds a resource of the incremental layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedEvent {
    /// A guest came in.
    Arrived,
    /// A guest was served (the amount is per service).
    Served,
    /// Money a service brought in (the amount multiplies the price).
    Earned,
    /// A guest left happy, or angry.
    Happy,
    Angry,
    /// A night ended.
    Night,
}

/// Something the incremental layer locks until an expression holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lockable {
    Unit(usize),
    Room(usize),
}

/// How the inn and its incremental layer (an OpenIdle game, played by one player) meet:
///
/// ```text
/// idle ledger
///     player inn                  # the OpenIdle player that is the inn (default: inn)
///     tick 0.25                   # seconds of the idle game per tick of the night
///     share Gold                  # one resource for both: the inn's global and the idle game's
///     feed Served served          # the idle resource Served gets 1 for every guest served
///     feed Takings earned 1       # ... the price of every service
///     feed Renown happy           # ... 1 for every guest who leaves happy (`angry`, `arrived`, `night`)
///     price ale 1 + 0.5 * Stout   # the ale's price, times this (an OpenIdle expression)
///     price * 1 + 0.1 * Legend    # every price, times this
///     arrive 1 + Bell             # the chance of a guest, times this
///     patience 1 + 0.5 * Benches  # how long guests wait, times this
///     unlock bed Featherbeds      # the unit (or room) can be built only while this holds
///     set Fame Renown             # the inn's global Fame is this, after every step
///     restart sell                # taking this prestige sells the inn: the building starts over
/// end
/// ```
#[derive(Clone, Debug, Default)]
pub struct IdleBridge {
    pub id: String,
    pub player: String,
    pub tick: f64,
    /// (inn global, idle resource)
    pub share: Vec<(usize, String)>,
    /// (idle resource, what feeds it, how much)
    pub feeds: Vec<(String, FeedEvent, f64)>,
    /// (service, or every service, expression)
    pub prices: Vec<(Option<usize>, String)>,
    pub arrive: Vec<String>,
    pub patience: Vec<String>,
    pub unlocks: Vec<(Lockable, String)>,
    /// (inn global, expression)
    pub sets: Vec<(usize, String)>,
    /// Prestiges that start the building over.
    pub restart: Vec<String>,
}

/// An incremental layer's OpenIdle game.
#[derive(Clone)]
pub struct IdleGame(pub std::sync::Arc<openidle::Game>);

impl std::fmt::Debug for IdleGame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IdleGame({})", self.0.name)
    }
}

#[derive(Clone, Debug, Default)]
pub struct InnDef {
    pub id: String,
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub entrance: (usize, usize),
    /// The room kind the entrance cell starts as.
    pub entrance_room: usize,
    /// Global resource that guests pay in and building costs.
    pub money: usize,
    /// Global resource raised by happy guests and lowered by angry ones.
    pub reputation: usize,
    /// Reputation change for a guest leaving with mood 1 (and minus this for mood 0).
    pub review: f64,
    /// Global resources at the start.
    pub start: Vec<(usize, f64)>,
    /// The season ends after this many nights (phases marked `night`).
    pub nights: u32,
    /// At most this many guests at once.
    pub max_guests: usize,
    /// Levels you can dig in a season (joined by stairs), and whether they go down (a dungeon)
    /// or up (a tower, a station's decks).
    pub levels: usize,
    pub down: bool,
    /// The endless mode's grid: width, height, levels and most guests at once.
    pub endless: (usize, usize, usize, usize),
    /// Costs of an inner wall and of a door on the edge between two dug cells.
    pub wall_cost: f64,
    pub door_cost: f64,
}

#[derive(Clone, Debug, Default)]
pub struct PhaseDef {
    pub id: String,
    pub name: String,
    /// Ticks; 0 means the phase waits for the player (`next`).
    pub length: u32,
    /// Digging and building are allowed.
    pub build: bool,
    /// Guests are simulated (arrive, move, are served).
    pub open: bool,
    /// Counts towards the season's `nights`.
    pub night: bool,
    /// Global resources set when the phase starts.
    pub set: Vec<(usize, f64)>,
    /// Chance per tick that a guest arrives, plus `per` times a global resource.
    pub arrive: f64,
    pub arrive_per: Option<(usize, f64)>,
}

#[derive(Clone, Debug)]
pub struct RoomDef {
    pub id: String,
    pub name: String,
    pub cost: f64,
    pub colour: String,
    pub floor: String,
    /// Watabou Dwellings room names that import as this room (`living-room`; `*` for any).
    pub dwellings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct UnitKind {
    pub id: String,
    /// Index in GlassBox's `rules.units`.
    pub def: usize,
    pub name: String,
    pub cost: f64,
    /// Room kinds it may stand in (empty: any).
    pub rooms: Vec<usize>,
    pub seats: u32,
    /// Local stock the unit starts with.
    pub start: Vec<(usize, f64)>,
    pub model: String,
    /// Stairs: join their cell to the same cell one level further (down or up).
    pub stairs: bool,
}

#[derive(Clone, Debug)]
pub struct NeedDef {
    pub id: String,
    pub name: String,
    /// A guest goes looking once a need reaches this.
    pub urgent: f64,
    pub max: f64,
}

/// What a service takes: a resource held by the inn (`global`) or by the unit (`local`).
#[derive(Clone, Debug)]
pub struct Take {
    pub resource: usize,
    pub amount: f64,
}

#[derive(Clone, Debug)]
pub struct ServiceDef {
    pub id: String,
    pub name: String,
    pub units: Vec<usize>,
    pub need: usize,
    /// How much of the need it takes away.
    pub relief: f64,
    pub takes: Vec<Take>,
    pub price: f64,
    /// Ticks it lasts; the guest stays at the unit meanwhile.
    pub time: u32,
    /// Map emissions at the unit's cell when it starts: (map, amount, radius).
    pub emits: Vec<(usize, f64, u32)>,
}

/// A test on a map at the guest's cell or on a global resource.
#[derive(Clone, Debug)]
pub enum Test {
    Map(usize, Cmp, f64),
    Global(usize, Cmp, f64),
}

#[derive(Clone, Debug)]
pub struct GuestDef {
    pub id: String,
    pub name: String,
    pub weight: f64,
    pub purse: (f64, f64),
    pub stay: (u32, u32),
    pub patience: u32,
    /// (need, growth per tick, starting value)
    pub needs: Vec<(usize, f64, f64)>,
    /// Each tick at a cell where one of these holds, mood drops by `amount`.
    pub dislikes: Vec<(Test, f64)>,
    /// All must hold for this guest to arrive.
    pub when: Vec<Test>,
    /// Map emissions at the guest's cell every tick.
    pub emits: Vec<(usize, f64, u32)>,
    pub model: String,
}

/// A compiled ruleset.
#[derive(Clone, Debug, Default)]
pub struct Ruleset {
    pub rules: Rules,
    pub inn: InnDef,
    pub phases: Vec<PhaseDef>,
    pub rooms: Vec<RoomDef>,
    pub units: Vec<UnitKind>,
    pub needs: Vec<NeedDef>,
    pub services: Vec<ServiceDef>,
    pub guests: Vec<GuestDef>,
    /// Colours for the layers, by layer name (`colour` in a layer block): hints for clients.
    pub layer_colours: Vec<(String, String)>,
    /// The `look` block's lines, `(key, rest of the line)`: hints for clients, unread here.
    pub look: Vec<(String, String)>,
    /// The incremental layer, if the ruleset has one.
    pub idle: Option<IdleBridge>,
    pub idle_game: Option<IdleGame>,
    pub warnings: Vec<String>,
}

impl Ruleset {
    pub fn room(&self, id: &str) -> Option<usize> {
        self.rooms.iter().position(|r| r.id == id)
    }
    pub fn unit(&self, id: &str) -> Option<usize> {
        self.units.iter().position(|u| u.id == id)
    }
    pub fn need(&self, id: &str) -> Option<usize> {
        self.needs.iter().position(|n| n.id == id)
    }
    pub fn guest(&self, id: &str) -> Option<usize> {
        self.guests.iter().position(|g| g.id == id)
    }
}

/// Whether a file belongs to the inn (`*.inn`, `*.gbx`) or to its incremental layer
/// (`idle.toml`, `params.toml`, `*.idle`).
pub fn is_inn_file(name: &str) -> bool {
    name.ends_with(".inn") || name.ends_with(".gbx")
}

pub fn is_idle_file(name: &str) -> bool {
    name == "idle.toml" || name == "params.toml" || name.ends_with(".idle")
}

/// Compiles a ruleset from `(file name, text)` pairs, in order. OpenInn's declarations are read
/// first, from every file, so GlassBox rules can name units declared in any file. The incremental
/// layer's files go to OpenIdle.
pub fn load(all: &[(String, String)]) -> Result<Ruleset, Error> {
    let files: Vec<(String, String)> = all
        .iter()
        .filter(|(n, _)| is_inn_file(n))
        .cloned()
        .collect();
    let files = &files[..];
    let mut kinds: Vec<&str> = glassbox::KINDS.to_vec();
    kinds.extend_from_slice(&HOST_KINDS);
    let mut blocks: Vec<(String, Block)> = Vec::new();
    for (name, text) in files {
        for b in split_blocks(name, tokenize(text), &kinds)? {
            if HOST_KINDS.contains(&b.kind.as_str()) {
                blocks.push((name.clone(), b));
            }
        }
    }
    let mut rs = Ruleset::default();
    // Declarations that others refer to, kind by kind.
    for kind in ["layer", "room", "need", "unit"] {
        for (f, b) in blocks.iter().filter(|(_, b)| b.kind == kind) {
            match kind {
                "layer" => layer(&mut rs, f, b)?,
                "room" => room(&mut rs, f, b)?,
                "need" => need(&mut rs, f, b)?,
                _ => unit(&mut rs, f, b)?,
            }
        }
    }
    // GlassBox rules (they may name the units now).
    for (name, text) in files {
        let (w, _) = compile(&mut rs.rules, name, text, &HOST_KINDS)?;
        rs.warnings.extend(w);
    }
    for kind in ["inn", "phase", "service", "guest", "look", "idle"] {
        for (f, b) in blocks.iter().filter(|(_, b)| b.kind == kind) {
            match kind {
                "inn" => inn(&mut rs, f, b)?,
                "phase" => phase(&mut rs, f, b)?,
                "service" => service(&mut rs, f, b)?,
                "look" => look(&mut rs, f, b)?,
                "idle" => idle(&mut rs, f, b)?,
                _ => guest(&mut rs, f, b)?,
            }
        }
    }
    check(&rs)?;
    idle_game(&mut rs, all)?;
    Ok(rs)
}

/// Loads every `*.gbx` and `*.inn` file of a folder, in name order.
pub fn load_dir(dir: &std::path::Path) -> Result<Ruleset, Error> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| Error::new(&dir.display().to_string(), 0, e.to_string()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| is_inn_file(n) || is_idle_file(n))
        })
        .collect();
    names.sort();
    let mut files = Vec::new();
    for p in names {
        let text = std::fs::read_to_string(&p)
            .map_err(|e| Error::new(&p.display().to_string(), 0, e.to_string()))?;
        files.push((p.file_name().unwrap().to_string_lossy().into_owned(), text));
    }
    load(&files)
}

// ---------- helpers ----------

fn err(f: &str, l: &Line, m: impl Into<String>) -> Error {
    Error::new(f, l.no, m)
}

/// The words after the key, joined: free text such as names.
fn text(l: &Line) -> String {
    l.words[1..].join(" ")
}

fn word<'a>(f: &str, l: &'a Line, i: usize) -> Result<&'a str, Error> {
    l.words
        .get(i)
        .map(String::as_str)
        .ok_or_else(|| err(f, l, format!("{} is incomplete", l.words.join(" "))))
}

fn global(rs: &mut Rules, f: &str, l: &Line, id: &str) -> Result<usize, Error> {
    rs.declare_resource(id, Scope::Global)
        .map_err(|m| err(f, l, m))
}

/// `map X <cmp> v` or `global X <cmp> v`, from word `i`.
fn test(rs: &mut Ruleset, f: &str, l: &Line, i: usize) -> Result<Test, Error> {
    let what = word(f, l, i)?;
    let id = word(f, l, i + 1)?;
    let op = word(f, l, i + 2)?;
    let cmp = Cmp::parse(op).ok_or_else(|| {
        err(
            f,
            l,
            format!("expected atLeast, atMost, greater, less or is, found {op:?}"),
        )
    })?;
    let v = number(f, l, i + 3)?;
    match what {
        "map" => {
            let m = rs
                .rules
                .map(id)
                .ok_or_else(|| err(f, l, format!("there is no layer {id:?}")))?;
            Ok(Test::Map(m, cmp, v))
        }
        "global" => Ok(Test::Global(global(&mut rs.rules, f, l, id)?, cmp, v)),
        _ => Err(err(f, l, format!("expected map or global, found {what:?}"))),
    }
}

/// `map X out amount [radius r]`.
fn emission(rs: &Ruleset, f: &str, l: &Line) -> Result<(usize, f64, u32), Error> {
    let id = word(f, l, 1)?;
    if word(f, l, 2)? != "out" {
        return Err(err(f, l, "only `map <layer> out <amount>` is allowed here"));
    }
    let m = rs
        .rules
        .map(id)
        .ok_or_else(|| err(f, l, format!("there is no layer {id:?}")))?;
    let radius = match l.words.iter().position(|w| w == "radius") {
        Some(i) => number(f, l, i + 1)?.max(0.0) as u32,
        None => 0,
    };
    Ok((m, number(f, l, 3)?, radius))
}

fn unknown(f: &str, b: &Block, l: &Line) -> Error {
    err(
        f,
        l,
        format!(
            "{} {}: unknown line {:?}",
            b.kind,
            b.name,
            l.words.join(" ")
        ),
    )
}

fn dup(f: &str, b: &Block) -> Error {
    Error::new(f, b.no, format!("{} {} is declared twice", b.kind, b.name))
}

// ---------- blocks ----------

fn layer(rs: &mut Ruleset, f: &str, b: &Block) -> Result<(), Error> {
    if rs.rules.map(&b.name).is_some() {
        return Err(dup(f, b));
    }
    let m = rs.rules.declare_map(&b.name);
    for l in &b.lines {
        if matches!(l.words[0].as_str(), "colour" | "color") {
            let c = format!("#{}", word(f, l, 1)?.trim_start_matches('#'));
            rs.layer_colours.push((b.name.clone(), c));
            continue;
        }
        let v = number(f, l, 1)?;
        let def = &mut rs.rules.maps[m];
        match l.words[0].as_str() {
            "decay" => def.decay = v.clamp(0.0, 1.0),
            "diffuse" => def.diffuse = v.clamp(0.0, 1.0),
            "min" => def.min = v,
            "max" => def.max = v,
            _ => return Err(unknown(f, b, l)),
        }
    }
    Ok(())
}

fn room(rs: &mut Ruleset, f: &str, b: &Block) -> Result<(), Error> {
    if rs.room(&b.name).is_some() {
        return Err(dup(f, b));
    }
    let mut r = RoomDef {
        id: b.name.clone(),
        name: b.name.clone(),
        cost: 0.0,
        colour: "#888888".into(),
        floor: String::new(),
        dwellings: Vec::new(),
    };
    for l in &b.lines {
        match l.words[0].as_str() {
            "name" => r.name = text(l),
            "cost" => r.cost = number(f, l, 1)?,
            // Hex without `#` (which starts a comment): `colour b5734a`.
            "colour" | "color" => r.colour = format!("#{}", word(f, l, 1)?.trim_start_matches('#')),
            "floor" => r.floor = word(f, l, 1)?.to_string(),
            "dwellings" => r
                .dwellings
                .extend(l.words[1..].iter().map(|w| dwellings_name(w))),
            _ => return Err(unknown(f, b, l)),
        }
    }
    rs.rooms.push(r);
    Ok(())
}

/// A Dwellings room name as rulesets write it: lower case, `-` for spaces.
pub fn dwellings_name(name: &str) -> String {
    name.trim().to_lowercase().replace([' ', '_'], "-")
}

fn need(rs: &mut Ruleset, f: &str, b: &Block) -> Result<(), Error> {
    if rs.need(&b.name).is_some() {
        return Err(dup(f, b));
    }
    let mut n = NeedDef {
        id: b.name.clone(),
        name: b.name.clone(),
        urgent: 50.0,
        max: 100.0,
    };
    for l in &b.lines {
        match l.words[0].as_str() {
            "name" => n.name = text(l),
            "urgent" => n.urgent = number(f, l, 1)?,
            "max" => n.max = number(f, l, 1)?,
            _ => return Err(unknown(f, b, l)),
        }
    }
    rs.needs.push(n);
    Ok(())
}

fn unit(rs: &mut Ruleset, f: &str, b: &Block) -> Result<(), Error> {
    if rs.unit(&b.name).is_some() {
        return Err(dup(f, b));
    }
    let mut name = b.name.clone();
    let (mut cost, mut seats, mut rooms, mut start, mut model) =
        (0.0, 1, Vec::new(), Vec::new(), String::new());
    let mut starts: Vec<(String, f64, &Line)> = Vec::new();
    let mut stairs = false;
    for l in &b.lines {
        match l.words[0].as_str() {
            "stairs" => stairs = true,
            "name" => name = text(l),
            "cost" => cost = number(f, l, 1)?,
            "seats" => seats = number(f, l, 1)?.max(0.0) as u32,
            "in" => {
                for id in &l.words[1..] {
                    rooms.push(
                        rs.room(id)
                            .ok_or_else(|| err(f, l, format!("there is no room {id:?}")))?,
                    );
                }
            }
            "start" => starts.push((word(f, l, 1)?.to_string(), number(f, l, 2)?, l)),
            "model" => model = text(l),
            _ => return Err(unknown(f, b, l)),
        }
    }
    let def = rs.rules.declare_unit(&b.name, &name);
    for (id, v, l) in starts {
        start.push((
            rs.rules
                .declare_resource(&id, Scope::Local)
                .map_err(|m| err(f, l, m))?,
            v,
        ));
    }
    rs.units.push(UnitKind {
        id: b.name.clone(),
        def,
        name,
        cost,
        rooms,
        seats,
        start,
        model,
        stairs,
    });
    Ok(())
}

fn inn(rs: &mut Ruleset, f: &str, b: &Block) -> Result<(), Error> {
    if !rs.inn.id.is_empty() {
        return Err(Error::new(
            f,
            b.no,
            format!("there is already an inn: {}", rs.inn.id),
        ));
    }
    let mut d = InnDef {
        id: b.name.clone(),
        name: b.name.clone(),
        review: 2.0,
        nights: 7,
        max_guests: 12,
        levels: 1,
        wall_cost: 4.0,
        door_cost: 8.0,
        ..Default::default()
    };
    let mut endless = None;
    let (mut money, mut rep, mut entrance_room) = (None, None, None);
    for l in &b.lines {
        match l.words[0].as_str() {
            "name" => d.name = text(l),
            "grid" => {
                d.width = number(f, l, 1)? as usize;
                d.height = number(f, l, 2)? as usize;
            }
            "entrance" => {
                d.entrance = (number(f, l, 1)? as usize, number(f, l, 2)? as usize);
                let id = word(f, l, 3)?;
                entrance_room = Some(
                    rs.room(id)
                        .ok_or_else(|| err(f, l, format!("there is no room {id:?}")))?,
                );
            }
            "money" => money = Some(global(&mut rs.rules, f, l, word(f, l, 1)?)?),
            "reputation" => {
                rep = Some(global(&mut rs.rules, f, l, word(f, l, 1)?)?);
                if l.words.len() > 2 {
                    d.review = number(f, l, 2)?;
                }
            }
            "start" => {
                let r = global(&mut rs.rules, f, l, word(f, l, 1)?)?;
                d.start.push((r, number(f, l, 2)?));
            }
            "nights" => d.nights = number(f, l, 1)?.max(1.0) as u32,
            "guests" => d.max_guests = number(f, l, 1)?.max(1.0) as usize,
            "levels" => {
                d.levels = number(f, l, 1)?.max(1.0) as usize;
                d.down = match l.words.get(2).map(String::as_str) {
                    None | Some("up") => false,
                    Some("down") => true,
                    Some(w) => {
                        return Err(err(f, l, format!("levels go `up` or `down`, not {w:?}")))
                    }
                };
            }
            "endless" => {
                endless = Some((
                    number(f, l, 1)?.max(1.0) as usize,
                    number(f, l, 2)?.max(1.0) as usize,
                    number(f, l, 3)?.max(1.0) as usize,
                    number(f, l, 4)?.max(1.0) as usize,
                ))
            }
            "wall" => d.wall_cost = number(f, l, 1)?,
            "door" => d.door_cost = number(f, l, 1)?,
            _ => return Err(unknown(f, b, l)),
        }
    }
    let at = |m: &str| Error::new(f, b.no, format!("inn {}: {m}", b.name));
    d.money = money.ok_or_else(|| at("needs `money <global resource>`"))?;
    d.reputation = rep.ok_or_else(|| at("needs `reputation <global resource>`"))?;
    d.entrance_room = entrance_room.ok_or_else(|| at("needs `entrance <x> <y> <room>`"))?;
    if d.width == 0 || d.height == 0 {
        return Err(at("needs `grid <width> <height>`"));
    }
    if d.entrance.0 >= d.width || d.entrance.1 >= d.height {
        return Err(at("the entrance is outside the grid"));
    }
    d.endless = endless.unwrap_or((d.width, d.height, d.levels.max(3), d.max_guests));
    if d.entrance.0 >= d.endless.0 || d.entrance.1 >= d.endless.1 {
        return Err(at("the entrance is outside the endless grid"));
    }
    rs.inn = d;
    Ok(())
}

fn phase(rs: &mut Ruleset, f: &str, b: &Block) -> Result<(), Error> {
    if rs.phases.iter().any(|p| p.id == b.name) {
        return Err(dup(f, b));
    }
    let mut p = PhaseDef {
        id: b.name.clone(),
        name: b.name.clone(),
        ..Default::default()
    };
    for l in &b.lines {
        match l.words[0].as_str() {
            "name" => p.name = text(l),
            "length" => p.length = number(f, l, 1)?.max(0.0) as u32,
            "build" => p.build = true,
            "open" => p.open = true,
            "night" => p.night = true,
            "set" => {
                let r = global(&mut rs.rules, f, l, word(f, l, 1)?)?;
                p.set.push((r, number(f, l, 2)?));
            }
            // arrive 0.05 [per Reputation 0.001]
            "arrive" => {
                p.arrive = number(f, l, 1)?;
                if l.words.get(2).map(String::as_str) == Some("per") {
                    let r = global(&mut rs.rules, f, l, word(f, l, 3)?)?;
                    p.arrive_per = Some((r, number(f, l, 4)?));
                }
            }
            _ => return Err(unknown(f, b, l)),
        }
    }
    rs.phases.push(p);
    Ok(())
}

fn service(rs: &mut Ruleset, f: &str, b: &Block) -> Result<(), Error> {
    let mut s = ServiceDef {
        id: b.name.clone(),
        name: b.name.clone(),
        units: Vec::new(),
        need: usize::MAX,
        relief: 0.0,
        takes: Vec::new(),
        price: 0.0,
        time: 1,
        emits: Vec::new(),
    };
    for l in &b.lines {
        match l.words[0].as_str() {
            "name" => s.name = text(l),
            "at" => {
                for id in &l.words[1..] {
                    s.units.push(
                        rs.unit(id)
                            .ok_or_else(|| err(f, l, format!("there is no unit {id:?}")))?,
                    );
                }
            }
            "need" => {
                let id = word(f, l, 1)?;
                s.need = rs
                    .need(id)
                    .ok_or_else(|| err(f, l, format!("there is no need {id:?}")))?;
                s.relief = number(f, l, 2)?;
            }
            "global" | "local" => {
                let scope = if l.words[0] == "global" {
                    Scope::Global
                } else {
                    Scope::Local
                };
                let id = word(f, l, 1)?;
                if word(f, l, 2)? != "in" {
                    return Err(err(
                        f,
                        l,
                        "a service only takes: `global <resource> in <amount>`",
                    ));
                }
                let resource = rs
                    .rules
                    .declare_resource(id, scope)
                    .map_err(|m| err(f, l, m))?;
                s.takes.push(Take {
                    resource,
                    amount: number(f, l, 3)?,
                });
            }
            "price" => s.price = number(f, l, 1)?,
            "time" => s.time = number(f, l, 1)?.max(1.0) as u32,
            "map" => s.emits.push(emission(rs, f, l)?),
            _ => return Err(unknown(f, b, l)),
        }
    }
    if s.units.is_empty() || s.need == usize::MAX {
        return Err(Error::new(
            f,
            b.no,
            format!(
                "service {} needs `at <unit>` and `need <need> <amount>`",
                b.name
            ),
        ));
    }
    rs.services.push(s);
    Ok(())
}

fn guest(rs: &mut Ruleset, f: &str, b: &Block) -> Result<(), Error> {
    if rs.guest(&b.name).is_some() {
        return Err(dup(f, b));
    }
    let mut g = GuestDef {
        id: b.name.clone(),
        name: b.name.clone(),
        weight: 1.0,
        purse: (10.0, 10.0),
        stay: (60, 60),
        patience: 30,
        needs: Vec::new(),
        dislikes: Vec::new(),
        when: Vec::new(),
        emits: Vec::new(),
        model: String::new(),
    };
    for l in &b.lines {
        match l.words[0].as_str() {
            "name" => g.name = text(l),
            "weight" => g.weight = number(f, l, 1)?.max(0.0),
            "purse" => g.purse = (number(f, l, 1)?, number(f, l, 2)?),
            "stay" => g.stay = (number(f, l, 1)? as u32, number(f, l, 2)? as u32),
            "patience" => g.patience = number(f, l, 1)?.max(1.0) as u32,
            "need" => {
                let id = word(f, l, 1)?;
                let n = rs
                    .need(id)
                    .ok_or_else(|| err(f, l, format!("there is no need {id:?}")))?;
                let start = if l.words.len() > 3 {
                    number(f, l, 3)?
                } else {
                    0.0
                };
                g.needs.push((n, number(f, l, 2)?, start));
            }
            // dislike map Noise greater 6 0.02
            "dislike" => {
                let t = test(rs, f, l, 1)?;
                g.dislikes.push((t, number(f, l, 5)?));
            }
            "if" => {
                let t = test(rs, f, l, 1)?;
                g.when.push(t);
            }
            "map" => g.emits.push(emission(rs, f, l)?),
            "model" => g.model = text(l),
            _ => return Err(unknown(f, b, l)),
        }
    }
    if g.purse.1 < g.purse.0 || g.stay.1 < g.stay.0 {
        return Err(Error::new(
            f,
            b.no,
            format!("guest {}: purse and stay are `low high`", b.name),
        ));
    }
    rs.guests.push(g);
    Ok(())
}

/// `look`: free `key values...` lines for the client; only duplicates are refused.
fn look(rs: &mut Ruleset, f: &str, b: &Block) -> Result<(), Error> {
    if !rs.look.is_empty() {
        return Err(Error::new(f, b.no, "there is more than one look block"));
    }
    for l in &b.lines {
        let key = l.words[0].clone();
        if rs.look.iter().any(|(k, _)| *k == key) {
            return Err(err(f, l, format!("look {}: {key} is given twice", b.name)));
        }
        rs.look.push((key, text(l)));
    }
    Ok(())
}

/// `idle`: how the inn meets its incremental layer. Names in the idle game are checked once it
/// is read (see [`idle_game`]).
fn idle(rs: &mut Ruleset, f: &str, b: &Block) -> Result<(), Error> {
    if rs.idle.is_some() {
        return Err(Error::new(f, b.no, "there is more than one idle block"));
    }
    let mut d = IdleBridge {
        id: b.name.clone(),
        player: "inn".into(),
        tick: 0.25,
        ..Default::default()
    };
    let rest = |l: &Line, from: usize| l.words[from.min(l.words.len())..].join(" ");
    for l in &b.lines {
        match l.words[0].as_str() {
            "player" => d.player = word(f, l, 1)?.to_string(),
            "tick" => d.tick = number(f, l, 1)?.max(0.0),
            "share" => {
                let g = global(&mut rs.rules, f, l, word(f, l, 1)?)?;
                let res = l.words.get(2).unwrap_or(&l.words[1]).clone();
                d.share.push((g, res));
            }
            "feed" => {
                let res = word(f, l, 1)?.to_string();
                let ev = match word(f, l, 2)? {
                    "arrived" => FeedEvent::Arrived,
                    "served" => FeedEvent::Served,
                    "earned" => FeedEvent::Earned,
                    "happy" => FeedEvent::Happy,
                    "angry" => FeedEvent::Angry,
                    "night" => FeedEvent::Night,
                    w => {
                        return Err(err(
                            f,
                            l,
                            format!(
                            "feed from arrived, served, earned, happy, angry or night, not {w:?}"
                        ),
                        ))
                    }
                };
                let amount = if l.words.len() > 3 {
                    number(f, l, 3)?
                } else {
                    1.0
                };
                d.feeds.push((res, ev, amount));
            }
            "price" => {
                let target = word(f, l, 1)?;
                let s = if target == "*" {
                    None
                } else {
                    Some(
                        rs.services
                            .iter()
                            .position(|s| s.id == target)
                            .ok_or_else(|| err(f, l, format!("there is no service {target:?}")))?,
                    )
                };
                d.prices.push((s, rest(l, 2)));
            }
            "arrive" => d.arrive.push(rest(l, 1)),
            "patience" => d.patience.push(rest(l, 1)),
            "unlock" => {
                let id = word(f, l, 1)?;
                let what = rs
                    .unit(id)
                    .map(Lockable::Unit)
                    .or_else(|| rs.room(id).map(Lockable::Room))
                    .ok_or_else(|| err(f, l, format!("there is no unit or room {id:?}")))?;
                d.unlocks.push((what, rest(l, 2)));
            }
            "set" => {
                let g = global(&mut rs.rules, f, l, word(f, l, 1)?)?;
                d.sets.push((g, rest(l, 2)));
            }
            "restart" => d.restart.extend(l.words[1..].iter().cloned()),
            _ => return Err(unknown(f, b, l)),
        }
    }
    rs.idle = Some(d);
    Ok(())
}

/// Reads the incremental layer's OpenIdle game and checks that the `idle` block's names and
/// expressions make sense in it.
fn idle_game(rs: &mut Ruleset, all: &[(String, String)]) -> Result<(), Error> {
    let Some(bridge) = &rs.idle else {
        return Ok(());
    };
    let mut files = std::collections::BTreeMap::new();
    for (name, text) in all.iter().filter(|(n, _)| is_idle_file(n)) {
        let key = if name == "idle.toml" {
            "game.toml"
        } else {
            name.as_str()
        };
        files.insert(key.to_string(), text.clone());
    }
    if !files.contains_key("game.toml") {
        return Err(Error::new(
            "idle.toml",
            0,
            format!(
                "idle {}: the ruleset has no idle.toml for its incremental layer",
                bridge.id
            ),
        ));
    }
    let game = openidle::Game::from_files(&files, &[]).map_err(|e| {
        let file = if e.file == "game.toml" {
            "idle.toml"
        } else {
            e.file.as_str()
        };
        Error::new(file, e.line, e.msg)
    })?;
    let game = std::sync::Arc::new(game);
    let at = |m: String| Error::new("idle", 0, format!("idle {}: {m}", bridge.id));
    let is_res = |id: &str| matches!(game.thing(id), Some(openidle::expr::Thing::Res(_)));
    for (_, res) in &bridge.share {
        if !is_res(res) {
            return Err(at(format!("share: the idle game has no resource {res:?}")));
        }
    }
    for (res, _, _) in &bridge.feeds {
        if !is_res(res) {
            return Err(at(format!("feed: the idle game has no resource {res:?}")));
        }
    }
    for p in &bridge.restart {
        if !matches!(game.thing(p), Some(openidle::expr::Thing::Pres(_))) {
            return Err(at(format!("restart: the idle game has no prestige {p:?}")));
        }
    }
    let mut probe = openidle::World::new(game.clone(), 1);
    probe.join(&bridge.player, &bridge.player);
    let exprs = bridge
        .prices
        .iter()
        .map(|(_, e)| ("price", e))
        .chain(bridge.arrive.iter().map(|e| ("arrive", e)))
        .chain(bridge.patience.iter().map(|e| ("patience", e)))
        .chain(bridge.unlocks.iter().map(|(_, e)| ("unlock", e)))
        .chain(bridge.sets.iter().map(|(_, e)| ("set", e)));
    for (key, e) in exprs {
        probe
            .eval(Some(&bridge.player), e)
            .map_err(|m| at(format!("{key} {e}: {m}")))?;
    }
    rs.idle_game = Some(IdleGame(game));
    Ok(())
}

fn check(rs: &Ruleset) -> Result<(), Error> {
    let whole = |m: &str| Error::new("ruleset", 0, m);
    if rs.inn.id.is_empty() {
        return Err(whole("there is no inn block"));
    }
    if rs.phases.is_empty() {
        return Err(whole("there are no phases"));
    }
    if !rs.phases.iter().any(|p| p.night) {
        return Err(whole(
            "no phase is marked `night`: the season would never end",
        ));
    }
    if rs.guests.is_empty() {
        return Err(whole("there are no guests"));
    }
    Ok(())
}

impl Ruleset {
    /// What an interface needs to know about the ruleset: names, costs, colours, models.
    pub fn catalog(&self) -> serde_json::Value {
        use serde_json::json;
        let res = |r: usize| self.rules.resources[r].id.clone();
        json!({
            "inn": {
                "id": self.inn.id, "name": self.inn.name,
                "width": self.inn.width, "height": self.inn.height,
                "entrance": [self.inn.entrance.0, self.inn.entrance.1],
                "money": res(self.inn.money), "reputation": res(self.inn.reputation),
                "nights": self.inn.nights, "guests": self.inn.max_guests,
                "levels": self.inn.levels, "down": self.inn.down,
                "endless": [self.inn.endless.0, self.inn.endless.1, self.inn.endless.2, self.inn.endless.3],
                "wall": self.inn.wall_cost, "door": self.inn.door_cost,
            },
            "phases": self.phases.iter().map(|p| json!({
                "id": p.id, "name": p.name, "length": p.length, "build": p.build, "open": p.open, "night": p.night,
            })).collect::<Vec<_>>(),
            "layers": self.rules.maps.iter().map(|m| json!({
                "id": m.id, "max": if m.max.is_finite() { m.max } else { 0.0 },
                "colour": self.layer_colours.iter().find(|(id, _)| *id == m.id).map(|(_, c)| c.clone()),
            })).collect::<Vec<_>>(),
            "idle": self.idle.as_ref().zip(self.idle_game.as_ref()).map(|(b, g)| json!({
                "id": b.id, "name": g.0.name, "player": b.player,
                "tick": g.0.tick, "offline_max": g.0.offline_max, "restart": b.restart,
            })),
            "look": self.look.iter().map(|(k, v)| (k.clone(), serde_json::Value::from(v.clone()))).collect::<serde_json::Map<_, _>>(),
            "rooms": self.rooms.iter().map(|r| json!({
                "id": r.id, "name": r.name, "cost": r.cost, "colour": r.colour, "floor": r.floor,
                "dwellings": r.dwellings,
            })).collect::<Vec<_>>(),
            "units": self.units.iter().map(|u| json!({
                "id": u.id, "name": u.name, "cost": u.cost, "seats": u.seats, "model": u.model,
                "stairs": u.stairs,
                "rooms": u.rooms.iter().map(|&r| self.rooms[r].id.clone()).collect::<Vec<_>>(),
                "services": self.services.iter().filter(|s| s.units.contains(&self.unit(&u.id).unwrap()))
                    .map(|s| s.id.clone()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "needs": self.needs.iter().map(|n| json!({ "id": n.id, "name": n.name, "urgent": n.urgent, "max": n.max })).collect::<Vec<_>>(),
            "services": self.services.iter().map(|s| json!({
                "id": s.id, "name": s.name, "need": self.needs[s.need].id, "price": s.price, "time": s.time,
            })).collect::<Vec<_>>(),
            "guests": self.guests.iter().map(|g| json!({ "id": g.id, "name": g.name, "model": g.model })).collect::<Vec<_>>(),
        })
    }
}
