# OpenInn rulesets

A ruleset is a folder of `*.inn` and `*.gbx` files, read in name order. Every file is a list of
blocks:

```text
kind name
    key values...
end
```

`#` and `//` start comments (so colours are written without `#`: `colour b5734a`). Blocks of
GlassBox kinds (`unitRule`, `mapRule`, `agentRule`, `rule`) are compiled by the
[glassbox](https://crates.io/crates/glassbox) crate; see Lilienstern's SCRIPTING.md for them.
OpenInn's own kinds are below. All OpenInn declarations are read before the GlassBox rules, so a
`unitRule` may name a unit declared in any file. Every error names its file and line.

Inside a block, avoid lines made of a block kind and one more word (`need Thirst`): the splitter
takes them for a block that is missing its `end`. Lines with more words are fine.

## inn

```text
inn dungeon
    name The Gilded Mimic
    grid 12 9                  # cells you can dig
    entrance 6 0 hall          # the first cell, dug as this room kind; guests come in here
    money Gold                 # global resource for prices and costs
    reputation Reputation 3    # global resource; a review moves it by up to ±3
    start Gold 320             # global resources at the start (any number of lines)
    nights 7                   # the season ends after this many night phases
    guests 14                  # at most this many guests at once
    levels 2 down              # levels to dig in a season, joined by stairs: `down` or `up`
    endless 18 14 5 30         # the endless mode's grid, levels and most guests at once
    wall 4                     # an inner wall between two dug cells costs this
    door 8                     # and a door
end
```

The **endless mode** has no last night: the inn opens night after night on the larger grid,
with more levels to dig. Each mode keeps its own save.

## phase

Phases run in the order they are declared, then the day starts again.

```text
phase night
    name Night: the doors are open
    length 180                 # ticks; without length the phase waits for the player (next)
    build                      # digging and building allowed
    open                       # guests arrive and are served
    night                      # counts towards the season's nights
    set Night 1                # global resources set when the phase starts
    arrive 0.06 per Reputation 0.003   # chance per tick of a guest, plus per unit of a global
end
```

When an open phase ends, every guest still inside leaves (and reviews the inn).

## layer

A GlassBox map over the dug cells.

```text
layer Noise
    decay 0.25                 # fraction lost per tick
    diffuse 0.15               # fraction spread to neighbouring cells per tick
    min 0
    max 30
    colour ff9f1c              # for the overlay in the game
end
```

## room

```text
room taproom
    name Taproom
    cost 10                    # money per cell dug
    colour b5734a              # for maps and the interface
    floor floor_wood_large     # a hint for the view: a model (see Model hints)
    dwellings living-room room *       # Dwellings rooms that import as this (see Dwellings)
    text Where the drinking is done.   # for the interface (several lines are joined)
    finish boards floor_wood_large_dark 3 Dark boards   # a floor the player can lay instead
end
```

`finish <id> <model> <cost> <name>` lines are floors a player can lay on a cell of the room
(the `paint` command), at that cost per cell: a choice of looks, with no effect on the rules.

## unit

A GlassBox unit kind: the engine adds one GlassBox unit per piece built.

```text
unit bar
    name Bar counter
    in taproom                 # room kinds it may stand in (none: anywhere)
    cost 50
    seats 3                    # guests it serves at once
    start Mugs 4               # local stock it is built with
    model bar_straight_A       # a hint for the view (see Model hints)
    text Three stools and a tap.       # for the interface
end
```

Players can turn a unit a quarter at a time (the `turn` command); it changes only how it looks.

A unit with the line `stairs` joins its cell to the same cell on the next level (below in an
inn whose levels go `down`, above in one that goes `up`). A cell on another level can be dug
next to a dug cell on its own level, or where stairs lead. Guests take the stairs, and so do
noise, heat and the rest: stairs are a link in the GlassBox graph like any other.

## Walls and doors

Dug cells next to each other are open to each other. A **wall** on the edge between them (the
`wall` command, at the inn's `wall` cost) cuts the link: guests walk round it, and layers no
longer spread across it, so a wall between the taproom and the dormitory keeps the noise out.
A **door** (`door` cost) lets guests and layers through, and marks a room's way in.

## need

```text
need Thirst
    name Thirst
    urgent 50                  # a guest goes looking at this value
    max 100
end
```

## service

What a unit gives a guest.

```text
service ale
    name A mug of ale
    at bar                     # unit kinds that offer it
    need Thirst 60             # takes this much off the need
    global Ale in 1            # what it uses up: the inn's stores (global) or the unit's (local)
    price 3                    # paid from the guest's purse into the inn's money
    time 4                     # ticks the guest stays at the unit
    map Noise out 3 radius 1   # emitted when it starts
end
```

## guest

```text
guest wizard
    name Wizard
    weight 2                   # how often, against the other guests who may come
    if global Reputation atLeast 25    # comes only when all `if` tests hold
    purse 50 110               # money, between these
    stay 80 140                # ticks, between these
    patience 20                # ticks they will wait for an urgent need before leaving angry
    need Rest 0.9 50           # need, growth per tick, starting value
    dislike map Noise greater 4 0.01   # each tick where this holds, mood drops by 0.01
    like map Comfort greater 2 0.004   # ... and here it rises by 0.004
    map Noise out 0.1          # emitted where they stand, every tick
    model character-wizard     # a hint for the view
end
```

Tests (in `if`, `like` and `dislike`) are `map <layer> <cmp> <value>` (at the guest's cell, or the
entrance for `if`) or `global <resource> <cmp> <value>`, with GlassBox's comparisons: `atLeast`,
`atMost`, `greater`, `less`, `is`.

A guest's mood starts at 0.6, rises by 0.1 with every service (and by the `mood` of the combos
that hold where they were served), drops by 0.01 for every tick spent waiting for something
nobody offers, and moves with their likes and dislikes. A guest leaving with mood `m` moves
reputation by `(m - 0.5) × 2 × review`.

## combo

Furniture that works better together. A unit of one of the `at` kinds has the combo while every
`near` holds (enough units of those kinds within the radius, on the same level, counting
diagonals), it stands in one of the `in` room kinds (if any are given) and every `if` test holds at
its cell.

```text
combo snug
    name Snug corner
    text A table by the hearth: stew sells for a fifth more, and guests warm to the place.
    at table                   # the units that get it
    near hearth 2              # a hearth within 2 cells
    near candles,banner 1 2    # and two candles or banners beside it (kinds, radius, how many)
    in taproom                 # standing in a taproom
    if map Noise less 6        # tests at the unit's cell, now
    price 1.2                  # its services cost this much more
    relief 1.25                # take this much more of the need away
    time 0.75                  # take this part of the time
    mood 0.05                  # the guests it serves are this much happier
    map Comfort out 0.3 radius 1   # emitted every tick while it holds
end
```

Combos on the same unit multiply. A guest pays the price at the unit they are served at, combos
and all; the client shows which combos hold, and where a piece about to be placed would make one.

## look

How a client draws the inn. The engine passes these lines to the client untouched (in the
catalog's `look`) and reads none of them, so a ruleset plays the same without one. One block per
ruleset; every key is optional. The browser game reads these:

```text
look station
    models station glb         # furniture, floors and walls: web/models/station/<name>.glb
    characters guests glb      # guests: web/models/guests/<name>.glb (animated: idle, walk, sit)
    scale 4                    # size of floors, walls and the entrance (a cell is 4 units)
    furniture 3                # size of the units' models
    floor-y -1.17              # raise or sink the floors
    wall wall                  # on the edges between dug and undug cells
    window wall-window 3       # instead of a wall on some back walls (about one in 3)
    decor torch_mounted 3 0.8 2.6 0.5   # on every 3rd north wall, at x, height, distance from it
    entrance wall-door-wide 1 0         # on the entrance's north edge: model, scale, degrees
    background 080b17          # colours, hex without `#`
    ground none                # under the grid: a colour, or none
    stars 900                  # a starfield underneath
    grid 26304f 1a2140         # the grid's lines
    edge 3d4a7a                # the grid's border
    light dfe8ff 1c2340        # sky and ground colours of the light
    loading Pressurising the airlock    # shown while the models load
    arrive comes aboard through the airlock   # "A Freighter pilot comes aboard through the airlock."
    seeds orbit comet nebula   # words for new seeds (orbit-4821)
    inner 0.55                 # inner walls and doors, at this part of a wall's height
    door wall_doorway          # the model for doors
    level 7                    # how far apart levels are drawn
    levelname Deck             # "Deck 2/5"
    wallname Bulkhead          # the names of the wall and door tools
    doorname Hatch
    help help.html             # the help window's text: an HTML file in the ruleset's folder
    style style.css            # a stylesheet for the page, in the ruleset's folder
end
```

The words for money and reputation in the interface are the names of their resources
(`money Credits`: "3 credits").

## idle: the incremental layer

An inn can have an incremental layer: an [OpenIdle](https://github.com/sirmmo/openidle) game in
the same folder, played by the inn as its one player. `idle.toml` is OpenIdle's `game.toml` (its
name, its scripts, the length of its tick and of the progress given for time away), next to
`params.toml` and `*.idle` scripts in OpenIdle's own language (resources, generators, rules,
upgrades, actions, milestones, prestiges; see OpenIdle's
[SCRIPTING.md](https://github.com/sirmmo/openidle/blob/main/docs/SCRIPTING.md)). The `idle`
block says how the two meet:

```text
idle ledger
    tick 0.25                  # seconds of the idle game per tick of the night
    share Gold                 # one resource for both: the inn's global and the idle game's
    feed Served served         # the idle resource Served gets 1 for every guest served
    feed Renown happy          # ... 1 for every guest who leaves happy
    feed Renown angry -0.5     # ... -0.5 for every angry one (also: arrived, earned, night)
    feed Faith service heal    # ... 1 for every guest served this service
    feed Renown combo snug 0.2 # ... 0.2 for every guest served where this combo holds
    price ale 1 + 0.5 * Stout  # the ale's price, times this (an OpenIdle expression)
    price * 1 + 0.05 * Legend  # every price, times this
    arrive (1 + 0.25 * Signboard) * (1 + bell)   # the chance of a guest, times this
    patience 1 + 0.5 * Benches # how long guests wait, times this
    unlock bed Featherbeds     # the unit (or room) can be built only while this holds
    unlock snug Bellows        # a combo works only while this holds
    boost snug 1 + Bellows     # the combo's effects, times this (2: twice as strong)
    set Fame Renown            # the inn's global Fame is this, after every step
    restart sell               # taking this prestige sells the inn: the building starts over
end
```

The idle game runs at night with the inn's ticks, and in real time the rest of the time: through
the day, a pause, a season that is over, and the time a player is away (up to its
`offline_max`). Expressions are OpenIdle's, so an upgrade is 1 once bought, an action 1 while it
lasts, a resource its amount. A `restart` prestige keeps the idle game (what its `keep` lists)
and starts the inn over: a new building, from the ruleset's `start`, with new dice.

## Model hints

`floor` in a room and `model` in a unit or guest name models for the client. A unit or room
may use several models, separated by spaces, each as `name:x:z:degrees:scale`, with x and z in
cells from the centre of the cell (towards the wall behind it is −z). A table with two chairs:

```text
    model table chair:0:-0.3:0 chair:0:0.3:180
```

Units turn their back to a wall when they have one.

## Dwellings

Buildings import from and export to the JSON format of watabou's
[Dwellings](https://watabou.github.io/dwellings/) generator (its "Export as JSON"). On import,
the house is turned so its exit faces north and moved so the exit is the inn's entrance. Each
room becomes the room kind whose `dwellings` line lists its name (lower case, `-` for spaces:
`living-room`), else the kind with that name, else the kind listing `*`. Rooms are walled from
each other; their doors become doors and their doorways stay open. The house's floors stack the
way the inn's levels go, and its stairs become the ruleset's first `stairs` unit. Cells that do
not fit, or that cannot be reached from the entrance, are left out. The game starts with the
building already built, at no cost.

Dwellings names its rooms: room, corridor, stairhall, armoury, attic, bathroom, bedroom, cellar,
chapel, gallery, greenhouse, hall, kitchen, laboratory, library, lookout, observatory, secret
passage, living room, study, workshop, studio, trophy room, vault, storage.

## Testing a ruleset

`cargo test` plays a season of every folder in `games/` with a simple bot, which needs a
`bot.plan` in the folder: `dig x y room` and `build x y unit` lines that it works down each day,
doing what it can afford. `cargo run --example season -- games/<id> 1 2` prints what happens
night by night.

## Commands

`dig {x, y, z, room}`, `build {x, y, z, unit}`, `remove {x, y, z}` (half the cost back),
`wall {x, y, z, side, door}`, `unwall {x, y, z, side}`, `turn {x, y, z}` and
`paint {x, y, z, finish}` in building phases (`z` is the level,
0 by default; `side` is `n`, `e`, `s` or `w`); `next` ends a phase that waits for the player;
`tick {n}` runs a timed phase on; `eject {guest}` shows a guest the door without a review;
`plan {cells, walls, stairs}` builds a whole building, free, as a game's first command (an
import); `rename {name}` gives the inn a name of the player's own. See [the engine](ENGINE.md).
