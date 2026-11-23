//! Every ruleset in games/ loads, and the bot plays a whole season of it with guests served.
mod bot;

use openinn::{load_dir, Game};
use std::path::Path;

#[test]
fn every_ruleset_plays_a_season() {
    let games = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games");
    let mut dirs: Vec<_> = std::fs::read_dir(&games)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    assert!(dirs.len() >= 2, "expected at least two rulesets in games/");
    for dir in dirs {
        let rs = load_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        assert!(!rs.look.is_empty(), "{} has no look block", dir.display());
        let mut g = Game::new(rs, "1");
        bot::season_with(&mut g, &bot::plan_for(&dir));
        assert!(g.over, "{}: the season did not end", dir.display());
        assert!(
            g.stats.served > 50,
            "{}: only {} served",
            dir.display(),
            g.stats.served
        );
        assert!(
            g.stats.happy > g.stats.angry,
            "{}: {:?}",
            dir.display(),
            g.stats
        );
    }
}
