#![allow(dead_code)]
//! A plain innkeeper for tests and balance runs: each day it works down a fixed plan of digging
//! and building, doing what it can afford, then opens the doors and lets the night run.

use openinn::{Command, Event, Game};

/// The Gilded Mimic's plan around the stairs at (6, 0): a hall running south, the taproom to
/// the west with the kitchen and cellar behind it, the dormitory to the east.
pub fn plan() -> Vec<Command> {
    let dig = |x, y, room: &str| Command::Dig {
        x,
        y,
        room: room.into(),
    };
    let build = |x, y, unit: &str| Command::Build {
        x,
        y,
        unit: unit.into(),
    };
    vec![
        dig(6, 1, "hall"),
        dig(6, 2, "hall"),
        dig(5, 1, "taproom"),
        dig(5, 2, "taproom"),
        dig(4, 1, "taproom"),
        dig(4, 2, "taproom"),
        dig(3, 1, "kitchen"),
        dig(2, 1, "kitchen"),
        dig(3, 2, "cellar"),
        build(5, 1, "bar"),
        build(4, 2, "table"),
        build(3, 1, "hearth"),
        build(3, 2, "kegs"),
        build(2, 1, "larder"),
        // Day 2 and on, as money allows.
        dig(6, 3, "hall"),
        dig(7, 2, "dorm"),
        dig(7, 3, "dorm"),
        build(7, 2, "bunk"),
        build(7, 3, "bunk"),
        dig(5, 3, "taproom"),
        dig(4, 3, "taproom"),
        build(5, 3, "table"),
        build(4, 3, "table"),
        build(6, 2, "washtub"),
        dig(8, 2, "dorm"),
        dig(8, 3, "dorm"),
        build(8, 2, "bed"),
        build(8, 3, "bed"),
        dig(5, 4, "taproom"),
        build(5, 4, "bar"),
        dig(9, 2, "dorm"),
        build(9, 2, "bed"),
    ]
}

/// Gold the bot keeps back for the night's ale and provisions.
pub const RESERVE: f64 = 40.0;

fn cost(g: &Game, c: &Command) -> f64 {
    match c {
        Command::Dig { room, .. } => g.rs.room(room).map_or(0.0, |r| g.rs.rooms[r].cost),
        Command::Build { unit, .. } => g.rs.unit(unit).map_or(0.0, |u| g.rs.units[u].cost),
        _ => 0.0,
    }
}

/// Plays a whole season; returns every event.
pub fn season(g: &mut Game) -> Vec<Event> {
    let plan = plan();
    let mut done = vec![false; plan.len()];
    let mut log = Vec::new();
    while !g.over {
        if g.snapshot().build {
            for (i, c) in plan.iter().enumerate() {
                if done[i] || g.money() - cost(g, c) < RESERVE {
                    continue;
                }
                if let Ok(e) = g.apply(c.clone()) {
                    log.extend(e);
                    done[i] = true;
                }
            }
            log.extend(g.apply(Command::Next).expect("next"));
        } else {
            log.extend(g.apply(Command::Tick { n: 20 }).expect("tick"));
        }
    }
    log
}
