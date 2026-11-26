# OpenInn

An inn-keeping engine whose rules are GlassBox scripts. By day you dig rooms and furnish them;
by night the doors open and guests come in with needs, a purse and limited patience. They walk
to whatever serves their most pressing need, pay, make noise and mess, and leave a review that
decides who comes next.

The engine knows nothing about ale or dungeons. A **ruleset** (a folder of `*.inn` and `*.gbx`
files) declares the inn, its phases, rooms, furniture, needs, services and guests; the economy
between them is plain [GlassBox](https://crates.io/crates/glassbox), the same rule language as
[Lilienstern](https://github.com/sirmmo/lilienstern) and OpenFactor's packs.

The first ruleset is **The Gilded Mimic** (`games/dungeon-inn`): an inn on the third level of
the dungeon, for adventurers, dwarf miners, wizards and goblin peddlers on their way down.
The second is **The Slow Orbit** (`games/slow-orbit`): a bar and bunkhouse on a refuelling ring
at the edge of a mining belt, where the air goes stale (a CO2 layer that scrubbers clear) and
courier robots need a charging post. The third is **The Wheatsheaf** (`games/village-inn`): a
village inn on the green at a crossroads of the realm, for the taverns of
[OpenDorf](https://sirmmo.github.io/opendorf/), where farmhands, peddlers, merchants, soldiers,
quarrymen, bards, pilgrims, scholars and royal messengers stop; its stables earn well but make a
muck layer the other guests dislike, and its yard has a well, a kitchen garden and, later, a
maypole. Each ruleset also says how it looks (a `look` block: models, colours, words) and can bring
its own help text and stylesheet.

All three play as a seven-night **season** or in **endless** mode, where the inn keeps opening night
after night on a larger grid. Inns have **levels** joined by stairs, and **inner walls and doors**
that guests walk round and that stop noise, heat or stale air from spreading. Buildings import
from and export to the JSON format of watabou's [Dwellings](https://watabou.github.io/dwellings/).

Each inn also has an **incremental layer**, an [OpenIdle](https://github.com/sirmmo/openidle) game
that the inn plays as its one player: The Gilded Mimic's ledger (peddlers, bards, a loot fence,
upgrades such as dwarven stout or feather mattresses, a stair bell, and selling the inn for
legend), The Slow Orbit's contracts and The Wheatsheaf's market stall (stable lads, fiddlers,
field hands, a cook's helper, upgrades such as a malting floor or feather beds, and passing the inn
on for legacy). It shares the inn's purse, is fed by its guests, acts on
prices, arrivals, patience and what can be built, and runs on through the day and while you are
away. Its files sit next to the ruleset's: `idle.toml`, `params.toml`, `*.idle`.

**Play:** [The Gilded Mimic](https://ingmmo.com/openinn/play/) ·
[The Slow Orbit](https://ingmmo.com/openinn/play/?game=slow-orbit) ·
[The Wheatsheaf](https://ingmmo.com/openinn/play/?game=village-inn) ·
**Documentation:** https://ingmmo.com/openinn/

## How a ruleset works

* Every dug **cell is a GlassBox place**; neighbouring cells are joined. Layers (maps) such as
  Noise, Heat and Dirt spread between cells and fade, so a rowdy taproom keeps the dormitory
  next door awake.
* Furniture and stations are **GlassBox units** on cells, running ordinary `unitRule`s: the
  keg rack taps barrels, the hearth cooks stew and warms the rooms around it, the larder buys
  provisions, the washtub's scullion scrubs the floors.
* **Phases** set global resources when they start (`set Night 1`), so rules that should run
  only at night simply test `global Night atLeast 1`.
* **Guests** are what GlassBox has no words for: persistent people with needs that grow every
  tick, dislikes (tests on the layers where they stand), a purse, patience and a mood. A guest
  with an urgent need walks (breadth first) to the nearest unit offering a **service** for it
  with a free seat and the stock it takes, pays, and is served for a while. Guests who wait too
  long leave angry; everyone leaves a review that moves the inn's reputation, which brings more
  guests and unlocks pickier ones (`if global Reputation atLeast 25`).

The language is described in [docs/RULES.md](docs/RULES.md), and the engine (commands, events,
snapshots, the WebAssembly messages) in [docs/ENGINE.md](docs/ENGINE.md).

## Determinism and saves

No hash-ordered iteration, no transcendental functions, a seeded xorshift for arrivals, purses
and stays. The same ruleset, seed and commands give the same inn on every machine, so a save is
`{ seed, commands }` and `Game::replay` rebuilds it.

## Development

```sh
cargo test                                            # rules, determinism, a whole season
cargo run --example season -- games/dungeon-inn 1 2   # a bot plays seasons and reports each night
sh tools/site.sh && python3 -m http.server -d _site   # the docs on port 8000, the game in play/
```

The browser game is the engine compiled to WebAssembly (`crates/openinn-web`, JSON messages, no
generated glue) and a plain-JS page (`web/`, three.js, no build step). It loads a ruleset's files
from `games/<id>/` (`?game=<id>`, default `dungeon-inn`) and saves the game in localStorage as its
seed and commands. In the console, `openinn.send({ do: 'dig', x: 6, y: 1, room: 'hall' })`
sends a command.

| Path | What |
| --- | --- |
| `crates/openinn/src/script.rs` | OpenInn's blocks, compiled next to GlassBox's |
| `crates/openinn/src/sim.rs` | The game: cells and levels, walls, units, guests, phases, commands, snapshots |
| `crates/openinn/src/dwellings.rs` | Export to and import from watabou's Dwellings JSON |
| `crates/openinn/tests/` | Tests and the bot innkeeper they share with the example; `dwellings/` holds two houses exported from the Dwellings app |
| `games/dungeon-inn/` | The Gilded Mimic: `inn.inn` (declarations, look, the `idle` bridge), `economy.gbx` (GlassBox rules), `idle.toml` + `params.toml` + `ledger.idle` (the ledger, in OpenIdle), `help.html`, `bot.plan` (the test bot's build order) |
| `games/slow-orbit/` | The Slow Orbit, the same way, plus `style.css` |
| `games/village-inn/` | The Wheatsheaf, the same way: `market.idle` is its market stall |
| `crates/openinn-web/` | The engine as WebAssembly: `start`, `apply`, `view`, `save` messages |
| `web/` | The page: `js/engine.js` (wasm), `js/view.js` (three.js), `js/main.js` (interface) |
| `site/`, `docs/` | The documentation site: the overview page, and the pages rendered from `docs/*.md` |
| `tools/site.sh` | Builds `_site/` for GitHub Pages: the documentation at the root, the game in `play/` |

Still to come: staff to hire for the night, and more rulesets.

## Credits

- 3D models: [KayKit Dungeon Pack](https://kaylousberg.com) by Kay Lousberg;
  [Space Station Kit and Mini Characters](https://kenney.nl) by Kenney; the Ultimate Modular
  Ruins Pack by [Quaternius](https://quaternius.com) (`web/models/ruins/`, converted to glTF);
  for The Wheatsheaf (`web/models/village/`), pieces of Kenney's Fantasy Town, Nature, Graveyard,
  Survival and Prototype kits and KayKit's Medieval Hexagon Pack, made into self-contained GLB
  files; all CC0 (licences in `web/models/*/`).
- [three.js](https://threejs.org) r160, MIT (`web/vendor/three-LICENSE.txt`).
- Rules in [GlassBox](https://crates.io/crates/glassbox); the incremental layer runs on
  [OpenIdle](https://github.com/sirmmo/openidle).
- Fonts: Cinzel and Fredoka from Google Fonts (OFL).

## Licence

Code: GPL-3.0-or-later, see `LICENSE`. Models: CC0.

## Publishing on Wavedash

Each ruleset is its own game on [Wavedash](https://wavedash.com)
([docs](https://docs.wavedash.com)), configured in `wavedash/<game>/wavedash.toml`: `dungeon-inn`
(The Gilded Mimic), `slow-orbit` (The Slow Orbit) and `village-inn` (The Wheatsheaf).

1. **Create the game** in the Wavedash Developer Portal, or with the CLI (`curl -fsSL
   https://wavedash.com/cli/install.sh | sh`, `wavedash auth login`, then `wavedash project create
   --title "The Slow Orbit" --team-id <TEAM_ID>`).
2. **Set its id**: `game_id` in its `wavedash.toml`, or, to keep it out of the repository,
   `WAVEDASH_GAME_ID_<GAME>` in the environment and as a repository variable (Settings › Secrets and
   variables › Actions › Variables): `WAVEDASH_GAME_ID_DUNGEON_INN`, `WAVEDASH_GAME_ID_SLOW_ORBIT`.
3. **Upload a build**: `sh tools/wavedash.sh [game]` builds the site with `tools/site.sh`, copies
   the game into `wavedash/<game>/build/` with only that ruleset, which it opens without `?game=`
   (the documentation stays on the website), and, with the CLI on `PATH` and `WAVEDASH_TOKEN` set,
   runs `wavedash build push`. Without the CLI (or with `WAVEDASH_DRY_RUN=1`) it only says what it
   would push. On GitHub: Actions › Wavedash › Run workflow, with the secret `WAVEDASH_TOKEN`.
4. **Publish it**: `wavedash publish <BUILD_ID>` (the id `build push` printed), or in the Developer
   Portal.

On Wavedash the game stays behind Wavedash's loading screen until it calls `Wavedash.init()`:
`web/js/main.js` does it through `web/js/wavedash.js` once the inn is drawn (the loading bar also
feeds Wavedash's), or when the ruleset cannot start, to show why. Elsewhere (GitHub Pages, a local
server) there is no `Wavedash` object and the calls do nothing.
