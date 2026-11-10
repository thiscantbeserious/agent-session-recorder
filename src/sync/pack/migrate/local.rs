//! The local sync index of the old layout, read as plain JSON so the pack layer never names the engine.

use serde_json::{json, Value};

/// The v2 text of a v1 index, `None` when it already is v2.
pub fn convert_index(text: &str) -> Result<Option<String>, String> {
    let old: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if old["version"].as_u64().unwrap_or(1) >= 2 {
        return Ok(None);
    }
    let mut entries = serde_json::Map::new();
    for (path, e) in old["entries"].as_object().into_iter().flatten() {
        entries.insert(path.clone(), json!({ "sha256": e["sha256"], "remote_id": "", "synced_at": e["synced_at"], "gen": 1 }));
    }
    let v2 = json!({ "version": 2, "entries": entries, "open_pack": null });
    Ok(Some(v2.to_string()))
}
