//! Inner walls and doors, levels and stairs, the endless mode, and Dwellings export and import.
mod bot;

use openinn::dwellings;
use openinn::{load_dir, Command, Game, Mode, Side};
use serde_json::json;

fn dungeon() -> openinn::Ruleset {
    load_dir(&bot::dungeon_dir()).unwrap()
}

fn dig(x: usize, y: usize, z: usize, room: &str) -> Command {
    Command::Dig {
        x,
        y,
        z,
        room: room.into(),
    }
}

fn build(x: usize, y: usize, z: usize, unit: &str) -> Command {
    Command::Build {
        x,
        y,
        z,
        unit: unit.into(),
    }
}

fn wall(x: usize, y: usize, side: Side, door: bool) -> Command {
    Command::Wall {
        x,
        y,
        z: 0,
        side,
        door,
    }
}

fn gold(g: &Game) -> f64 {
    g.money()
}

#[test]
fn walls_go_between_dug_cells_and_come_down_for_half() {
    let mut g = Game::new(dungeon(), "w");
    // The entrance is (6, 0); nothing is dug south of it yet.
    assert!(g
        .apply(wall(6, 0, Side::S, false))
        .unwrap_err()
        .contains("two dug cells"));
    g.apply(dig(6, 1, 0, "taproom")).unwrap();
    let before = gold(&g);
    g.apply(wall(6, 0, Side::S, false)).unwrap();
    assert_eq!(gold(&g), before - 4.0);
    assert!(g
        .apply(wall(6, 1, Side::N, false))
        .unwrap_err()
        .contains("already a wall"));
    // A door replaces it, and is paid in full.
    g.apply(wall(6, 1, Side::N, true)).unwrap();
    assert_eq!(gold(&g), before - 12.0);
    let walls = g.snapshot().walls;
    assert_eq!(walls.len(), 1);
    assert_eq!(
        (walls[0].x, walls[0].y, walls[0].side, walls[0].door),
        (6, 0, Side::S, true)
    );
    g.apply(Command::Unwall {
        x: 6,
        y: 0,
        z: 0,
        side: Side::S,
    })
    .unwrap();
    assert_eq!(gold(&g), before - 8.0);
    assert!(g.snapshot().walls.is_empty());
}

/// The test bot's inn with the taproom walled off from the hall: nobody can reach the bar, so
/// nobody is served; with a door in the wall, they are.
#[test]
fn guests_walk_through_doors_but_not_walls() {
    let served = |door: Option<bool>| {
        let mut g = Game::new(dungeon(), "walls");
        for c in bot::plan().into_iter().take(14) {
            g.apply(c).unwrap();
        }
        // The hall is (6, 0) to (6, 2); the taproom is x 4..5, y 1..2, west of it.
        if let Some(door) = door {
            g.apply(wall(5, 1, Side::E, false)).unwrap();
            g.apply(wall(5, 2, Side::E, door)).unwrap();
        }
        g.apply(Command::Next).unwrap();
        g.apply(Command::Tick { n: 179 }).unwrap();
        g.stats.served
    };
    let open = served(None);
    assert!(open > 5, "{open}");
    assert_eq!(served(Some(false)), 0);
    assert!(served(Some(true)) > 5);
}

#[test]
fn stairs_lead_to_the_next_level() {
    let mut g = Game::new(dungeon(), "stairs");
    g.apply(dig(6, 1, 0, "hall")).unwrap();
    // Nothing leads down yet.
    assert!(g
        .apply(dig(6, 1, 1, "cellar"))
        .unwrap_err()
        .contains("stairs"));
    g.apply(build(6, 1, 0, "stairs")).unwrap();
    g.apply(dig(6, 1, 1, "cellar")).unwrap();
    g.apply(dig(6, 2, 1, "dorm")).unwrap();
    // The season has two levels: no stairs from the lower one.
    g.apply(dig(6, 3, 1, "hall")).unwrap();
    assert!(g
        .apply(build(6, 3, 1, "stairs"))
        .unwrap_err()
        .contains("no level below"));
    assert!(g
        .apply(dig(6, 1, 2, "cellar"))
        .unwrap_err()
        .contains("outside"));
    // A guest in need of rest finds the bunk downstairs.
    g.apply(build(6, 2, 1, "bunk")).unwrap();
    g.apply(Command::Next).unwrap();
    g.apply(Command::Tick { n: 179 }).unwrap();
    assert!(g.snapshot().cells.iter().any(|c| c.z == 1));
    assert!(g.stats.arrived > 0);
}

#[test]
fn the_endless_mode_has_no_last_night() {
    let rs = dungeon();
    let mut g = Game::with_mode(rs.clone(), "on", Mode::Endless);
    assert_eq!(g.snapshot().grid, [18, 14, 5]);
    let log = bot::season_with_nights(&mut g, &bot::plan(), 10);
    assert!(!g.over);
    assert!(log
        .iter()
        .any(|e| matches!(e, openinn::Event::Serve { .. })));
    // A deeper dig than the season allows.
    let mut g = Game::with_mode(rs, "deep", Mode::Endless);
    g.apply(dig(6, 1, 0, "hall")).unwrap();
    g.apply(build(6, 1, 0, "stairs")).unwrap();
    g.apply(dig(6, 1, 1, "hall")).unwrap();
    g.apply(build(6, 1, 1, "stairs")).unwrap();
    g.apply(dig(6, 1, 2, "cellar")).unwrap();
}

#[test]
fn a_plan_is_only_a_first_command() {
    let plan = Command::Plan {
        cells: vec![],
        walls: vec![],
        stairs: vec![],
    };
    let mut g = Game::new(dungeon(), "p");
    g.apply(dig(6, 1, 0, "hall")).unwrap();
    assert!(g.apply(plan.clone()).is_err());
    let mut g = Game::new(dungeon(), "p");
    g.apply(plan).unwrap();
}

/// An inn with rooms, inner walls, a door and stairs exports to Dwellings and imports back into
/// the same building.
#[test]
fn dwellings_round_trip() {
    let mut g = Game::new(dungeon(), "dw");
    for c in [
        dig(6, 1, 0, "hall"),
        dig(5, 1, 0, "taproom"),
        dig(4, 1, 0, "taproom"),
        dig(7, 1, 0, "dorm"),
        wall(6, 1, Side::E, true),
        wall(5, 1, Side::E, false),
        dig(6, 2, 0, "hall"),
        dig(5, 2, 0, "taproom"), // open to the hall: a doorway
        build(6, 2, 0, "stairs"),
        dig(6, 2, 1, "cellar"),
        dig(5, 2, 1, "cellar"),
    ] {
        g.apply(c).unwrap();
    }
    let doc = dwellings::export(&g);
    let floors = doc["floors"].as_array().unwrap();
    assert_eq!(floors.len(), 2);
    assert_eq!(floors[0]["level"], 0);
    assert_eq!(floors[1]["level"], -1); // the dungeon goes down
    assert_eq!(
        doc["exit"],
        json!({ "cell": { "i": 0, "j": 6 }, "dir": "n" })
    );
    let names: Vec<_> = floors[0]["rooms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"Taproom") && names.contains(&"Dormitory"),
        "{names:?}"
    );
    assert!(floors[0]["doors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["type"] == "REGULAR"));
    assert_eq!(floors[0]["stairs"][0]["up"], false);
    assert_eq!(floors[1]["stairs"][0]["up"], true);

    let imported = dwellings::import(&g.rs, Mode::Season, &doc).unwrap();
    assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
    let mut h = Game::new(dungeon(), "dw2");
    h.apply(imported.plan).unwrap();
    let key = |g: &Game| {
        let s = g.snapshot();
        let mut cells: Vec<_> = s
            .cells
            .iter()
            .map(|c| (c.x, c.y, c.z, c.room.clone()))
            .collect();
        cells.sort();
        let mut walls: Vec<_> = s
            .walls
            .iter()
            .map(|w| (w.x, w.y, w.z, w.side, w.door))
            .collect();
        walls.sort();
        let stairs: Vec<_> = s
            .units
            .iter()
            .map(|u| (u.x, u.y, u.z, u.unit.clone()))
            .collect();
        (cells, walls, stairs)
    };
    let (c1, w1, s1) = key(&g);
    let (c2, w2, s2) = key(&h);
    assert_eq!(c1, c2);
    assert_eq!(s1, s2);
    // Rooms come back walled from each other: the door stays a door, the wall a wall, and the
    // doorway between the hall and the taproom stays open.
    assert!(w2.contains(&(6, 1, 0, Side::E, true)), "{w2:?}");
    assert!(w2.contains(&(5, 1, 0, Side::E, false)), "{w2:?}");
    assert_eq!(w1, w2);
}

/// A house as Dwellings writes it: the exit on the south side of the ground floor, rooms named
/// in Dwellings' words. It is turned so the exit faces north and lands on the inn's entrance.
#[test]
fn a_dwellings_house_is_turned_and_placed_at_the_entrance() {
    let doc = json!({
        "floors": [{
            "level": 0,
            "rooms": [
                { "name": "Hall", "cells": [{ "i": 3, "j": 1 }, { "i": 2, "j": 1 }] },
                { "name": "Kitchen", "cells": [{ "i": 2, "j": 0 }, { "i": 1, "j": 0 }] },
                { "name": "Living room", "cells": [{ "i": 1, "j": 1 }, { "i": 0, "j": 1 }] }
            ],
            "doors": [
                { "edge": { "cell": { "i": 2, "j": 1 }, "dir": "w" }, "type": "REGULAR" },
                { "edge": { "cell": { "i": 2, "j": 1 }, "dir": "n" }, "type": "DOORWAY" },
                { "edge": { "cell": { "i": 3, "j": 1 }, "dir": "s" }, "type": "REGULAR" }
            ],
            "windows": [],
            "stairs": []
        }],
        "exit": { "cell": { "i": 3, "j": 1 }, "dir": "s" }
    });
    let rs = dungeon();
    let im = dwellings::import(&rs, Mode::Season, &doc).unwrap();
    let mut g = Game::new(rs, "house");
    g.apply(im.plan).unwrap();
    let s = g.snapshot();
    let at = |x, y| {
        s.cells
            .iter()
            .find(|c| (c.x, c.y, c.z) == (x, y, 0))
            .map(|c| c.room.as_str())
    };
    // Turned half way round: the exit cell is the entrance (6, 0), the hall runs south to (6, 1),
    // the living room (a taproom) beyond it, the kitchen to the east.
    assert_eq!(at(6, 0), Some("hall"));
    assert_eq!(at(6, 1), Some("hall"));
    assert_eq!(at(6, 2), Some("taproom"));
    assert_eq!(at(6, 3), Some("taproom"));
    assert_eq!(at(7, 1), Some("kitchen"));
    assert_eq!(at(7, 2), Some("kitchen"));
    assert_eq!(s.cells.len(), 6);
    // The kitchen door is a door; the doorway to the living room is open.
    assert!(s
        .walls
        .iter()
        .any(|w| (w.x, w.y, w.side, w.door) == (6, 1, Side::E, true)));
    assert!(!s
        .walls
        .iter()
        .any(|w| (w.x, w.y, w.side) == (6, 1, Side::S)));
    assert!(s
        .walls
        .iter()
        .any(|w| (w.x, w.y, w.side, w.door) == (6, 2, Side::E, false)));
}

/// Real exports from the Dwellings app import into every ruleset in both modes, everything they
/// build can be reached, and exporting the result and importing it again gives the same inn.
#[test]
fn real_dwellings_houses_import_and_round_trip() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/dwellings");
    let games = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games");
    for house in ["two-floors.json", "five-floors.json"] {
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(house)).unwrap()).unwrap();
        for game in ["dungeon-inn", "slow-orbit", "village-inn"] {
            let rs = load_dir(&games.join(game)).unwrap();
            for mode in [Mode::Season, Mode::Endless] {
                let what = format!("{house} in {game} ({mode:?})");
                let im =
                    dwellings::import(&rs, mode, &doc).unwrap_or_else(|e| panic!("{what}: {e}"));
                assert!(
                    !im.warnings.iter().any(|w| w.contains("reached")),
                    "{what}: {:?}",
                    im.warnings
                );
                let mut g = Game::with_mode(rs.clone(), "h", mode);
                g.apply(im.plan).unwrap_or_else(|e| panic!("{what}: {e}"));
                let s = g.snapshot();
                assert!(s.cells.len() > 15, "{what}: {} cells", s.cells.len());
                assert!(s.cells.iter().any(|c| c.z == 1), "{what}: one level only");
                // Out and back in.
                let again = dwellings::import(&rs, mode, &dwellings::export(&g)).unwrap();
                let mut h = Game::with_mode(rs.clone(), "h", mode);
                h.apply(again.plan).unwrap();
                let key = |g: &Game| {
                    let s = g.snapshot();
                    let mut c: Vec<_> = s
                        .cells
                        .iter()
                        .map(|c| (c.x, c.y, c.z, c.room.clone()))
                        .collect();
                    c.sort();
                    let u: Vec<_> = s.units.iter().map(|u| (u.x, u.y, u.z)).collect();
                    (c, u)
                };
                assert_eq!(key(&g), key(&h), "{what}");
            }
        }
    }
}
