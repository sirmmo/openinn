mod bot;

use openinn::{load, load_dir, Command, Event, Game, Ruleset};
use std::path::Path;

fn dungeon() -> Ruleset {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/dungeon-inn");
    match load_dir(&dir) {
        Ok(rs) => rs,
        Err(e) => panic!("{e}"),
    }
}

fn global(g: &Game, id: &str) -> f64 {
    g.snapshot()
        .globals
        .iter()
        .find(|(k, _)| k == id)
        .map(|(_, v)| *v)
        .unwrap()
}

#[test]
fn the_dungeon_inn_loads() {
    let rs = dungeon();
    assert_eq!(rs.inn.name, "The Gilded Mimic");
    assert_eq!(rs.phases.len(), 2);
    assert_eq!(rs.guests.len(), 8);
    assert_eq!(rs.rooms.len(), 12);
    assert!(rs.combos.len() >= 12, "{} combos", rs.combos.len());
    assert_eq!(rs.rooms[rs.room("taproom").unwrap()].colour, "#b5734a");
    assert!(rs.warnings.is_empty(), "{:?}", rs.warnings);
    assert_eq!(rs.rules.unit_rules.len(), 16);
}

#[test]
fn the_same_seed_and_commands_give_the_same_season() {
    let mut a = Game::new(dungeon(), "mimic");
    let mut b = Game::new(dungeon(), "mimic");
    bot::season(&mut a);
    bot::season(&mut b);
    let sa = serde_json::to_string(&a.snapshot()).unwrap();
    assert_eq!(sa, serde_json::to_string(&b.snapshot()).unwrap());
    // A save is the seed and the commands.
    let c = Game::replay(dungeon(), "mimic", &a.commands).unwrap();
    assert_eq!(sa, serde_json::to_string(&c.snapshot()).unwrap());
    // Another seed, another season.
    let mut d = Game::new(dungeon(), "kobold");
    bot::season(&mut d);
    assert_ne!(sa, serde_json::to_string(&d.snapshot()).unwrap());
}

#[test]
fn digging_and_building_follow_the_rules() {
    let mut g = Game::new(dungeon(), "x");
    let gold = global(&g, "Gold");
    let dig = |x, y, room: &str| Command::Dig {
        x,
        y,
        z: 0,
        room: room.into(),
    };
    assert!(g
        .apply(dig(0, 8, "hall"))
        .unwrap_err()
        .contains("next to the inn"));
    assert!(g
        .apply(dig(6, 0, "hall"))
        .unwrap_err()
        .contains("already dug"));
    g.apply(dig(6, 1, "taproom")).unwrap();
    assert_eq!(global(&g, "Gold"), gold - 10.0);
    let e = g
        .apply(Command::Build {
            x: 6,
            y: 1,
            z: 0,
            unit: "bunk".into(),
        })
        .unwrap_err();
    assert!(e.contains("Dormitory"), "{e}");
    g.apply(Command::Build {
        x: 6,
        y: 1,
        z: 0,
        unit: "bar".into(),
    })
    .unwrap();
    assert_eq!(global(&g, "Gold"), gold - 60.0);
    // Selling gives half back.
    g.apply(Command::Remove { x: 6, y: 1, z: 0 }).unwrap();
    assert_eq!(global(&g, "Gold"), gold - 35.0);
    // The day waits for the player; the night runs by itself and allows no digging.
    assert!(g.apply(Command::Tick { n: 1 }).is_err());
    g.apply(Command::Next).unwrap();
    assert_eq!(g.snapshot().phase, "night");
    assert_eq!(global(&g, "Night"), 1.0);
    assert!(g.apply(dig(6, 2, "hall")).is_err());
    assert!(g.apply(Command::Next).is_err());
}

#[test]
fn the_old_ruins_make_an_academy_and_a_colonnade() {
    let mut g = Game::new(dungeon(), "x");
    for (x, y, room) in [
        (6, 1, "library"),
        (6, 2, "library"),
        (7, 2, "library"),
        (5, 1, "gallery"),
        (4, 1, "gallery"),
        (5, 2, "gallery"),
    ] {
        g.apply(Command::Dig {
            x,
            y,
            z: 0,
            room: room.into(),
        })
        .unwrap();
    }
    for (x, y, unit) in [(6, 2, "desk"), (6, 1, "bookcase"), (5, 1, "statue")] {
        g.apply(Command::Build {
            x,
            y,
            z: 0,
            unit: unit.into(),
        })
        .unwrap();
    }
    let combos = |g: &Game, x: usize, y: usize| {
        let s = g.snapshot();
        s.units
            .iter()
            .find(|u| (u.x, u.y) == (x, y))
            .unwrap()
            .combos
            .clone()
    };
    // A desk by a bookcase, under a statue's gaze: the Academy (and no hushed corner with one bookcase).
    assert_eq!(combos(&g, 6, 2), vec!["academy".to_string()]);
    assert!(combos(&g, 5, 1).is_empty());
    g.apply(Command::Build {
        x: 4,
        y: 1,
        z: 0,
        unit: "column".into(),
    })
    .unwrap();
    assert!(combos(&g, 5, 1).is_empty(), "one column is not a colonnade");
    g.apply(Command::Build {
        x: 5,
        y: 2,
        z: 0,
        unit: "column".into(),
    })
    .unwrap();
    assert_eq!(combos(&g, 5, 1), vec!["colonnade".to_string()]);
    // The Society needs its charter from the ledger before it forms.
    let cat = g.rs.catalog();
    let society = cat["combos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "society")
        .unwrap();
    assert_eq!(society["near"].as_array().unwrap().len(), 2);
}

#[test]
fn guests_drink_eat_and_pay() {
    let mut g = Game::new(dungeon(), "taproom");
    for c in bot::plan().into_iter().take(14) {
        g.apply(c).unwrap();
    }
    let gold = global(&g, "Gold");
    g.apply(Command::Next).unwrap();
    let mut served = std::collections::BTreeMap::new();
    let mut arrived = 0;
    while g.snapshot().phase == "night" {
        for e in g.apply(Command::Tick { n: 10 }).unwrap() {
            match e {
                Event::Serve { service, .. } => *served.entry(service).or_insert(0) += 1,
                Event::Arrive { .. } => arrived += 1,
                _ => {}
            }
        }
    }
    assert!(arrived >= 5, "only {arrived} guests came");
    assert!(served.get("ale").copied().unwrap_or(0) >= 5, "{served:?}");
    assert!(served.get("stew").copied().unwrap_or(0) >= 2, "{served:?}");
    assert!(!served.contains_key("bunk"), "there are no bunks yet");
    // Ale and stew cost the inn gold to make, but sell for more.
    assert!(
        global(&g, "Gold") > gold,
        "{} -> {}",
        gold,
        global(&g, "Gold")
    );
    // Everyone has gone home at dawn, and the night flag is down.
    assert!(g.snapshot().guests.is_empty());
    assert_eq!(global(&g, "Night"), 0.0);
}

#[test]
fn a_season_ends_after_its_nights() {
    let mut g = Game::new(dungeon(), "season");
    let log = bot::season(&mut g);
    assert!(g.over);
    assert_eq!(g.nights_done, 7);
    assert!(matches!(log.last(), Some(Event::Over { .. })));
    assert!(g.apply(Command::Next).is_err());
    let s = g.snapshot();
    assert!(s.stats.served > 50, "{:?}", s.stats);
    assert!(s.stats.happy > s.stats.angry, "{:?}", s.stats);
}

#[test]
fn errors_name_the_file_and_line() {
    let base = || {
        "inn i\n grid 3 3\n entrance 1 0 hall\n money Gold\n reputation Rep\nend\n\
         room hall\nend\nneed Thirst\nend\nphase p\n night\n length 5\n open\nend\n\
         guest g\n need Thirst 1 0\nend\n"
            .to_string()
    };
    assert!(load(&[("a.inn".into(), base())]).is_ok());
    let e = load(&[("a.inn".into(), base() + "unit bar\n in taproom\nend\n")]).unwrap_err();
    assert_eq!((e.file.as_str(), e.line), ("a.inn", 20));
    assert!(e.message.contains("taproom"), "{e}");
    let e = load(&[("a.inn".into(), base() + "service s\n at bar\nend\n")]).unwrap_err();
    assert!(e.message.contains("no unit"), "{e}");
    let e = load(&[
        ("a.inn".into(), base() + "unit kegs\nend\n"),
        (
            "b.gbx".into(),
            "unitRule tap\n appliesTo kegs\n global Ale out 1\n local Ale in 1\nend\n".into(),
        ),
    ])
    .unwrap_err();
    assert_eq!((e.file.as_str(), e.line), ("b.gbx", 4));
}
