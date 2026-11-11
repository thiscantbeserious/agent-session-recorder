//! Pure parsing for the gist backend, no process and no disk.

use crate::sync::store::StoreError;

/// Description prefix of every agr gist.
pub(crate) const DESC_PREFIX: &str = "agr:v1 ";

/// GitHub stops listing after this many gists, see https://docs.github.com/en/rest/gists/gists.
pub(crate) const LIST_LIMIT: usize = 3000;

/// Gist id from the URL `gh gist create` prints.
pub(crate) fn id_from_create_output(stdout: &str) -> Option<String> {
    let url = stdout.lines().find(|l| l.contains("gist.github.com/"))?;
    let id = url.trim().rsplit('/').next()?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())).then(|| id.to_string())
}

const NETWORK: [&str; 4] = [
    "could not resolve host",
    "no such host",
    "timed out",
    "network is unreachable",
];
const CREDENTIALS: [&str; 3] = [
    "could not read username",
    "authentication failed",
    "invalid username or password",
];

const RATE_LIMIT: [&str; 4] = [
    "api rate limit",
    "secondary rate limit",
    "http 429",
    "error: 429",
];
/// git prints this for a missing gist, and also without credentials (matched first, see CREDENTIALS).
const NOT_FOUND: [&str; 2] = ["http 404", "not found (http"];
const GIT_NO_ACCESS: &str = "repository not found";
const REJECTED: [&str; 3] = ["! [rejected]", "non-fast-forward", "fetch first"];

/// A `git push` the remote rejected is `Moved` with no index, the caller fetches the current one.
/// Map a failed `gh` or `git` run to one clear error, never echoes raw stderr.
pub(crate) fn map_failure(tool: &'static str, stderr: &str, code: Option<i32>) -> StoreError {
    let text = stderr.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| text.contains(n));
    if has(&RATE_LIMIT) {
        StoreError::RateLimited
    } else if tool == "git" && has(&REJECTED) {
        StoreError::Moved(String::new())
    } else if text.contains("not logged in") || text.contains("gh auth login") {
        StoreError::NotLoggedIn
    } else if text.contains("scope") {
        StoreError::MissingScope
    } else if tool == "gh" && has(&NOT_FOUND) {
        StoreError::NotFound
    } else if has(&NETWORK) {
        StoreError::Unreachable("no connection to github.com".into())
    } else if tool == "git" && has(&CREDENTIALS) {
        StoreError::Other("git has no credentials for gists, run `gh auth setup-git`".into())
    } else if tool == "gh" && text.contains("unknown flag") {
        StoreError::Other("gh is too old for agr sync, update gh".into())
    } else if tool == "git" && text.contains(GIT_NO_ACCESS) {
        // git always authenticates through `gh` here and `check` verified the login, as
        // such "not found" means the gist is gone or not ours, both are unreachable.
        StoreError::NotFound
    } else {
        StoreError::Other(format!("{tool} failed (exit {code:?})"))
    }
}

/// Check `gh auth status` output, stdout and stderr together. Only called after exit 0.
/// Fine-grained tokens print no scopes line, a missing gist permission then shows on
/// the first gist call.
pub(crate) fn check_auth(output: &str) -> Result<(), StoreError> {
    let scopes = output.lines().find(|l| l.contains("Token scopes"));
    match scopes {
        Some(line) if line.contains("gist") => Ok(()),
        Some(_) => Err(StoreError::MissingScope),
        None => Ok(()),
    }
}
