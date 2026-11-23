# The engine

OpenInn is a Rust crate (`crates/openinn`) with no idea what an inn sells. It reads a ruleset,
runs the game one command at a time and describes the result as plain data, so the same engine
drives the browser game, the tests and the bot that plays whole seasons.

## From Rust

```rust
let rs = openinn::load_dir(Path::new("games/dungeon-inn"))?;   // or load(&[(name, text), ...])
let mut game = openinn::Game::new(rs, "mimic-4242");          // the seed is any string
let events = game.apply(Command::Dig { x: 6, y: 1, room: "taproom".into() })?;
let view = game.snapshot();                                    // everything a client draws
```

`Game::replay(rs, seed, &commands)` rebuilds a game from a save. A command that is not allowed
(digging at night, building without the money) returns `Err` with a sentence and changes nothing.

## Commands

Commands are JSON objects tagged by `do`:

| Command | When | What |
| --- | --- | --- |
| `{"do": "dig", "x": 6, "y": 1, "room": "taproom"}` | building phase | Digs a cell next to the inn as a room kind |
| `{"do": "build", "x": 6, "y": 1, "unit": "bar"}` | building phase | Puts a unit on a dug cell |
| `{"do": "remove", "x": 6, "y": 1}` | building phase | Takes the unit away; half its cost comes back |
| `{"do": "next"}` | phase without a length | Ends the phase (opens the doors) |
| `{"do": "tick", "n": 1}` | timed phase | Runs the phase on for `n` ticks |
| `{"do": "eject", "guest": 12}` | open phase | Shows a guest the door, without a review |

## Events

`apply` answers with what happened, tagged by `type`:

* `phase` `{phase, day}`: a phase started.
* `arrive` `{guest, kind}`: a guest came in.
* `serve` `{guest, service, x, y, price}`: a guest was served at a unit and paid.
* `leave` `{guest, kind, mood, why}`: a guest left; `why` is a phrase for the log ("time to
  go", "closing time", "no rest to be had", "shown the door").
* `over` `{money, reputation, served}`: the season is over.

## Snapshot

`snapshot()` is everything a client needs to draw the inn:

* `day`, `phase`, `phase_tick`, `phase_length` (0: waits for the player), `build`, `over`;
* `globals`: `[name, value]` pairs (money, reputation, stores, flags);
* `layers`: the layer names; every cell carries their values in this order;
* `cells`: `{x, y, room, maps}` for every dug cell;
* `units`: `{x, y, unit, model, seated}`;
* `guests`: `{id, kind, x, y, mood, purse, needs, busy}` (`busy` names the service being given);
* `stats`: `{arrived, served, happy, angry, earned}` for the season.

The ruleset's own `catalog()` lists the inn, phases, layers, rooms, units (with the services
they offer), needs, services and guests, with their names, costs and model hints, and the
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
| `{"op": "start", "files": [[name, text], ...], "seed": "...", "commands": [...]}` | `{catalog, view, events}` (`commands` is optional: a save to replay) |
| `{"op": "apply", "command": {...}}` | `{view, events}`, or `{error}` and nothing changes |
| `{"op": "view"}` | `{view}` |
| `{"op": "save"}` | `{seed, commands}` |

`web/js/engine.js` wraps these in a small class (`Engine.load`, `start`, `apply`, `save`). The
game page loads a ruleset from `games/<id>/` (listed in its `files.json`), picked with
`?game=<id>`, and in the console `openinn.send({ do: 'dig', x: 6, y: 1, room: 'hall' })` sends a
command.

## Determinism and saves

No hash-ordered iteration, no transcendental functions, and a seeded xorshift for arrivals,
purses and stays. The same ruleset, seed and commands give the same inn on every machine, so a
save is `{seed, commands}`: the game keeps it in `localStorage`, and a link with `?seed=` starts
someone else on the same night of guests.

## Building and testing

```sh
cargo test                                            # rules, determinism, a whole season
cargo run --example season -- games/dungeon-inn 1 2   # a bot plays seasons and reports each night
sh tools/site.sh && python3 -m http.server -d _site   # this site and the game, on port 8000
```

Every push to `main` runs clippy, the tests and a bot season of every ruleset, then publishes
the site.
