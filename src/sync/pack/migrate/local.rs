//! The local sync index of the old layout, read as plain JSON so the pack layer never names the engine.

use serde_json::{json, Map, Value};

/// What v2 keeps of a v1 entry: hash and gen so the plan keeps its last synced state, the
/// stamp so an unchanged file is not hashed again. `remote_id`, `token` and `pending` are gone.
fn carry(e: &Value) -> Value {
    json!({
        "sha256": e["sha256"],
        "remote_id": "",
        "synced_at": e["synced_at"],
        "gen": e["gen"].as_u64().unwrap_or(0),
        "size": e["size"].as_u64().unwrap_or(0),
        "mtime_ns": e["mtime_ns"].as_u64().unwrap_or(0),
    })
}

/// The v2 text of a v1 index, `None` when it already is v2. A pending entry never synced
/// anything, so it is dropped and its local file reads as new.
pub fn convert_index(text: &str) -> Result<Option<String>, String> {
    let old: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if old["version"].as_u64().unwrap_or(1) >= 2 {
        return Ok(None);
    }
    let entries: Map<String, Value> = (old["entries"].as_object().into_iter().flatten())
        .filter(|(_, e)| e["pending"] != true)
        .map(|(path, e)| (path.clone(), carry(e)))
        .collect();
    let v2 = json!({ "version": 2, "entries": entries, "open_pack": null });
    Ok(Some(v2.to_string()))
}
