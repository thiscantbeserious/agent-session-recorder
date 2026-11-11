//! The local sync index of the old layout, read as plain JSON so the pack layer never names the engine.

use serde_json::{json, Map, Value};

/// What v2 keeps of a v1 entry: hash and gen so the plan keeps its last synced state, the
/// stamp so an unchanged file is not hashed again. `remote_id`, `token` and `pending` are gone.
fn carry(path: &str, e: &Value) -> Result<Value, String> {
    let text = |key: &str| {
        e[key]
            .as_str()
            .ok_or_else(|| format!("entry {path} has no {key}"))
    };
    Ok(json!({
        "sha256": text("sha256")?,
        "remote_id": "",
        "synced_at": text("synced_at")?,
        "gen": e["gen"].as_u64().unwrap_or(0),
        "size": e["size"].as_u64().unwrap_or(0),
        "mtime_ns": e["mtime_ns"].as_u64().unwrap_or(0),
    }))
}

/// The v2 text of a v1 index, `None` when it already is v2. A pending entry never synced
/// anything, so it is dropped and its local file reads as new.
pub fn convert_index(text: &str) -> Result<Option<String>, String> {
    let old: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if old["version"].as_u64().unwrap_or(1) >= 2 {
        return Ok(None);
    }
    let old_entries = old["entries"]
        .as_object()
        .ok_or("entries is not an object")?;
    let entries = (old_entries.iter())
        .filter(|(_, e)| e["pending"] != true)
        .map(|(path, e)| Ok((path.clone(), carry(path, e)?)))
        .collect::<Result<Map<String, Value>, String>>()?;
    let v2 = json!({ "version": 2, "entries": entries, "open_pack": null });
    Ok(Some(v2.to_string()))
}

/// Same name as the engine's index file, the pack layer must not name the engine.
const INDEX_FILE: &str = ".agr-sync.json";

/// Converts `<dir>/.agr-sync.json` in place when it is v1, true when it was written.
/// A missing file is left alone, the engine starts an empty v2 from it. A file that exists
/// but cannot be read or parsed aborts, the engine would rebuild it empty and plan unsynced
/// local edits as new.
pub fn convert_file(dir: &std::path::Path) -> Result<bool, String> {
    let path = dir.join(INDEX_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("could not read {}: {e}", path.display())),
    };
    let converted = convert_index(&text).map_err(|e| {
        format!(
            "{} is not valid ({e}), fix or remove it first",
            path.display()
        )
    })?;
    let Some(v2) = converted else {
        return Ok(false);
    };
    crate::files::codec::atomic_write(&path, |w| Ok(w.write_all(v2.as_bytes())?))
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(true)
}
