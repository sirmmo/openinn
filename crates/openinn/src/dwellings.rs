//! Buildings in the JSON format of watabou's Dwellings generator
//! (<https://watabou.github.io/dwellings/>, "Export as JSON"):
//!
//! ```text
//! { "floors": [ { "level": 0,
//!                 "rooms":   [ { "name": "Kitchen", "cells": [ {"i": 2, "j": 5}, ... ] } ],
//!                 "doors":   [ { "edge": { "cell": {"i": 2, "j": 5}, "dir": "n" }, "type": "REGULAR" } ],
//!                 "windows": [ { "cell": {...}, "dir": "e" } ],
//!                 "stairs":  [ { "cell": {...}, "dir": "s", "up": true } ] } ],
//!   "exit": { "cell": {...}, "dir": "s" } }
//! ```
//!
//! `i` is the row (north is `i - 1`) and `j` the column; an edge is the side `dir` of `cell`.
//! Floors are levels 0 (the ground floor) and up, then a basement at level -1. Rooms are walled
//! from each other; doors are `REGULAR` (a door) or `DOORWAY` (an opening).
//!
//! In an inn, a room is a connected area of cells of one room kind not divided by walls. Levels
//! go up (level = z) or down (level = -z) as the ruleset's `levels` say; an import stacks the
//! house's floors the inn's way (see `import`).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde_json::{json, Value};

use crate::script::{dwellings_name, Ruleset};
use crate::sim::{Command, Game, Mode, PlanAt, PlanCell, PlanWall, Side};

fn side_of(d: &str) -> Option<Side> {
    match d {
        "n" => Some(Side::N),
        "e" => Some(Side::E),
        "s" => Some(Side::S),
        "w" => Some(Side::W),
        _ => None,
    }
}

fn cell_json(x: usize, y: usize) -> Value {
    json!({ "i": y, "j": x })
}

fn edge_json(x: usize, y: usize, side: Side) -> Value {
    json!({ "cell": cell_json(x, y), "dir": side.letter() })
}

/// The inn's building as a Dwellings JSON document.
pub fn export(g: &Game) -> Value {
    let snap = g.snapshot();
    let rs = &g.rs;
    let [w, h, levels] = snap.grid;
    let idx = |x: usize, y: usize, z: usize| (z * h + y) * w + x;
    let mut room: Vec<Option<usize>> = vec![None; w * h * levels];
    for c in &snap.cells {
        room[idx(c.x, c.y, c.z)] = rs.room(&c.room);
    }
    let walls: BTreeMap<(usize, usize, usize, Side), bool> = snap
        .walls
        .iter()
        .map(|v| ((v.x, v.y, v.z, v.side), v.door))
        .collect();
    // The wall, if any, on side `s` of (x, y, z): walls are kept on east and south sides.
    let wall_at = |x: usize, y: usize, z: usize, s: Side| -> Option<bool> {
        match s {
            Side::E | Side::S => walls.get(&(x, y, z, s)).copied(),
            Side::W if x > 0 => walls.get(&(x - 1, y, z, Side::E)).copied(),
            Side::N if y > 0 => walls.get(&(x, y - 1, z, Side::S)).copied(),
            _ => None,
        }
    };
    let next = |x: usize, y: usize, s: Side| -> Option<(usize, usize)> {
        let (dx, dy) = s.delta();
        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
        (nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h)
            .then_some((nx as usize, ny as usize))
    };
    let stairs: Vec<(usize, usize, usize)> = snap
        .units
        .iter()
        .filter(|u| rs.unit(&u.unit).is_some_and(|k| rs.units[k].stairs))
        .map(|u| (u.x, u.y, u.z))
        .collect();

    let mut floors = Vec::new();
    for z in 0..levels {
        // Rooms: connected areas of one kind, not crossing walls or doors.
        let mut comp = vec![usize::MAX; w * h];
        let mut rooms = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let Some(kind) = room[idx(x, y, z)] else {
                    continue;
                };
                if comp[y * w + x] != usize::MAX {
                    continue;
                }
                let id = rooms.len();
                let mut cells = Vec::new();
                let mut q = VecDeque::from([(x, y)]);
                comp[y * w + x] = id;
                while let Some((cx, cy)) = q.pop_front() {
                    cells.push(cell_json(cx, cy));
                    for s in Side::ALL {
                        let Some((nx, ny)) = next(cx, cy, s) else {
                            continue;
                        };
                        if comp[ny * w + nx] == usize::MAX
                            && room[idx(nx, ny, z)] == Some(kind)
                            && wall_at(cx, cy, z, s).is_none()
                        {
                            comp[ny * w + nx] = id;
                            q.push_back((nx, ny));
                        }
                    }
                }
                rooms.push(json!({ "name": rs.rooms[kind].name, "cells": cells }));
            }
        }
        if rooms.is_empty() {
            continue;
        }
        // Doors: every door, and one doorway where two rooms meet with no wall between them.
        let mut doors = Vec::new();
        let mut joined = BTreeSet::new();
        for y in 0..h {
            for x in 0..w {
                if room[idx(x, y, z)].is_none() {
                    continue;
                }
                for s in [Side::E, Side::S] {
                    let Some((nx, ny)) = next(x, y, s) else {
                        continue;
                    };
                    if room[idx(nx, ny, z)].is_none() {
                        continue;
                    }
                    let (a, b) = (comp[y * w + x], comp[ny * w + nx]);
                    match wall_at(x, y, z, s) {
                        Some(true) => {
                            doors.push(json!({ "edge": edge_json(x, y, s), "type": "REGULAR" }))
                        }
                        Some(false) => {}
                        None if a != b && joined.insert((a.min(b), a.max(b))) => {
                            doors.push(json!({ "edge": edge_json(x, y, s), "type": "DOORWAY" }))
                        }
                        None => {}
                    }
                }
            }
        }
        let level = if rs.inn.down { -(z as i64) } else { z as i64 };
        let mut st = Vec::new();
        for &(x, y, sz) in &stairs {
            // Stairs join sz and sz + 1; each floor lists the flight that leaves it.
            let further = !rs.inn.down; // going to sz + 1 is going up in an inn that grows up
            if sz == z {
                st.push(json!({ "cell": cell_json(x, y), "dir": "n", "up": further }));
            } else if sz + 1 == z {
                st.push(json!({ "cell": cell_json(x, y), "dir": "n", "up": !further }));
            }
        }
        floors.push(json!({
            "level": level,
            "rooms": rooms,
            "doors": doors,
            "windows": [],
            "stairs": st,
        }));
    }
    let (ex, ey) = rs.inn.entrance;
    json!({ "floors": floors, "exit": edge_json(ex, ey, Side::N) })
}

/// What an import made of a Dwellings document: the plan to build, and what was left out.
#[derive(Debug)]
pub struct Import {
    pub plan: Command,
    pub warnings: Vec<String>,
}

fn int(v: &Value, k: &str) -> Result<i64, String> {
    v.get(k)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("`{k}` is missing or not a whole number"))
}

fn ij(v: &Value) -> Result<(i64, i64), String> {
    Ok((int(v, "j")?, int(v, "i")?))
}

/// Turns a Dwellings document into a plan for a new game of this ruleset and mode: the house
/// is turned so its exit faces north and moved so the exit is the inn's entrance; each room
/// becomes the room kind that lists its name in `dwellings` (else the one with that name, else
/// the one listing `*`, else the entrance's); rooms are walled from each other, with their doors;
/// stairs become the ruleset's first stairs unit. What does not fit, or cannot be reached from
/// the entrance, is left out, with a warning.
pub fn import(rs: &Ruleset, mode: Mode, doc: &Value) -> Result<Import, String> {
    let (gw, gh, levels) = match mode {
        Mode::Season => (rs.inn.width, rs.inn.height, rs.inn.levels),
        Mode::Endless => (rs.inn.endless.0, rs.inn.endless.1, rs.inn.endless.2),
    };
    let floors = doc
        .get("floors")
        .and_then(Value::as_array)
        .ok_or("this is not a Dwellings export: there are no floors")?;
    let mut warnings = Vec::new();

    // The exit, on the ground floor: turn it to the north, put it on the entrance.
    let exit = doc.get("exit").ok_or("this Dwellings export has no exit")?;
    let (exj, exi) = ij(exit.get("cell").ok_or("the exit has no cell")?)?;
    let exit_side = exit
        .get("dir")
        .and_then(Value::as_str)
        .and_then(side_of)
        .ok_or("the exit has no direction")?;
    let mut turns = 0;
    let mut s = exit_side;
    while s != Side::N {
        s = s.cw();
        turns += 1;
    }
    // A quarter turn clockwise on screen (y down): (x, y) -> (-y, x).
    let turn = |(x, y): (i64, i64)| -> (i64, i64) {
        let (mut x, mut y) = (x, y);
        for _ in 0..turns {
            (x, y) = (-y, x);
        }
        (x, y)
    };
    let turn_side = |mut s: Side| {
        for _ in 0..turns {
            s = s.cw();
        }
        s
    };
    let (tx, ty) = turn((exj, exi));
    let (ex, ey) = (rs.inn.entrance.0 as i64, rs.inn.entrance.1 as i64);
    let place = |j: i64, i: i64| {
        let (x, y) = turn((j, i));
        (x - tx + ex, y - ty + ey)
    };

    // Which level of the inn each floor becomes: the ground floor is the entrance's level, and
    // the floors on one side of it follow in the inn's direction. That is the side the inn grows
    // to (basements in a dungeon, upper floors in a tower), unless the house has more floors on
    // the other side: then those are turned round (a three-storey house becomes three levels
    // down in a dungeon). The floors on the other side are left out.
    let count = |up: bool| {
        floors
            .iter()
            .filter(|f| {
                let l = int(f, "level").unwrap_or(0);
                if up {
                    l > 0
                } else {
                    l < 0
                }
            })
            .count()
    };
    let (ups, downs) = (count(true), count(false));
    let turned = if rs.inn.down {
        ups > downs
    } else {
        downs > ups
    };
    let along: i64 = if rs.inn.down != turned { -1 } else { 1 };
    let level_z = |level: i64| -> Option<usize> {
        let z = level * along;
        (z >= 0 && (z as usize) < levels).then_some(z as usize)
    };

    // Room kinds by Dwellings name.
    let kind_for = |name: &str| -> usize {
        let n = dwellings_name(name);
        let by = |f: &dyn Fn(&crate::script::RoomDef) -> bool| rs.rooms.iter().position(f);
        by(&|r| r.dwellings.contains(&n))
            .or_else(|| by(&|r| dwellings_name(&r.id) == n || dwellings_name(&r.name) == n))
            .or_else(|| by(&|r| r.dwellings.iter().any(|d| d == "*")))
            .unwrap_or(rs.inn.entrance_room)
    };

    // Cells: (x, y, z) -> (room kind, Dwellings room number).
    let mut cells: BTreeMap<(usize, usize, usize), (usize, usize)> = BTreeMap::new();
    let mut doors: BTreeMap<(usize, usize, usize, Side), bool> = BTreeMap::new();
    let mut flights: Vec<(usize, usize, usize, usize)> = Vec::new(); // x, y, z from, z to
    let (mut outside, mut dropped_floors, mut room_no) = (0, 0, 0);
    for f in floors {
        let level = int(f, "level").unwrap_or(0);
        let Some(z) = level_z(level) else {
            dropped_floors += 1;
            continue;
        };
        for r in f
            .get("rooms")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = r.get("name").and_then(Value::as_str).unwrap_or("room");
            let kind = kind_for(name);
            for c in r
                .get("cells")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let (j, i) = ij(c)?;
                let (x, y) = place(j, i);
                if x < 0 || y < 0 || x as usize >= gw || y as usize >= gh {
                    outside += 1;
                    continue;
                }
                cells.insert((x as usize, y as usize, z), (kind, room_no));
            }
            room_no += 1;
        }
        for d in f
            .get("doors")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(e) = d.get("edge") else { continue };
            let (j, i) = ij(e.get("cell").ok_or("a door has no cell")?)?;
            let Some(side) = e.get("dir").and_then(Value::as_str).and_then(side_of) else {
                continue;
            };
            let (x, y) = place(j, i);
            if x < 0 || y < 0 {
                continue;
            }
            let regular = d.get("type").and_then(Value::as_str) == Some("REGULAR");
            doors.insert((x as usize, y as usize, z, turn_side(side)), regular);
        }
        for st in f
            .get("stairs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (j, i) = ij(st.get("cell").ok_or("stairs have no cell")?)?;
            let up = st.get("up").and_then(Value::as_bool).unwrap_or(true);
            let Some(to) = level_z(if up { level + 1 } else { level - 1 }) else {
                continue;
            };
            let (x, y) = place(j, i);
            if x >= 0 && y >= 0 && (x as usize) < gw && (y as usize) < gh {
                flights.push((x as usize, y as usize, z, to));
            }
        }
    }
    if dropped_floors > 0 {
        warnings.push(format!(
            "{dropped_floors} floor(s) did not fit in this inn's {levels} level(s) and were left out"
        ));
    }
    if outside > 0 {
        warnings.push(format!(
            "{outside} cell(s) fell outside the inn's grid and were left out"
        ));
    }
    let (ex, ey) = (ex as usize, ey as usize);
    if !cells.contains_key(&(ex, ey, 0)) {
        return Err("the house's exit is not on its ground floor".into());
    }

    // Stairs join the lower z of each flight to the next; both ends must be dug.
    let mut stairs = BTreeSet::new();
    for (x, y, a, b) in flights {
        let z = a.min(b);
        if a.abs_diff(b) != 1 {
            continue;
        }
        for zz in [z, z + 1] {
            if !cells.contains_key(&(x, y, zz)) {
                // The landing: as the room next to it, if there is one.
                let near = Side::ALL.iter().find_map(|s| {
                    let (dx, dy) = s.delta();
                    cells
                        .get(&((x as i64 + dx) as usize, (y as i64 + dy) as usize, zz))
                        .copied()
                });
                if let Some(v) = near {
                    cells.insert((x, y, zz), v);
                }
            }
        }
        if cells.contains_key(&(x, y, z)) && cells.contains_key(&(x, y, z + 1)) {
            stairs.insert((x, y, z));
        }
    }
    if !stairs.is_empty() && !rs.units.iter().any(|u| u.stairs) {
        warnings.push("this ruleset has no stairs, so only the ground floor can be reached".into());
        stairs.clear();
    }

    // Walls between rooms, with their doors; doorways stay open.
    let mut walls: BTreeMap<(usize, usize, usize, Side), bool> = BTreeMap::new();
    for (&(x, y, z), &(_, no)) in &cells {
        for s in [Side::E, Side::S] {
            let (dx, dy) = s.delta();
            let n = ((x as i64 + dx) as usize, (y as i64 + dy) as usize, z);
            let Some(&(_, other)) = cells.get(&n) else {
                continue;
            };
            if other == no {
                continue;
            }
            let opposite = match s {
                Side::E => Side::W,
                _ => Side::N,
            };
            let door = doors
                .get(&(x, y, z, s))
                .or_else(|| doors.get(&(n.0, n.1, z, opposite)))
                .copied();
            match door {
                Some(false) => {} // a doorway: no wall
                Some(true) => {
                    walls.insert((x, y, z, s), true);
                }
                None => {
                    walls.insert((x, y, z, s), false);
                }
            }
        }
    }

    // Keep what can be reached from the entrance.
    let mut seen = BTreeSet::from([(ex, ey, 0)]);
    let mut q = VecDeque::from([(ex, ey, 0)]);
    while let Some((x, y, z)) = q.pop_front() {
        let mut next = Vec::new();
        for s in Side::ALL {
            let (dx, dy) = s.delta();
            let (nx, ny) = (x as i64 + dx, y as i64 + dy);
            if nx < 0 || ny < 0 {
                continue;
            }
            let n = (nx as usize, ny as usize, z);
            let key = match s {
                Side::E | Side::S => (x, y, z, s),
                Side::W => (n.0, n.1, z, Side::E),
                Side::N => (n.0, n.1, z, Side::S),
            };
            if cells.contains_key(&n) && walls.get(&key) != Some(&false) {
                next.push(n);
            }
        }
        if stairs.contains(&(x, y, z)) {
            next.push((x, y, z + 1));
        }
        if z > 0 && stairs.contains(&(x, y, z - 1)) {
            next.push((x, y, z - 1));
        }
        for n in next {
            if seen.insert(n) {
                q.push_back(n);
            }
        }
    }
    let unreachable = cells.len() - seen.len().min(cells.len());
    if unreachable > 0 {
        warnings.push(format!(
            "{unreachable} cell(s) could not be reached from the entrance and were left out"
        ));
    }
    cells.retain(|k, _| seen.contains(k));
    walls.retain(|&(x, y, z, s), _| {
        let (dx, dy) = s.delta();
        seen.contains(&(x, y, z))
            && seen.contains(&((x as i64 + dx) as usize, (y as i64 + dy) as usize, z))
    });
    stairs.retain(|&(x, y, z)| seen.contains(&(x, y, z)) && seen.contains(&(x, y, z + 1)));

    let plan = Command::Plan {
        cells: cells
            .iter()
            .map(|(&(x, y, z), &(k, _))| PlanCell {
                x,
                y,
                z,
                room: rs.rooms[k].id.clone(),
            })
            .collect(),
        walls: walls
            .iter()
            .map(|(&(x, y, z, side), &door)| PlanWall {
                x,
                y,
                z,
                side,
                door,
            })
            .collect(),
        stairs: stairs.iter().map(|&(x, y, z)| PlanAt { x, y, z }).collect(),
    };
    Ok(Import { plan, warnings })
}
