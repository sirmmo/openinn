//! Plays a season of a ruleset with the test bot and reports each night:
//!     cargo run --example season -- games/dungeon-inn [seed...]
#[path = "../tests/bot.rs"]
mod bot;

use openinn::{load_dir, Event, Game};
use std::collections::BTreeMap;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args
        .first()
        .cloned()
        .unwrap_or_else(|| "games/dungeon-inn".into());
    let seeds: Vec<String> = if args.len() > 1 {
        args[1..].to_vec()
    } else {
        vec!["1".into(), "2".into(), "3".into()]
    };
    let rs = load_dir(std::path::Path::new(&dir)).unwrap_or_else(|e| panic!("{e}"));
    let plan = bot::plan_for(std::path::Path::new(&dir));
    for seed in seeds {
        let mut g = Game::new(rs.clone(), &seed);
        let log = bot::season_with(&mut g, &plan);
        let mut night = 0;
        let mut why: BTreeMap<String, u32> = BTreeMap::new();
        let (mut served, mut arrived, mut moods) = (
            BTreeMap::<String, u32>::new(),
            BTreeMap::<String, u32>::new(),
            Vec::new(),
        );
        println!("seed {seed}");
        for e in &log {
            match e {
                Event::Phase { phase, .. } if !is_night(&g, phase) && night > 0 => {
                    report(night, &arrived, &served, &why, &moods);
                    (why, served, arrived, moods) = (
                        BTreeMap::new(),
                        BTreeMap::new(),
                        BTreeMap::new(),
                        Vec::new(),
                    );
                }
                Event::Phase { phase, .. } if is_night(&g, phase) => night += 1,
                Event::Arrive { kind, .. } => *arrived.entry(kind.clone()).or_default() += 1,
                Event::Serve { service, .. } => *served.entry(service.clone()).or_default() += 1,
                Event::Leave {
                    why: w, mood, kind, ..
                } => {
                    *why.entry(format!("{kind}: {w}")).or_default() += 1;
                    moods.push(*mood);
                }
                Event::Over {
                    money, reputation, ..
                } => {
                    report(night, &arrived, &served, &why, &moods);
                    println!(
                        "  season over: gold {money:.0}, reputation {reputation:.1}, {:?}\n",
                        g.stats
                    );
                }
                _ => {}
            }
        }
    }
}

fn is_night(g: &Game, phase: &str) -> bool {
    g.rs.phases.iter().any(|p| p.id == phase && p.night)
}

fn report(
    n: u32,
    arrived: &BTreeMap<String, u32>,
    served: &BTreeMap<String, u32>,
    why: &BTreeMap<String, u32>,
    moods: &[f64],
) {
    let avg = if moods.is_empty() {
        0.0
    } else {
        moods.iter().sum::<f64>() / moods.len() as f64
    };
    println!(
        "  night {n}: arrived {arrived:?}\n    served {served:?}\n    left (mood {avg:.2}) {why:?}"
    );
}
