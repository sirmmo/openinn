//! OpenInn in the browser. The page sends JSON requests and gets JSON answers through three
//! functions, with no generated glue (`web/js/engine.js` wraps them):
//!
//! ```js
//! const bytes = new TextEncoder().encode(JSON.stringify(request));
//! const ptr = wasm.inn_alloc(bytes.length);
//! new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
//! const out = wasm.inn_call(ptr, bytes.length);      // frees the request
//! const answer = JSON.parse(new TextDecoder().decode(
//!     new Uint8Array(wasm.memory.buffer, out, wasm.inn_out_len())));
//! ```
//!
//! Requests (`op`):
//!
//! * `start` `{files: [[name, text], ...], seed, commands?}`: compiles the ruleset and starts a
//!   game (replaying `commands`, a save). Answers `{catalog, view, events}`.
//! * `apply` `{command}` (`{"do": "dig", "x": 1, "y": 2, "room": "taproom"}`, see
//!   `openinn::Command`): answers `{view, events}`, or `{error}` and nothing changes.
//! * `view`; `save` (answers `{seed, commands}`).

use std::cell::RefCell;

use openinn::{load, Command, Game};
use serde_json::{json, Value};

thread_local! {
    static GAME: RefCell<Option<Game>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Room for a request of `len` bytes.
#[no_mangle]
pub extern "C" fn inn_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Answers the request at `ptr` (and frees it); the answer is at the returned pointer, for
/// [`inn_out_len`] bytes, until the next call.
///
/// # Safety
/// `ptr` must come from [`inn_alloc`] with `len` bytes written.
#[no_mangle]
pub unsafe extern "C" fn inn_call(ptr: *mut u8, len: usize) -> *const u8 {
    let bytes = Vec::from_raw_parts(ptr, len, len.max(1));
    let answer = match std::str::from_utf8(&bytes)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(t).ok())
    {
        Some(req) => handle(&req).unwrap_or_else(|e| json!({ "error": e })),
        None => json!({ "error": "the request is not JSON" }),
    };
    OUT.with(|o| {
        let mut o = o.borrow_mut();
        *o = answer.to_string().into_bytes();
        o.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn inn_out_len() -> usize {
    OUT.with(|o| o.borrow().len())
}

/// Answers one request (also usable natively, for tests).
pub fn handle(req: &Value) -> Result<Value, String> {
    let op = req["op"].as_str().ok_or("`op` is missing")?;
    match op {
        "start" => {
            let files: Vec<(String, String)> =
                serde_json::from_value(req["files"].clone()).map_err(|e| format!("files: {e}"))?;
            let seed = req["seed"].as_str().unwrap_or("1");
            let rs = load(&files).map_err(|e| e.to_string())?;
            let catalog = rs.catalog();
            let mut game = Game::new(rs, seed);
            let mut events = Vec::new();
            if let Some(cmds) = req.get("commands").filter(|c| !c.is_null()) {
                let cmds: Vec<Command> =
                    serde_json::from_value(cmds.clone()).map_err(|e| format!("commands: {e}"))?;
                for c in cmds {
                    events.extend(
                        game.apply(c)
                            .map_err(|e| format!("the save does not replay: {e}"))?,
                    );
                }
            }
            let view = serde_json::to_value(game.snapshot()).map_err(|e| e.to_string())?;
            GAME.with(|g| *g.borrow_mut() = Some(game));
            Ok(json!({ "catalog": catalog, "view": view, "events": events }))
        }
        _ => GAME.with(|g| {
            let mut g = g.borrow_mut();
            let game = g.as_mut().ok_or("start a game first")?;
            match op {
                "apply" => {
                    let c: Command = serde_json::from_value(req["command"].clone())
                        .map_err(|e| format!("command: {e}"))?;
                    let events = game.apply(c)?;
                    Ok(json!({ "view": game.snapshot(), "events": events }))
                }
                "view" => Ok(json!({ "view": game.snapshot() })),
                "save" => Ok(json!({ "seed": game.seed, "commands": game.commands })),
                _ => Err(format!("unknown op {op:?}")),
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Value {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/dungeon-inn");
        let mut out = Vec::new();
        for name in ["economy.gbx", "inn.inn"] {
            out.push(json!([
                name,
                std::fs::read_to_string(dir.join(name)).unwrap()
            ]));
        }
        Value::Array(out)
    }

    #[test]
    fn messages_play_and_save_a_game() {
        let a = handle(&json!({ "op": "start", "files": files(), "seed": "web" })).unwrap();
        assert_eq!(a["catalog"]["inn"]["name"], "The Gilded Mimic");
        assert_eq!(a["view"]["phase"], "day");
        let dig =
            json!({ "op": "apply", "command": { "do": "dig", "x": 6, "y": 1, "room": "taproom" } });
        assert!(
            handle(&dig).unwrap()["view"]["cells"]
                .as_array()
                .unwrap()
                .len()
                == 2
        );
        assert!(handle(&dig).unwrap_err().contains("already dug"));
        handle(&json!({ "op": "apply", "command": { "do": "next" } })).unwrap();
        let t = handle(&json!({ "op": "apply", "command": { "do": "tick", "n": 30 } })).unwrap();
        assert_eq!(t["view"]["phase_tick"], 30);
        let save = handle(&json!({ "op": "save" })).unwrap();
        let b = handle(&json!({ "op": "start", "files": files(), "seed": save["seed"], "commands": save["commands"] })).unwrap();
        assert_eq!(b["view"], t["view"]);
    }
}
