#![allow(dead_code)]
//! A plain innkeeper for tests and balance runs: each day it works down a ruleset's plan of
//! digging and building (`bot.plan`, `dig x y [z] room`, `build x y [z] unit`), doing what it can afford, then opens the doors and lets the
//! night run.

use openinn::{Command, Event, Game};
use std::path::{Path, PathBuf};

/// A ruleset's plan, from its `bot.plan`: `dig x y room` and `build x y unit` lines, in order.
pub fn plan_for(dir: &Path) -> Vec<Command> {
    let text = std::fs::read_to_string(dir.join("bot.plan"))
        .unwrap_or_else(|e| panic!("{}: {e}", dir.join("bot.plan").display()));
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let w: Vec<&str> = line.split('#').next().unwrap().split_whitespace().collect();
        if w.is_empty() {
            continue;
        }
        let n = |k: usize| -> usize {
            w.get(k)
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| panic!("bot.plan line {}: {line:?}", i + 1))
        };
        // `dig x y room`, or `dig x y z room` on another level.
        let (z, at) = match w.get(3).and_then(|v| v.parse().ok()) {
            Some(z) => (z, 4),
            None => (0, 3),
        };
        let name = w.get(at).map(|s| s.to_string()).unwrap_or_default();
        out.push(match w[0] {
            "dig" => Command::Dig {
                x: n(1),
                y: n(2),
                z,
                room: name,
            },
            "build" => Command::Build {
                x: n(1),
                y: n(2),
                z,
                unit: name,
            },
            _ => panic!("bot.plan line {}: {line:?}", i + 1),
        });
    }
    out
}

/// The Gilded Mimic's folder.
pub fn dungeon_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/dungeon-inn")
}

/// The Gilded Mimic's plan.
pub fn plan() -> Vec<Command> {
    plan_for(&dungeon_dir())
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

/// Plays a whole season of The Gilded Mimic; returns every event.
pub fn season(g: &mut Game) -> Vec<Event> {
    season_with(g, &plan())
}

/// Plays a whole season following a plan; returns every event.
pub fn season_with(g: &mut Game, plan: &[Command]) -> Vec<Event> {
    season_with_nights(g, plan, u32::MAX)
}

/// Plays following a plan until the season ends or this many nights are done.
pub fn season_with_nights(g: &mut Game, plan: &[Command], nights: u32) -> Vec<Event> {
    let mut done = vec![false; plan.len()];
    let mut log = Vec::new();
    while !g.over && g.nights_done < nights {
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
