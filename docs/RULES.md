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
end
```

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
end
```

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
end
```

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
    map Noise out 0.1          # emitted where they stand, every tick
    model character-wizard     # a hint for the view
end
```

Tests (in `if` and `dislike`) are `map <layer> <cmp> <value>` (at the guest's cell, or the
entrance for `if`) or `global <resource> <cmp> <value>`, with GlassBox's comparisons: `atLeast`,
`atMost`, `greater`, `less`, `is`.

A guest's mood starts at 0.6, rises by 0.1 with every service, drops by 0.01 for every tick spent
waiting for something nobody offers, and by their dislikes. A guest leaving with mood `m` moves
reputation by `(m - 0.5) × 2 × review`.

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
    help help.html             # the help window's text: an HTML file in the ruleset's folder
    style style.css            # a stylesheet for the page, in the ruleset's folder
end
```

The words for money and reputation in the interface are the names of their resources
(`money Credits`: "3 credits").

## Model hints

`floor` in a room and `model` in a unit or guest name models for the client. A unit or room
may use several models, separated by spaces, each as `name:x:z:degrees:scale`, with x and z in
cells from the centre of the cell (towards the wall behind it is −z). A table with two chairs:

```text
    model table chair:0:-0.3:0 chair:0:0.3:180
```

Units turn their back to a wall when they have one.

## Testing a ruleset

`cargo test` plays a season of every folder in `games/` with a simple bot, which needs a
`bot.plan` in the folder: `dig x y room` and `build x y unit` lines that it works down each day,
doing what it can afford. `cargo run --example season -- games/<id> 1 2` prints what happens
night by night.

## Commands

`dig {x, y, room}`, `build {x, y, unit}`, `remove {x, y}` (half the cost back) in building
phases; `next` ends a phase that waits for the player; `tick {n}` runs a timed phase on;
`eject {guest}` shows a guest the door without a review.
