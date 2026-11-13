//! The only network code: one bounded `curl` process per request, redirects checked by agr.

use super::{API_URL, ASSET_PREFIX};
use anyhow::{anyhow, bail, Result};
use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const OVERRIDE_VAR: &str = "AGR_UPDATE_RELEASE_BASE";
const HOSTS: [&str; 4] = [
    "api.github.com",
    "github.com",
    "release-assets.githubusercontent.com",
    "objects.githubusercontent.com",
];
const MAX_HOPS: usize = 3;

/// Where releases come from: the fixed GitHub repo, or a test override.
#[derive(Debug, Clone)]
pub struct Source {
    pub api_url: String,
    pub prefix: String,
    local: bool,
}

impl Source {
    pub fn github() -> Self {
        Self {
            api_url: API_URL.into(),
            prefix: ASSET_PREFIX.into(),
            local: false,
        }
    }

    /// Build from the override base. Only `https://` and `file://` are accepted.
    pub fn from_override(base: &str) -> Result<Self> {
        if !(base.starts_with("https://") || base.starts_with("file://")) {
            bail!("{OVERRIDE_VAR} must start with https:// or file://");
        }
        let base = base.trim_end_matches('/');
        Ok(Self {
            api_url: format!("{base}/latest.json"),
            prefix: format!("{base}/download/"),
            local: base.starts_with("file://"),
        })
    }

    /// The fixed source, or the override when `AGR_UPDATE_RELEASE_BASE` is set.
    pub fn from_env() -> Result<Self> {
        match std::env::var(OVERRIDE_VAR) {
            Ok(base) => Self::from_override(&base),
            Err(_) => Ok(Self::github()),
        }
    }

    /// Fetch the latest release JSON.
    pub fn fetch_api(&self) -> Result<String> {
        text(&self.api_url, Kind::Api, self.local)
    }

    /// Fetch a small text file such as `SHA256SUMS`.
    pub fn fetch_text(&self, url: &str) -> Result<String> {
        text(url, Kind::Text, self.local)
    }

    /// Download the binary straight into `dest`.
    pub fn fetch_file(&self, url: &str, dest: &Path) -> Result<()> {
        follow(url, |u| hop(u, Kind::Binary, Some(dest), self.local)).map(|_| ())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Api,
    Text,
    Binary,
}

/// One request result: the body, or the next URL to ask.
#[derive(Debug)]
pub enum Hop {
    Done(Vec<u8>),
    Redirect(String),
}

/// True for `https://<allowed host>/...` without userinfo or port.
pub fn host_allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or("");
    !host.is_empty() && !host.contains(['@', ':']) && HOSTS.contains(&host)
}

/// One line for a curl exit code.
pub fn classify_exit(code: i32, stderr: &str) -> String {
    match code {
        6 => "DNS failure, cannot resolve the host".into(),
        7 => "cannot connect, are you offline".into(),
        28 => "timeout, the server did not answer in time".into(),
        63 => "download too large".into(),
        _ => format!(
            "curl failed with exit code {code}: {}",
            stderr.lines().next().unwrap_or("no details")
        ),
    }
}

/// One line for an HTTP status, `None` when it is fine.
pub fn classify_status(code: u32) -> Option<String> {
    match code {
        403 | 429 => Some(format!("GitHub rate limit (HTTP {code}), try again later")),
        c if c >= 400 => Some(format!("HTTP error {c}")),
        _ => None,
    }
}

/// The curl argument list for one request, never run through a shell.
pub fn curl_args(url: &str, kind: Kind, out: Option<&Path>) -> Vec<String> {
    let proto = if url.starts_with("file://") {
        "=file"
    } else {
        "=https"
    };
    let (time, size) = if kind == Kind::Binary {
        ("300", "64M")
    } else {
        ("30", "1M")
    };
    let mut a: Vec<String> = [
        "--disable",
        "--silent",
        "--show-error",
        "--proto",
        proto,
        "--connect-timeout",
        "10",
        "--max-time",
        time,
        "--max-filesize",
        size,
        "--write-out",
        "%{stderr}%{http_code} %{redirect_url}",
    ]
    .map(String::from)
    .to_vec();
    if kind == Kind::Api {
        a.extend(["-H".into(), "Accept: application/vnd.github+json".into()]);
    }
    if let Some(out) = out {
        a.extend(["--output".into(), out.display().to_string()]);
    }
    a.push(url.to_string());
    a
}

/// Spawn curl. `path` replaces `PATH` for the lookup, used by tests.
pub fn run_curl(args: &[String], path: Option<&OsStr>) -> Result<Output> {
    let mut cmd = Command::new("curl");
    cmd.args(args).stdin(Stdio::null());
    if let Some(path) = path {
        cmd.env("PATH", path);
    }
    cmd.output().map_err(|e| match e.kind() {
        ErrorKind::NotFound => anyhow!("agr update needs curl, install it and retry"),
        _ => anyhow!("cannot run curl: {e}"),
    })
}

fn hop(url: &str, kind: Kind, out: Option<&Path>, local: bool) -> Result<Hop> {
    let output = run_curl(&curl_args(url, kind, out), None)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(code) = output.status.code().filter(|c| *c != 0) {
        bail!("{}", classify_exit(code, &stderr));
    }
    if local {
        return Ok(Hop::Done(output.stdout));
    }
    let last = stderr.lines().last().unwrap_or("");
    let (code, next) = last.split_once(' ').unwrap_or((last, ""));
    let code: u32 = code
        .parse()
        .map_err(|_| anyhow!("malformed curl status line"))?;
    if (300..400).contains(&code) && !next.is_empty() {
        return Ok(Hop::Redirect(next.to_string()));
    }
    match classify_status(code) {
        Some(msg) => bail!("{msg}"),
        None => Ok(Hop::Done(output.stdout)),
    }
}

/// Run `request` and follow its redirects, at most 3, each target on the allowlist.
pub fn follow(url: &str, mut request: impl FnMut(&str) -> Result<Hop>) -> Result<Vec<u8>> {
    let mut url = url.to_string();
    for _ in 0..=MAX_HOPS {
        match request(&url)? {
            Hop::Done(body) => return Ok(body),
            Hop::Redirect(next) if host_allowed(&next) => url = next,
            Hop::Redirect(next) => bail!("redirect refused, host not allowed: {next}"),
        }
    }
    bail!("redirect refused, more than {MAX_HOPS} hops")
}

fn text(url: &str, kind: Kind, local: bool) -> Result<String> {
    let body = follow(url, |u| hop(u, kind, None, local))?;
    String::from_utf8(body).map_err(|_| anyhow!("malformed release response, not text"))
}
