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
end
```

## room

```text
room taproom
    name Taproom
    cost 10                    # money per cell dug
    colour b5734a              # for maps and the interface
    floor floor_wood_large     # a hint for the view (a model name)
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
    model bar_straight_A       # a hint for the view
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

## Commands

`dig {x, y, room}`, `build {x, y, unit}`, `remove {x, y}` (half the cost back) in building
phases; `next` ends a phase that waits for the player; `tick {n}` runs a timed phase on;
`eject {guest}` shows a guest the door without a review.
