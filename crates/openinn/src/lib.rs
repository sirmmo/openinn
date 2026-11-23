//! OpenInn: an inn-keeping engine whose rules are GlassBox scripts.
//!
//! A ruleset (a folder of `*.gbx` and `*.inn` files) declares the inn, its phases, the rooms you
//! dig, the furniture and stations you put in them, the guests who come and what they need, and
//! the services that meet those needs. The economy between them (cooking, brewing, buying
//! supplies, noise and dirt spreading from room to room) is plain GlassBox: unit rules, map
//! rules and global resources, run by the [`glassbox`] crate on the graph of dug cells.
//!
//! The engine adds what GlassBox has no words for: persistent guests with needs, a purse, a
//! mood and patience, who walk the inn to whatever serves their most pressing need, pay, and
//! leave a review; and phases (building by day, serving by night) that set global flags the
//! rules can test.

pub mod dwellings;
mod script;
mod sim;

pub use glassbox::Error;
pub use script::{
    load, load_dir, GuestDef, InnDef, NeedDef, PhaseDef, RoomDef, Ruleset, ServiceDef, UnitKind,
    HOST_KINDS,
};
pub use sim::{
    CellView, Command, Event, Game, GuestView, Mode, PlanAt, PlanCell, PlanWall, Side, Snapshot,
    Stats, UnitView, WallView,
};
