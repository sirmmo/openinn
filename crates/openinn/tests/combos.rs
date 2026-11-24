//! Combos (furniture that works better together), likes, turning units, floor finishes and the
//! inn's own name, on a small ruleset of their own.
use openinn::{load, Command, Event, Game};

const INN: &str = r#"
inn t
    name Test Inn
    grid 8 4
    entrance 3 0 hall
    money Gold
    reputation Rep 2
    start Gold 1000
    nights 2
    guests 6
end
phase day
    build
end
phase night
    length 120
    open
    night
    arrive 0.2
end
layer Comfort
    decay 0.2
    diffuse 0.1
    max 20
end
room hall
    cost 1
    text Where guests come in.
    finish flags floor_tile_large 3 Flagstones
    finish boards floor_wood_large 5
end
need Thirst
    urgent 30
end
unit bar
    in hall
    cost 10
    seats 2
    text Serves drinks.
end
unit lamp
    in hall
    cost 5
    seats 0
end
service drink
    at bar
    need Thirst 50
    price 4
    time 10
end
combo lit
    name A lit bar
    text A bar by a lamp sells dearer, faster.
    at bar
    near lamp 1
    in hall
    price 1.5
    time 0.5
    mood 0.1
    map Comfort out 2
end
guest drinker
    purse 100 100
    stay 100 100
    patience 50
    need Thirst 1 40
    like map Comfort greater 0.5 0.01
end
"#;

fn game() -> Game {
    Game::new(load(&[("t.inn".into(), INN.into())]).unwrap(), "c")
}

fn dig(g: &mut Game, x: usize, y: usize) {
    g.apply(Command::Dig {
        x,
        y,
        z: 0,
        room: "hall".into(),
    })
    .unwrap();
}

fn build(g: &mut Game, x: usize, y: usize, unit: &str) {
    g.apply(Command::Build {
        x,
        y,
        z: 0,
        unit: unit.into(),
    })
    .unwrap();
}

fn night_prices(g: &mut Game) -> Vec<f64> {
    g.apply(Command::Next).unwrap();
    let mut prices = Vec::new();
    while !g.snapshot().build && !g.over {
        for e in g.apply(Command::Tick { n: 1 }).unwrap() {
            if let Event::Serve { price, .. } = e {
                prices.push(price);
            }
        }
    }
    prices
}

#[test]
fn a_combo_holds_while_its_furniture_is_near() {
    let mut g = game();
    dig(&mut g, 3, 1);
    dig(&mut g, 4, 1);
    dig(&mut g, 5, 1);
    build(&mut g, 3, 1, "bar");
    let s = g.snapshot();
    assert!(s.units[0].combos.is_empty());
    // A lamp two cells away is too far; one beside the bar makes the combo.
    build(&mut g, 5, 1, "lamp");
    assert!(g.snapshot().units[0].combos.is_empty());
    build(&mut g, 4, 1, "lamp");
    assert_eq!(g.snapshot().units[0].combos, vec!["lit".to_string()]);
    let prices = night_prices(&mut g);
    assert!(!prices.is_empty());
    assert!(prices.iter().all(|&p| p == 6.0), "{prices:?}");
    // The combo's comfort reached the hall.
    assert!(g.snapshot().cells.iter().any(|c| c.maps[0] > 0.0));
    // Take the lamp away (in the day): plain prices again.
    g.apply(Command::Remove { x: 4, y: 1, z: 0 }).unwrap();
    assert!(g.snapshot().units[0].combos.is_empty());
    let prices = night_prices(&mut g);
    assert!(prices.iter().all(|&p| p == 4.0), "{prices:?}");
    let cat = g.rs.catalog();
    assert_eq!(cat["combos"][0]["near"][0]["units"][0], "lamp");
    assert_eq!(cat["units"][0]["text"], "Serves drinks.");
}

#[test]
fn turning_finishes_and_a_name() {
    let mut g = game();
    dig(&mut g, 3, 1);
    build(&mut g, 3, 1, "bar");
    for _ in 0..3 {
        g.apply(Command::Turn { x: 3, y: 1, z: 0 }).unwrap();
    }
    assert_eq!(g.snapshot().units[0].rot, 3);
    let before = g.money();
    g.apply(Command::Paint {
        x: 3,
        y: 1,
        z: 0,
        finish: "boards".into(),
    })
    .unwrap();
    assert_eq!(g.money(), before - 5.0);
    let s = g.snapshot();
    let cell = s.cells.iter().find(|c| (c.x, c.y) == (3, 1)).unwrap();
    assert_eq!(cell.finish.as_deref(), Some("boards"));
    assert!(g
        .apply(Command::Paint {
            x: 3,
            y: 1,
            z: 0,
            finish: "marble".into()
        })
        .is_err());
    g.apply(Command::Rename {
        name: "The Lamp and Bar".into(),
    })
    .unwrap();
    assert_eq!(g.snapshot().name, "The Lamp and Bar");
    // All of it replays.
    let b = Game::replay(g.rs.clone(), "c", &g.commands).unwrap();
    assert_eq!(b.snapshot().units[0].rot, 3);
    assert_eq!(b.snapshot().name, "The Lamp and Bar");
    assert_eq!(b.money(), g.money());
}
