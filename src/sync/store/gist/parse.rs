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

// inline: map_failure, check_auth and id_from_create_output are crate-private pure parsers
#[cfg(test)]
mod tests {
    use super::{check_auth, id_from_create_output, map_failure, StoreError};

    #[test]
    fn a_404_is_not_found() {
        let gh = "gh: Not Found (HTTP 404)";
        assert_eq!(map_failure("gh", gh, Some(1)), StoreError::NotFound);
    }

    #[test]
    fn git_not_found_is_gone_but_missing_credentials_are_not() {
        // git authenticates through gh, as such plain "not found" means gone or not ours.
        let git = "remote: Repository not found.\nfatal: repository 'https://gist.github.com/x.git/' not found";
        assert_eq!(map_failure("git", git, Some(128)), StoreError::NotFound);
        let cred = "fatal: could not read Username for 'https://gist.github.com': terminal prompts disabled\nremote: Repository not found.";
        let err = map_failure("git", cred, Some(128)).to_string();
        assert!(err.contains("gh auth setup-git"), "{err}");
    }

    #[test]
    fn create_output_gives_the_id() {
        let out = "https://gist.github.com/someone/0123abcd4567\n";
        assert_eq!(id_from_create_output(out).as_deref(), Some("0123abcd4567"));
        assert_eq!(id_from_create_output("- Creating gist...\n"), None);
        assert_eq!(id_from_create_output(""), None);
    }

    #[test]
    fn failures_map_to_variants() {
        let gh = |s| map_failure("gh", s, Some(1));
        assert_eq!(
            gh("You are not logged into any GitHub hosts. To log in, run: gh auth login"),
            StoreError::NotLoggedIn
        );
        assert_eq!(gh("HTTP 404: Not Found (https://api.github.com/gists)\nThis API operation needs the \"gist\" scope."), StoreError::MissingScope);
        assert!(matches!(gh("dial tcp: lookup api.github.com: no such host\nCould not resolve host: api.github.com"), StoreError::Unreachable(_)));
        assert!(matches!(
            gh("connection timed out"),
            StoreError::Unreachable(_)
        ));
    }

    #[test]
    fn rate_limits_map_to_their_own_variant() {
        let gh = |s| map_failure("gh", s, Some(1));
        for text in [
            "gh: API rate limit exceeded for user ID 1. (HTTP 403)",
            "HTTP 429: Too Many Requests (https://api.github.com/gists)",
            "HTTP 403: You have exceeded a secondary rate limit. Please wait a few minutes",
            "remote: You have exceeded a secondary rate limit.\nfatal: unable to access",
        ] {
            assert_eq!(gh(text), StoreError::RateLimited, "{text}");
        }
        assert_eq!(
            map_failure("git", "HTTP 429", Some(128)),
            StoreError::RateLimited
        );
        let plain = gh("HTTP 403: Resource not accessible");
        assert!(matches!(plain, StoreError::Other(_)), "{plain:?}");
    }

    #[test]
    fn git_push_rate_limit_is_rate_limited() {
        let text = "fatal: unable to access 'https://gist.github.com/x.git/': The requested URL returned error: 429";
        assert_eq!(map_failure("git", text, Some(128)), StoreError::RateLimited);
    }

    #[test]
    fn a_path_echoing_rate_limit_is_not_rate_limited() {
        let text = "error: pathspec 'notes/rate limit ideas.cast.gz' did not match any file";
        let err = map_failure("git", text, Some(1));
        assert!(matches!(err, StoreError::Other(_)), "{err:?}");
    }

    #[test]
    fn git_push_rejection_is_rejected() {
        for text in [
            "To https://gist.github.com/x.git\n ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs",
            "hint: Updates were rejected because the tip of your current branch is behind\nhint: (non-fast-forward)",
        ] {
            assert_eq!(map_failure("git", text, Some(1)), StoreError::Moved(String::new()), "{text}");
        }
    }

    #[test]
    fn git_credential_failure_hints_setup_git() {
        let git = |s| map_failure("git", s, Some(128));
        for text in [
            "fatal: could not read Username for 'https://gist.github.com': terminal prompts disabled",
            "remote: Invalid username or password.\nfatal: Authentication failed for 'https://gist.github.com/x.git/'",
        ] {
            let err = git(text).to_string();
            assert!(err.contains("gh auth setup-git"), "{err}");
        }
    }

    #[test]
    fn unknown_failure_does_not_echo_stderr() {
        let err = map_failure("git", "secret-looking ghp_abc123 text", Some(1)).to_string();
        assert!(!err.contains("ghp_abc123"));
        assert!(err.contains("git"));
    }

    #[test]
    fn auth_status_needs_gist_scope() {
        let ok = "github.com\n  - Logged in to github.com account me\n  - Token scopes: 'gist', 'read:org', 'repo'\n";
        assert_eq!(check_auth(ok), Ok(()));
        let none = "github.com\n  - Token scopes: 'read:org', 'repo'\n";
        assert_eq!(check_auth(none), Err(StoreError::MissingScope));
    }

    #[test]
    fn auth_status_without_scopes_line_passes() {
        let fine_grained = "github.com\n  - Logged in to github.com account me\n";
        assert_eq!(check_auth(fine_grained), Ok(()));
    }
    #[test]
    fn old_gh_without_a_flag_asks_for_an_update() {
        let err = map_failure("gh", "unknown flag: --yes\n\nUsage: ...", Some(1));
        assert!(err.to_string().contains("update gh"), "{err}");
    }
}
