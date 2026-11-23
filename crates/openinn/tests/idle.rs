//! The incremental layer: The Gilded Mimic's ledger, an OpenIdle game that shares the inn's gold,
//! is fed by its guests and acts on its prices, arrivals, patience and furniture.
mod bot;

use openinn::{load_dir, Command, Event, Game, Mode};
use serde_json::json;

fn game(seed: &str) -> Game {
    Game::new(load_dir(&bot::dungeon_dir()).unwrap(), seed)
}

fn idle(cmd: serde_json::Value) -> Command {
    Command::Idle {
        cmd: serde_json::from_value(cmd).unwrap(),
    }
}

fn buy(id: &str) -> Command {
    idle(json!({ "cmd": "buy", "id": id }))
}

/// The ledger's amount of a resource, from the view.
fn ledger(g: &Game, res: &str) -> f64 {
    let v = g.snapshot().idle.unwrap();
    let r = v["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == res)
        .unwrap();
    r["amount"]
        .as_str()
        .map_or_else(|| r["amount"].as_f64().unwrap(), |s| s.parse().unwrap())
}

#[test]
fn the_ledger_shares_the_gold_and_earns_while_you_wait() {
    let mut g = game("purse");
    assert_eq!(g.money(), 320.0);
    assert_eq!(ledger(&g, "Gold"), 320.0);
    // A peddler, bought in the ledger, is paid from the inn's purse.
    g.apply(buy("Peddler")).unwrap();
    assert_eq!(g.money(), 295.0);
    // Digging in the inn spends the ledger's gold too.
    g.apply(Command::Dig {
        x: 6,
        y: 1,
        z: 0,
        room: "hall".into(),
    })
    .unwrap();
    assert_eq!(g.money(), 289.0);
    // A minute away: the peddler sells 0.2 gold a second. Waits in a row are one command.
    g.apply(Command::Wait { ms: 30_000 }).unwrap();
    g.apply(Command::Wait { ms: 30_000 }).unwrap();
    assert!((g.money() - (289.0 + 12.0)).abs() < 1e-9, "{}", g.money());
    assert_eq!(g.commands.last(), Some(&Command::Wait { ms: 60_000 }));
}

#[test]
fn guests_feed_the_ledger_and_upgrades_change_the_inn() {
    let mut g = game("feed");
    // Feather beds are locked until the ledger buys feather mattresses.
    g.apply(Command::Dig {
        x: 6,
        y: 1,
        z: 0,
        room: "dorm".into(),
    })
    .unwrap();
    let e = g
        .apply(Command::Build {
            x: 6,
            y: 1,
            z: 0,
            unit: "bed".into(),
        })
        .unwrap_err();
    assert!(e.contains("not unlocked"), "{e}");
    assert!(g.snapshot().locked.contains(&"bed".to_string()));

    // The first day's building only, so the takings pile up.
    let mut g = game("feed");
    let first_day: Vec<_> = bot::plan().into_iter().take(14).collect();
    bot::season_with_nights(&mut g, &first_day, 3);
    assert!(g.stats.served >= 15);
    assert_eq!(ledger(&g, "Served"), f64::from(g.stats.served));
    assert!(ledger(&g, "Renown") > 0.0);
    // Dwarven stout: ale for half as much again.
    let ale = 0;
    assert_eq!(g.rs.services[ale].id, "ale");
    assert_eq!(g.price(ale), 3.0);
    g.apply(buy("Stout")).unwrap();
    assert_eq!(g.price(ale), 4.5);
    assert_eq!(g.snapshot().prices[ale], 4.5);
    // The ledger's events come out as the inn's.
    let events = g.apply(buy("Featherbeds")).unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Idle { event } if event["kind"] == "upgrade")));
    assert!(!g.snapshot().locked.contains(&"bed".to_string()));
}

#[test]
fn the_same_commands_give_the_same_inn_and_ledger() {
    let mut a = game("same");
    bot::season_with_nights(&mut a, &bot::plan(), 2);
    a.apply(buy("Peddler")).unwrap();
    a.apply(idle(json!({ "cmd": "act", "id": "bell" })))
        .unwrap();
    a.apply(Command::Wait { ms: 12_345 }).unwrap();
    bot::season_with_nights(&mut a, &bot::plan(), 3);
    let b = Game::replay_mode(a.rs.clone(), "same", Mode::Season, &a.commands).unwrap();
    assert_eq!(a.money(), b.money());
    assert_eq!(a.stats.served, b.stats.served);
    assert_eq!(a.snapshot().idle, b.snapshot().idle);
}

#[test]
fn selling_the_inn_starts_the_building_over_and_keeps_the_legend() {
    let mut g = game("sell");
    bot::season_with_nights(&mut g, &bot::plan(), 7);
    assert!(g.over);
    // The season is over, but the ledger still works: sell the inn.
    let renown = ledger(&g, "Renown");
    assert!(renown >= 5.0, "{renown}");
    let events = g
        .apply(idle(json!({ "cmd": "prestige", "id": "sell" })))
        .unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Restart { run: 1 })));
    assert!(!g.over);
    assert_eq!(g.runs, 1);
    let s = g.snapshot();
    assert_eq!(s.cells.len(), 1, "only the entrance is left");
    assert_eq!((s.day, s.phase.as_str()), (1, "day"));
    assert_eq!(g.money(), 320.0);
    let legend = ledger(&g, "Legend");
    assert_eq!(legend, (renown / 5.0).sqrt().floor());
    // Every price is 5% higher per legend.
    assert_eq!(
        g.price(0),
        ((3.0 * (1.0 + 0.05 * legend)) * 100.0_f64).round() / 100.0
    );
    // And the whole thing replays.
    let b = Game::replay_mode(g.rs.clone(), "sell", Mode::Season, &g.commands).unwrap();
    assert_eq!(b.runs, 1);
    assert_eq!(b.snapshot().idle, g.snapshot().idle);
}
