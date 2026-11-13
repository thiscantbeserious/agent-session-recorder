# Requirements: agr update (self-update from GitHub releases)

## Problem Statement

Users of `agr` who installed a release binary have no way to get a newer version except downloading and replacing it by hand. We want one command that does this, without the risk of replacing a working binary with a broken or tampered one.

## Desired Outcome

`agr update` downloads the latest GitHub release binary for the current platform, verifies it, and replaces the running binary. `agr update --check` only reports whether a newer version exists. Nothing else in `agr` touches the network because of this feature.

## Scope

### In Scope

- `agr update` and `agr update --check`.
- The three platforms we publish today: macos-arm64, macos-x86_64, linux-x86_64.
- Verification of the download before replacement.
- Safe replacement with rollback.
- A clear failure on unsupported platforms, Windows included.
- Release workflow changes needed so the download can be verified (see R5, R6).
- Correcting the `repository` field in `Cargo.toml` (see R19).

### Out of Scope

- Auto-update of any kind.
- Background or startup version checks, any network call outside `agr update`.
- Package-manager integration (brew, cargo-binstall, apt) and updating a cargo-installed binary through cargo.
- Windows support and a Windows release asset.
- Update channels (beta, nightly), pinning or downgrading to a specific version.
- Signature verification (possible later, see R6).

## Context (verified against the repo at 81d5bc4)

- Current version is 0.3.0 (`Cargo.toml`).
- Build matrix in `.github/workflows/release.yml` has linux-x86_64, macos-x86_64, macos-arm64 only. No Windows target.
- "Prepare release assets" renames each binary to its artifact name (`agr-linux-x86_64`, `agr-macos-x86_64`, `agr-macos-arm64`) and uploads `release-assets/*`. Assets are bare binaries, no archive, no checksum file, no signature.
- `agr` has no HTTP client dependency today. Network access exists only through the `gh` and `git` CLIs in the sync feature.
- `Cargo.toml` `repository` points at `simon/agent-session-recorder`, while the repo is `thiscantbeserious/agent-session-recorder`. The source of releases must be the latter (see R3, R19).

## Requirements

### Command surface

**R1. `agr update` upgrades to the latest release.**
- Given a newer release exists, the binary at the running path ends up being the new version, and `agr --version` reports it.
- Given the running version equals the latest, it prints that it is up to date, exits 0 and changes nothing on disk.
- Given the running version is newer than the latest release (dev build), it prints that and changes nothing.
- Prereleases and drafts are never offered.

**R2. `agr update --check` reports only.**
- It prints current version, latest version and whether an update is available.
- It never downloads an asset and never writes to disk, including temp files next to the binary.
- Exit codes: 0 when up to date (or running a newer dev build), 10 when an update is available, 1 when the check itself failed. The code 10 is fixed so scripts can rely on it.

**R3. Release source is fixed.**
- Only releases of `thiscantbeserious/agent-session-recorder` are consulted.
- There is no user-facing flag or documented env var that redirects it to another repo or host.
- A hidden environment override `AGR_UPDATE_RELEASE_BASE` exists only so tests can point `agr update` at a local fake server. It is undocumented, does not appear in `agr --help` or the README, and is not a supported interface.

### Platforms

**R4. Platform to asset mapping.**
- macOS arm64, macOS x86_64 and Linux x86_64 map to the matching published asset.
- Any other platform or architecture (linux-arm64, for example) fails before any download with a message naming the detected platform and the supported ones.
- Windows fails before any download with a message that states plainly that `agr update` is not supported on Windows because no Windows release exists, and points to the releases page for manual install. Exit non-zero. The message must not suggest a retry.
- `agr update --check` on Windows gives the same message.
- If the latest release has no asset for the current platform, it fails with a message naming the missing asset and the release version, and changes nothing.

### Verification

**R5. No unverified binary ever replaces anything.**
- The user can rely on this: if verification fails, cannot be performed, or the verification data is missing, the running binary is untouched and the command exits non-zero with the reason.
- There is no flag to skip verification.
- Verification happens on the fully downloaded file, before it is made executable at the final path and before any rename.

**R6. Verification is a SHA-256 checksum file.**
Releases today carry no checksum, so with the current workflow R5 can only be met by failing. `release.yml` must publish a SHA-256 checksum file next to the assets, and `agr update` verifies the downloaded asset against it before install. The first release containing the checksum file is the earliest that `agr update` can succeed.
- Limit: this catches corruption and partial downloads, not a compromised release (an attacker who can replace the asset can replace the checksum file too). Signatures or attestation are possible later and are out of scope here.
- The workflow change and the verifier ship together, and an acceptance test shows a tampered asset is rejected.

**R7. Behaviour against releases without verification data.**
- Given the latest release has no checksum file (every release up to and including 0.3.0), `agr update` refuses with a message saying so and that a manual install is needed.
- `--check` still works and reports the version. It also says that the newer release cannot be applied automatically because it has no checksums.

**R8. Downloaded binary sanity check.**
- Before replacement the file must be a non-empty executable for the current platform. A file that fails this is rejected like a failed verification (R5).
- The new binary is smoke-run with `--version` before the replacement, and the reported version must match the expected release version. This runs downloaded code only after the R6 checksum has passed. A mismatch or non-zero exit is rejected like a failed verification.

### Replacement safety

**R9. Atomic replacement.**
- At no point is the running binary path missing or partially written. Another `agr` process started during the update runs either the old or the new binary, never a broken one.
- The new file is staged on the same filesystem as the target.

**R10. Rollback on failure.**
- If any step after staging fails (permission, disk full, interrupted, rename error), the original binary is intact and runnable, staged files are removed, and the command exits non-zero with the cause.
- Ctrl-C during download or staging leaves the original untouched and cleans up temp files.
- File permissions of the original (mode, executable bit) are carried over to the new binary.

**R11. Binary not writable.**
- If the binary or its directory is read-only or owned by another user (for example `/usr/local/bin` without write access), the command fails before downloading, says which path is not writable, and tells the user to re-run with the right permissions or install elsewhere. It does not invoke sudo itself.

**R12. Binary installed by a package manager or cargo.**
- If the running binary is under `~/.cargo/bin` (cargo install) or another recognised managed location (for example Homebrew prefix), `agr update` refuses and does not overwrite it.
- The message names the right command: `cargo install agr` for cargo, `brew upgrade` for Homebrew. Exit non-zero, nothing changed.
- Detection must work when `agr` is run through a symlink. The real path is what gets checked, and the message names it.

**R13. Concurrency.**
- Two `agr update` runs at the same time must not corrupt the binary. The second one fails or waits, with a clear message.
- Updating while a recording session of `agr` is active must not interrupt that session. The running process keeps working, the new version applies to the next start.

### Network and privacy

**R14. No network call outside `agr update`.**
- No other command, startup, shell hook, or background process contacts the network for version information.
- Acceptance: with the update feature compiled in, every other command runs identically with networking blocked, and a test or code review confirms no call site outside the update module.
- `agr update --check` is the only version check. No cache file, no timer, no "update available" hint printed by other commands.

**R15. Network failure handling.**
- Offline, DNS failure, timeout, HTTP error, rate limit (GitHub API) and a malformed response each produce a one-line cause and non-zero exit, with nothing changed on disk.
- Requests have a bounded timeout. Download size is bounded or checked against the advertised size.
- Only HTTPS is used. Redirects may only go to GitHub hosts.
- Network access shells out to `curl`. No new HTTP dependency is added to `Cargo.toml`. `curl` becomes a prerequisite of `agr update`, and a missing `curl` fails with a one-line message saying so.
- The command never prints or logs tokens. If a GitHub token is used to avoid rate limits, it is optional and never required.

### Output and docs

**R16. Messages.**
- Success prints old and new version. Every failure names what failed and what the user can do, one line first, detail after.
- Respects existing colour and `NO_COLOR` handling.

**R17. Docs.**
- README and `agr --help` describe `update`, `--check`, its exit codes, supported platforms, the `curl` prerequisite, the Windows limitation and the manual fallback.
- `AGR_UPDATE_RELEASE_BASE` is not documented (see R3).
- Release workflow change is documented where the release process is described.

**R18. Testing.**
- Pure logic (version compare, platform mapping, install-location detection, verification) is unit tested without network.
- Replacement and rollback are tested against a temp directory, including the read-only, failure-midway and symlink cases.
- Download is tested against a local fake server or fixture through `AGR_UPDATE_RELEASE_BASE`, not the real GitHub.

**R19. Repository metadata.**
- `Cargo.toml` `repository` is corrected to `https://github.com/thiscantbeserious/agent-session-recorder`.
- No other place in the repo that names the release source points at the old `simon/agent-session-recorder` path.

## Acceptance Criteria (summary)

- [ ] R1 up-to-date, newer and dev-build cases behave as stated
- [ ] R2 `--check` performs no download and no write, exit codes 0, 10, 1 as stated
- [ ] R4 Windows and unsupported platforms fail before any download with a clear message
- [ ] R5 tampered, truncated and unverifiable downloads leave the binary untouched
- [ ] R6 release workflow publishes a SHA-256 checksum file, tampered asset rejected in a test
- [ ] R7 `--check` on a release without checksums says it cannot be applied automatically
- [ ] R8 new binary is smoke-run with `--version` before replacement
- [ ] R9/R10 replacement is atomic, every injected failure leaves the original runnable
- [ ] R11/R12 read-only binaries refused, cargo- and brew-installed binaries refused with the right command
- [ ] R14 no network call outside the update module
- [ ] R15 no new HTTP dependency in `Cargo.toml`
- [ ] R19 `Cargo.toml` `repository` corrected

## Constraints

- Mechanism (replacement technique, `curl` invocation details) is the Architect's decision. The user-facing guarantees above are fixed.
- No Windows asset exists, adding one is a separate feature.
- First usable release is the first one that ships the checksum file (R6), so end-to-end success cannot be shown against 0.3.0.

## Decisions

All seven open decisions were approved by the user.

1. **R2 exit code**: `--check` exits 10 when an update is available, 0 when up to date, 1 on failure.
2. **R3 release source**: no user-facing override. Hidden `AGR_UPDATE_RELEASE_BASE` for tests only, undocumented in help.
3. **R6 verification**: SHA-256 checksum file published by `release.yml` and verified before install. Catches corruption, not a compromised release. Signatures possible later.
4. **R7 `--check` wording**: also says the newer release cannot be applied automatically when it has no checksums (every release up to 0.3.0).
5. **R8 smoke run**: the new binary is run with `--version` before replacement.
6. **R12 managed installs**: cargo- and brew-installed binaries are refused, the message names the right command.
7. **R15 HTTP**: shell out to `curl`, no new HTTP dependency.
8. **Release target**: the updater ships in release 0.4.0, the first release with checksums. The minimum updatable release is 0.4.0, older releases are refused with the R7 message. The version bump to 0.4.0 in `Cargo.toml` and `Cargo.lock` is the last commit of the PR. No tag, the user releases after merge.

---
**Sign-off**: approved by the user 2026-11-13
