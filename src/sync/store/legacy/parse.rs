//! Pure parsing of the old gist listing, no process and no disk.

use super::{LegacyGist, StoreError};

#[derive(serde::Deserialize)]
struct AnyGist {
    id: String,
    description: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    files: std::collections::BTreeMap<String, AnyFile>,
    /// GitHub lists at most 300 files, the rest is cut.
    #[serde(default)]
    truncated: bool,
}

#[derive(serde::Deserialize)]
struct AnyFile {
    #[serde(default)]
    size: u64,
}

/// Every gist of the listing with its files, for `agr sync migrate`. Unlike `parse_list` there
/// is no cap, migrate acts on what it saw and is run again.
pub fn parse_all(json: &str) -> Result<Vec<LegacyGist>, StoreError> {
    let pages = serde_json::Deserializer::from_str(json).into_iter::<Vec<AnyGist>>();
    let mut out = Vec::new();
    for page in pages {
        let page = page.map_err(|_| StoreError::Other("unexpected gist list output".into()))?;
        out.extend(page.into_iter().map(|g| LegacyGist {
            id: g.id,
            description: g.description.unwrap_or_default(),
            files: g.files.into_iter().map(|(n, f)| (n, f.size)).collect(),
            created_at: g.created_at.unwrap_or_default(),
            truncated: g.truncated,
        }));
    }
    Ok(out)
}
