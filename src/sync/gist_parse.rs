//! Pure parsing for the gist backend, no process and no disk.

use super::backend::{BackendError, RemoteEntry};

/// Description prefix of every agr gist.
pub const DESC_PREFIX: &str = "agr:v1 ";

#[derive(serde::Deserialize)]
struct Gist {
    id: String,
    description: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
}

/// GitHub stops listing after this many gists, see https://docs.github.com/en/rest/gists/gists.
pub const LIST_LIMIT: usize = 3000;

/// Parse `gh api gists --paginate`, which prints one JSON array per page back to back.
pub fn parse_list(json: &str) -> Result<Vec<RemoteEntry>, BackendError> {
    let pages = serde_json::Deserializer::from_str(json).into_iter::<Vec<Gist>>();
    let mut entries = Vec::new();
    let mut total = 0;
    for page in pages {
        let page = page.map_err(|_| BackendError::Other("unexpected gist list output".into()))?;
        total += page.len();
        entries.extend(page.into_iter().filter_map(to_entry));
    }
    // A capped listing would make gists past the cap look missing, so refuse it.
    if total >= LIST_LIMIT {
        return Err(BackendError::Other(format!(
            "GitHub lists at most {LIST_LIMIT} gists and this account has that many, sync cannot \
             tell which sessions exist, delete old gists and retry \
             (https://docs.github.com/en/rest/gists/gists)"
        )));
    }
    Ok(entries)
}

fn to_entry(gist: Gist) -> Option<RemoteEntry> {
    let desc = gist.description?;
    let token = desc.strip_prefix(DESC_PREFIX)?.trim().to_string();
    Some(RemoteEntry {
        id: gist.id,
        meta_token: token,
        created_at: gist.created_at.unwrap_or_default(),
    })
}

/// Gist id from the URL `gh gist create` prints.
pub fn id_from_create_output(stdout: &str) -> Option<String> {
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
/// Only the `gh` API says this, git prints "repository not found" for auth failures too.
const NOT_FOUND: [&str; 2] = ["http 404", "not found (http"];
const GIT_NO_ACCESS: &str = "repository not found";
const REJECTED: [&str; 3] = ["! [rejected]", "non-fast-forward", "fetch first"];

/// Map a failed `gh` or `git` run to one clear error, never echoes raw stderr.
pub fn map_failure(tool: &'static str, stderr: &str, code: Option<i32>) -> BackendError {
    let text = stderr.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| text.contains(n));
    if has(&RATE_LIMIT) {
        BackendError::RateLimited
    } else if tool == "git" && has(&REJECTED) {
        BackendError::Changed(None)
    } else if text.contains("not logged in") || text.contains("gh auth login") {
        BackendError::NotLoggedIn
    } else if text.contains("scope") {
        BackendError::MissingScope
    } else if tool == "gh" && has(&NOT_FOUND) {
        BackendError::NotFound
    } else if has(&NETWORK) {
        BackendError::Unreachable("no connection to github.com".into())
    } else if tool == "git" && has(&CREDENTIALS) {
        BackendError::Other("git has no credentials for gists, run `gh auth setup-git`".into())
    } else if tool == "git" && text.contains(GIT_NO_ACCESS) {
        BackendError::Other("git cannot open the gist (no access or it is gone)".into())
    } else {
        BackendError::Other(format!("{tool} failed (exit {code:?})"))
    }
}

/// Check `gh auth status` output, stdout and stderr together.
pub fn check_auth(output: &str) -> Result<(), BackendError> {
    let scopes = output.lines().find(|l| l.contains("Token scopes"));
    match scopes {
        Some(line) if line.contains("gist") => Ok(()),
        Some(_) => Err(BackendError::MissingScope),
        None => Err(BackendError::NotLoggedIn),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, token: &str) -> RemoteEntry {
        RemoteEntry {
            id: id.into(),
            meta_token: token.into(),
            created_at: String::new(),
        }
    }

    #[test]
    fn list_reads_created_at() {
        let json = r#"[{"id":"a1","description":"agr:v1 T","created_at":"2026-01-02T03:04:05Z"}]"#;
        let got = parse_list(json).unwrap();
        assert_eq!(got[0].created_at, "2026-01-02T03:04:05Z");
    }

    #[test]
    fn a_404_is_not_found() {
        let gh = "gh: Not Found (HTTP 404)";
        assert_eq!(map_failure("gh", gh, Some(1)), BackendError::NotFound);
    }

    #[test]
    fn git_repository_not_found_is_never_not_found() {
        let git = "remote: Repository not found.\nfatal: repository 'https://gist.github.com/x.git/' not found";
        let err = map_failure("git", git, Some(128));
        assert!(
            matches!(&err, BackendError::Other(m) if m.contains("no access")),
            "{err:?}"
        );
        let cred = "fatal: could not read Username for 'https://gist.github.com': terminal prompts disabled\nremote: Repository not found.";
        let err = map_failure("git", cred, Some(128)).to_string();
        assert!(err.contains("gh auth setup-git"), "{err}");
    }

    #[test]
    fn list_keeps_only_agr_gists_across_pages() {
        let page1 =
            r#"[{"id":"a1","description":"agr:v1 TOKEN1"},{"id":"b2","description":"my notes"}]"#;
        let page2 = r#"[{"id":"c3","description":null},{"id":"d4","description":"agr:v1 TOKEN4"}]"#;
        let got = parse_list(&format!("{page1}{page2}")).unwrap();
        assert_eq!(got, vec![entry("a1", "TOKEN1"), entry("d4", "TOKEN4")]);
    }

    /// `n` gists in pages of 100, the first one an agr gist, the rest foreign.
    fn listing(n: usize) -> String {
        let all: Vec<String> = (0..n)
            .map(|i| match i {
                0 => r#"{"id":"g0","description":"agr:v1 T"}"#.to_string(),
                _ => format!(r#"{{"id":"g{i}","description":null}}"#),
            })
            .collect();
        all.chunks(100)
            .map(|c| format!("[{}]", c.join(",")))
            .collect()
    }

    #[test]
    fn list_aborts_at_3000_gists_counting_foreign_ones() {
        let got = parse_list(&listing(3000));
        let text = format!("{got:?}");
        assert!(got.is_err(), "{text}");
        assert!(
            text.contains("3000") && text.contains("docs.github.com"),
            "{text}"
        );
        assert!(parse_list(&listing(3500)).is_err());
    }

    #[test]
    fn list_below_the_limit_is_fine() {
        assert_eq!(parse_list(&listing(2999)).unwrap(), vec![entry("g0", "T")]);
    }

    #[test]
    fn list_of_nothing_and_garbage() {
        assert_eq!(parse_list("[]").unwrap(), vec![]);
        assert_eq!(parse_list("").unwrap(), vec![]);
        assert!(matches!(parse_list("<html>"), Err(BackendError::Other(_))));
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
            BackendError::NotLoggedIn
        );
        assert_eq!(gh("HTTP 404: Not Found (https://api.github.com/gists)\nThis API operation needs the \"gist\" scope."), BackendError::MissingScope);
        assert!(matches!(gh("dial tcp: lookup api.github.com: no such host\nCould not resolve host: api.github.com"), BackendError::Unreachable(_)));
        assert!(matches!(
            gh("connection timed out"),
            BackendError::Unreachable(_)
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
            assert_eq!(gh(text), BackendError::RateLimited, "{text}");
        }
        assert_eq!(
            map_failure("git", "HTTP 429", Some(128)),
            BackendError::RateLimited
        );
        let plain = gh("HTTP 403: Resource not accessible");
        assert!(matches!(plain, BackendError::Other(_)), "{plain:?}");
    }

    #[test]
    fn git_push_rate_limit_is_rate_limited() {
        let text = "fatal: unable to access 'https://gist.github.com/x.git/': The requested URL returned error: 429";
        assert_eq!(
            map_failure("git", text, Some(128)),
            BackendError::RateLimited
        );
    }

    #[test]
    fn a_path_echoing_rate_limit_is_not_rate_limited() {
        let text = "error: pathspec 'notes/rate limit ideas.cast.gz' did not match any file";
        let err = map_failure("git", text, Some(1));
        assert!(matches!(err, BackendError::Other(_)), "{err:?}");
    }

    #[test]
    fn git_push_rejection_is_rejected() {
        for text in [
            "To https://gist.github.com/x.git\n ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs",
            "hint: Updates were rejected because the tip of your current branch is behind\nhint: (non-fast-forward)",
        ] {
            assert_eq!(map_failure("git", text, Some(1)), BackendError::Changed(None), "{text}");
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
        assert_eq!(check_auth(none), Err(BackendError::MissingScope));
    }
}
