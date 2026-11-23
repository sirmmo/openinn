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
courier robots need a charging post. Each ruleset also says how it looks (a `look` block: models,
colours, words) and can bring its own help text and stylesheet.

Both play as a seven-night **season** or in **endless** mode, where the inn keeps opening night
after night on a larger grid. Inns have **levels** joined by stairs, and **inner walls and doors**
that guests walk round and that stop noise, heat or stale air from spreading. Buildings import
from and export to the JSON format of watabou's [Dwellings](https://watabou.github.io/dwellings/).

**Play:** [The Gilded Mimic](https://ingmmo.com/openinn/play/) ·
[The Slow Orbit](https://ingmmo.com/openinn/play/?game=slow-orbit) ·
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
| `games/dungeon-inn/` | The Gilded Mimic: `inn.inn` (declarations and look), `economy.gbx` (GlassBox rules), `help.html`, `bot.plan` (the test bot's build order) |
| `games/slow-orbit/` | The Slow Orbit, the same way, plus `style.css` |
| `crates/openinn-web/` | The engine as WebAssembly: `start`, `apply`, `view`, `save` messages |
| `web/` | The page: `js/engine.js` (wasm), `js/view.js` (three.js), `js/main.js` (interface) |
| `site/`, `docs/` | The documentation site: the overview page, and the pages rendered from `docs/*.md` |
| `tools/site.sh` | Builds `_site/` for GitHub Pages: the documentation at the root, the game in `play/` |

Still to come: staff to hire for the night, and more rulesets.

## Credits

- 3D models: [KayKit Dungeon Pack](https://kaylousberg.com) by Kay Lousberg;
  [Space Station Kit and Mini Characters](https://kenney.nl) by Kenney; all CC0 (licences in
  `web/models/*/`).
- [three.js](https://threejs.org) r160, MIT (`web/vendor/three-LICENSE.txt`).
- Rules in [GlassBox](https://crates.io/crates/glassbox).
- Fonts: Cinzel and Fredoka from Google Fonts (OFL).

## Licence

Code: GPL-3.0-or-later, see `LICENSE`. Models: CC0.
