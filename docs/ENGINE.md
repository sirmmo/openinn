# The engine

OpenInn is a Rust crate (`crates/openinn`) with no idea what an inn sells. It reads a ruleset,
runs the game one command at a time and describes the result as plain data, so the same engine
drives the browser game, the tests and the bot that plays whole seasons.

## From Rust

```rust
let rs = openinn::load_dir(Path::new("games/dungeon-inn"))?;   // or load(&[(name, text), ...])
let mut game = openinn::Game::new(rs, "mimic-4242");          // a season; the seed is any string
// or Game::with_mode(rs, seed, Mode::Endless): no last night, the ruleset's endless grid
let events = game.apply(Command::Dig { x: 6, y: 1, room: "taproom".into() })?;
let view = game.snapshot();                                    // everything a client draws
```

`Game::replay_mode(rs, seed, mode, &commands)` rebuilds a game from a save. A command that is not allowed
(digging at night, building without the money) returns `Err` with a sentence and changes nothing.

## Commands

Commands are JSON objects tagged by `do`:

| Command | When | What |
| --- | --- | --- |
| `{"do": "dig", "x": 6, "y": 1, "z": 0, "room": "taproom"}` | building phase | Digs a cell next to the inn (or where stairs lead) as a room kind |
| `{"do": "build", "x": 6, "y": 1, "z": 0, "unit": "bar"}` | building phase | Puts a unit on a dug cell |
| `{"do": "remove", "x": 6, "y": 1, "z": 0}` | building phase | Takes the unit away; half its cost comes back |
| `{"do": "wall", "x": 5, "y": 1, "z": 0, "side": "e", "door": false}` | building phase | Puts a wall (or a door) between two dug cells |
| `{"do": "unwall", "x": 5, "y": 1, "z": 0, "side": "e"}` | building phase | Takes it down; half its cost comes back |
| `{"do": "turn", "x": 6, "y": 1, "z": 0}` | building phase | Turns the unit a quarter round, clockwise (free; only its looks) |
| `{"do": "paint", "x": 6, "y": 1, "z": 0, "finish": "boards"}` | building phase | Lays one of the room's finishes on the cell (`""`: the plain floor back) |
| `{"do": "rename", "name": "The Rusty Lantern"}` | any time | The inn's own name (`""`: the ruleset's) |
| `{"do": "plan", "cells": [...], "walls": [...], "stairs": [...]}` | first command only | Builds a whole building at no cost (an import) |
| `{"do": "idle", "cmd": {"cmd": "buy", "id": "Peddler"}}` | any time | A command for the incremental layer: OpenIdle's `buy`, `act`, `prestige` |
| `{"do": "wait", "ms": 1000}` | any time | Time passing outside the night: the incremental layer runs on; waits in a row are saved as one |
| `{"do": "next"}` | phase without a length | Ends the phase (opens the doors) |
| `{"do": "tick", "n": 1}` | timed phase | Runs the phase on for `n` ticks |
| `{"do": "eject", "guest": 12}` | open phase | Shows a guest the door, without a review |

## Events

`apply` answers with what happened, tagged by `type`:

* `phase` `{phase, day}`: a phase started.
* `arrive` `{guest, kind}`: a guest came in.
* `serve` `{guest, service, x, y, z, price}`: a guest was served at a unit and paid (the
  price there: the service's, times the incremental layer's and the unit's combos').
* `leave` `{guest, kind, mood, why}`: a guest left; `why` is a phrase for the log ("time to
  go", "closing time", "no rest to be had", "shown the door").
* `over` `{money, reputation, served}`: the season is over.
* `idle` `{event}`: something happened in the incremental layer (an OpenIdle event: `kind` is
  `unlock`, `milestone`, `upgrade` or `prestige`, with `id` and `label`).
* `restart` `{run}`: the inn was sold; the building starts over.

## Snapshot

`snapshot()` is everything a client needs to draw the inn:

* `name` (the player's or the ruleset's), `day`, `phase`, `phase_tick`, `phase_length` (0:
  waits for the player), `build`, `over`;
* `globals`: `[name, value]` pairs (money, reputation, stores, flags);
* `layers`: the layer names; every cell carries their values in this order;
* `cells`: `{x, y, z, room, finish, maps}` for every dug cell (`finish`: the floor laid, or null);
* `walls`: `{x, y, z, side, door}`, each on the `e` or `s` side of its cell;
* `units`: `{x, y, z, unit, model, seated, rot, combos}` (`rot`: quarter turns clockwise;
  `combos`: those that hold for it now);
* `guests`: `{id, kind, x, y, z, mood, purse, needs, busy}` (`busy` names the service being given);
* `mode` (`season` or `endless`) and `grid`: `[width, height, levels]`;
* `idle`: the incremental layer as OpenIdle's view for the inn's player (resources, generators,
  upgrades, actions, milestones, prestiges), with its `offline_max`; `prices`: each service's
  price now (before combos); `locked`: units, rooms and combos not unlocked yet; `run`: how many times the inn was sold;
* `stats`: `{arrived, served, happy, angry, earned}` for the season.

The ruleset's own `catalog()` lists the inn, phases, layers, rooms (with their finishes), units (with the
services they offer), needs, services, guests and combos, with their names, texts, costs and
model hints, and the
`look` block's lines, so a client can build its menus and draw the inn without knowing the
ruleset.

## In the browser

`crates/openinn-web` compiles the engine to WebAssembly with three exported functions and no
generated glue. The page sends a JSON request and reads a JSON answer:

```js
const bytes = new TextEncoder().encode(JSON.stringify(request));
const ptr = wasm.inn_alloc(bytes.length);
new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
const out = wasm.inn_call(ptr, bytes.length);      // frees the request
const answer = JSON.parse(new TextDecoder().decode(
    new Uint8Array(wasm.memory.buffer, out, wasm.inn_out_len())));
```

| Request | Answer |
| --- | --- |
| `{"op": "start", "files": [[name, text], ...], "seed": "...", "mode": "season", "commands": [...]}` | `{catalog, view, events}` (`mode` and `commands` are optional: `commands` is a save to replay) |
| `{"op": "apply", "command": {...}}` | `{view, events}`, or `{error}` and nothing changes |
| `{"op": "view"}` | `{view}` |
| `{"op": "save"}` | `{seed, mode, commands}` |
| `{"op": "export"}` | `{dwellings}`: the building in the Dwellings JSON format |
| `{"op": "import", "dwellings": {...}, "mode": "endless"}` | `{plan, warnings}`: start a game with `commands: [plan]` |

`web/js/engine.js` wraps these in a small class (`Engine.load`, `start`, `apply`, `save`). The
game page loads a ruleset from `games/<id>/` (listed in its `files.json`), picked with
`?game=<id>`, and in the console `openinn.send({ do: 'dig', x: 6, y: 1, room: 'hall' })` sends a
command.

## Dwellings

`openinn::dwellings::export(&game)` writes the building in the JSON format of watabou's
[Dwellings](https://watabou.github.io/dwellings/) (read from the app's own exporter):

```text
{ "floors": [ { "level": 0,
                "rooms":   [ { "name": "Kitchen", "cells": [ {"i": 2, "j": 5} ] } ],
                "doors":   [ { "edge": { "cell": {"i": 2, "j": 5}, "dir": "n" }, "type": "REGULAR" } ],
                "windows": [],
                "stairs":  [ { "cell": {"i": 3, "j": 6}, "dir": "n", "up": true } ] } ],
  "exit": { "cell": {"i": 0, "j": 6}, "dir": "n" } }
```

`i` is the row (y), `j` the column (x); an edge is the side `dir` of `cell`. Each connected area
of one room kind, not divided by walls, is a room; doors are `REGULAR`, and where two rooms meet
with no wall between them one edge is written as a `DOORWAY`. Levels are `z` in an inn that
grows up and `-z` in one that goes down. `dwellings::import(&ruleset, mode, &doc)` reads one
back as a `plan` command (see [Rulesets](RULES.md#dwellings) for how). The Dwellings app has no
import of its own, so this goes one way there: from Dwellings into OpenInn, and from OpenInn to
any tool that reads the format.

## Determinism and saves

No hash-ordered iteration, no transcendental functions, and a seeded xorshift for arrivals,
purses and stays. The same ruleset, seed, mode and commands give the same inn on every machine,
so a save is `{seed, mode, commands}`: the game keeps it in `localStorage`, and a link with `?seed=` starts
someone else on the same night of guests.

## Building and testing

```sh
cargo test                                            # rules, determinism, a whole season
cargo run --example season -- games/dungeon-inn 1 2   # a bot plays seasons and reports each night
sh tools/site.sh && python3 -m http.server -d _site   # this site and the game, on port 8000
```

Every push to `main` runs clippy, the tests and a bot season of every ruleset, then publishes
the site.
