# Plan: agr update, self-update from GitHub releases

References: ADR.md, REQUIREMENTS.md

Every behaviour stage is TDD: RED tests first and failing for the right reason, then GREEN with the minimal code, then `cargo fmt` and `cargo clippy --all-targets -- -D warnings` and `cargo test`. Tests live only in `tests/integration/` and are registered in `tests/integration.rs`, never in `src/`. No test touches the real network, downloads use `AGR_UPDATE_RELEASE_BASE=file://<tempdir>`. No new dependency in `Cargo.toml`. Functions about 20 lines, files under about 400 lines. Library code never prints, `src/commands/update.rs` does.

Size budget: about 25 reviewed files (code, tests, workflow, Cargo files, generated man and wiki pages), well under 100. `.md` and `.state` do not count.

## Open Questions

1. Magic byte values, curl `User-Agent` acceptance and the `objects.githubusercontent.com` host are UNVERIFIED in the ADR. Stage 2 tests pin the magic bytes against the built `agr` binary itself, the rest is checked in the manual run after 0.4.0 ships.

## Stages

### Stage 1: Release workflow publishes SHA256SUMS

Goal: every release from 0.4.0 on carries `SHA256SUMS` next to the three binaries (ADR 5.1).
Owner: implementer
Depends on: none
Files: `.github/workflows/release.yml`, `tests/integration/release_workflow_test.rs`, `tests/integration.rs`

RED:
1. `release_assets_step_writes_sha256sums`: the "Prepare release assets" step text contains `sha256sum agr-* > SHA256SUMS`, run inside `release-assets`.
2. `release_upload_covers_sha256sums`: the `files:` of "Create GitHub Release" is still `release-assets/*`.
3. `release_matrix_matches_update_assets`: the three `artifact:` values equal the asset names of `agr::update::platform` without the `agr-` prefix (fails until Stage 2, kept `#[ignore]` with reason until then, un-ignored in Stage 2).

GREEN:
- [ ] Add `(cd release-assets && sha256sum agr-* > SHA256SUMS && cat SHA256SUMS)` after the rename loop.
- [ ] Tests 1 and 2 pass.

### Stage 2: Update module (check, verify, smoke run, replace with rollback)

Goal: `src/update/` per ADR 9 with every pure function and the full `check()` and `run()` flow, no CLI yet.
Owner: implementer
Depends on: Stage 1
Files: `src/update/mod.rs`, `src/update/version.rs`, `src/update/platform.rs`, `src/update/release.rs`, `src/update/fetch.rs`, `src/update/install.rs`, `src/lib.rs`, `tests/integration/helpers/update_fake.rs`, `tests/integration/helpers/mod.rs`, `tests/integration/update_version_test.rs`, `tests/integration/update_platform_test.rs`, `tests/integration/update_release_test.rs`, `tests/integration/update_fetch_test.rs`, `tests/integration/update_install_test.rs`, `tests/integration/release_workflow_test.rs`, `tests/integration.rs`

RED, version (`update_version_test.rs`):
1. `parse_accepts_three_numbers`: `parse("0.4.0")` is `(0,4,0)`, `parse("v0.4.0")` is `(0,4,0)`.
2. `parse_rejects_garbage`: `"0.4"`, `"a.b.c"`, `""` are errors.
3. `compare_orders_numerically`: `0.10.0 > 0.9.9`, `1.0.0 > 0.99.0`, equal is equal.
4. `version_output_dev_and_release`: `agr 0.4.0-dev+abc1234 (x, built y)` and `agr 0.4.0 (x, built y)` both give `0.4.0`.
5. `version_output_rejects_other_program`: `curl 8.4.0` and empty output are errors.

RED, platform (`update_platform_test.rs`):
6. `maps_supported_targets`: `("macos","aarch64")` gives `agr-macos-arm64`, `("macos","x86_64")` gives `agr-macos-x86_64`, `("linux","x86_64")` gives `agr-linux-x86_64`.
7. `windows_is_refused_without_retry`: error text contains `not supported on Windows`, `no Windows release`, the releases URL, and not `retry` or `try again`.
8. `unsupported_names_detected_and_supported`: `("linux","aarch64")` error contains `linux-aarch64` and all three supported names.
9. `release_matrix_matches_update_assets` from Stage 1 is un-ignored and passes.

RED, release (`update_release_test.rs`):
10. `latest_json_parses_tag_and_assets`: fixture JSON gives tag `v0.5.0`, asset name, size and URL.
11. `draft_or_prerelease_is_rejected`: `draft: true` or `prerelease: true` is an error.
12. `asset_url_outside_prefix_is_malformed`: `browser_download_url` on another repo is an error naming the URL.
13. `sha256sums_accepts_text_and_binary_flag`: lines `<hex>  agr-linux-x86_64` and `<hex> *agr-linux-x86_64` both resolve.
14. `sha256sums_rejects_bad_lines`: uppercase hex, 63 chars, duplicate name, missing name are errors.
15. `gate_refuses_below_minimum`: latest `0.3.0` with `SHA256SUMS` present gives the R7 error, text contains `no checksums` and the releases URL.
16. `gate_refuses_missing_checksum_file`: latest `0.5.0` without `SHA256SUMS` gives the same R7 error.
17. `gate_passes_from_minimum`: latest `0.4.0` with `SHA256SUMS` is accepted.
18. `missing_platform_asset_names_asset_and_version`: error contains `agr-linux-x86_64` and `v0.5.0`.

RED, fetch (`update_fetch_test.rs`):
19. `host_allowlist`: `api.github.com`, `github.com`, `release-assets.githubusercontent.com`, `objects.githubusercontent.com` allowed, `evil.com`, `github.com.evil.com`, `user@github.com`, `http://github.com/x` refused.
20. `classify_curl_exit_codes`: 6 gives DNS, 7 gives connect, 28 gives timeout, 63 gives too large, each one line.
21. `classify_http_status`: 403 and 429 give rate limit, 404 gives HTTP error with `404`.
22. `redirect_limit`: a fake chain of 4 hops is refused.
23. `missing_curl_says_so`: with `PATH` set to an empty temp dir the error is `agr update needs curl, install it and retry`.
24. `curl_args_are_bounded`: the built argument list for the binary download contains `--proto =https`, `--connect-timeout`, `--max-time 300`, `--max-filesize 64M`, and no `--location` and no `Authorization`.
25. `override_rejects_http`: `AGR_UPDATE_RELEASE_BASE=http://x` is an error.
26. `file_override_fetches_fixture`: with the fake release from `update_fake.rs`, `check()` returns latest `0.5.0`.

RED, install (`update_install_test.rs`, all `#[cfg(unix)]`):
27. `managed_cargo`: `<home>/.cargo/bin/agr` and `<CARGO_HOME>/bin/agr` give cargo, message contains `cargo install agr` and the path.
28. `managed_brew`: `/opt/homebrew/Cellar/agr/0.4.0/bin/agr` gives Homebrew with `brew upgrade`.
29. `managed_through_symlink`: a symlink in a temp dir pointing into a fake `.cargo/bin` resolves to the real path, and the message names the real path.
30. `readonly_dir_refused_before_download`: binary in a 0555 temp dir fails with the dir path and no request is made (fake release has a counter file the fetch layer would touch).
31. `staged_file_never_at_final_path_before_rename`: a fake release with a wrong checksum leaves the target byte identical and no `.agr.update-*` file in the dir.
32. `truncated_download_rejected`: asset shorter than the JSON `size` fails, target unchanged.
33. `tampered_asset_rejected`: one flipped byte fails with `checksum mismatch`, target unchanged (R6 acceptance).
34. `wrong_magic_rejected`: a shell script with the right checksum fails as not an executable for this platform.
35. `smoke_run_version_mismatch_rejected`: stub prints `agr 0.4.9` for release `v0.5.0`, fails, target unchanged.
36. `smoke_run_nonzero_rejected`: stub exits 3, fails, target unchanged.
37. `success_replaces_and_keeps_mode`: target with mode 0750 ends up as the stub, mode 0750, `--version` prints `0.5.0`.
38. `rename_failure_leaves_original`: target dir made read-only after staging through a test hook, rename fails, original runs, staged file removed.
39. `second_update_fails_while_locked`: a held `flock` on the target makes `run()` fail with `another agr update is running` and the path.
40. `old_inode_keeps_running`: a process started from the old binary before the rename finishes normally after the update.

GREEN:
- [ ] `version.rs`, `platform.rs`, `release.rs` pure functions (1 to 18).
- [ ] `fetch.rs` curl builder, redirect loop, allowlist, classifier, override (19 to 26).
- [ ] `install.rs` discovery, managed, writable, lock, staging guard, checks, rename (27 to 40).
- [ ] `mod.rs` `check()` and `run()` wire them in the order of ADR 2. `pub mod update` in `src/lib.rs`.

### Stage 3: CLI wiring, exit codes and help

Goal: `agr update` and `agr update --check` reachable, exit codes of ADR 3.2, help text.
Owner: implementer
Depends on: Stage 2
Files: `src/cli.rs`, `src/dispatch.rs`, `src/commands/mod.rs`, `src/commands/update.rs`, `tests/integration/update_cli_test.rs`, `tests/integration/update_boundary_test.rs`, `tests/integration/snapshots/integration__update_cli_test__cli_help_update.snap`, `tests/integration.rs`

RED (`update_cli_test.rs`, copy of `CARGO_BIN_EXE_agr` in a temp dir, fake release via override):
1. `check_update_available_exits_10`: latest `0.5.0`, exit 10, stdout has current, latest and `update available`, temp dir listing unchanged.
2. `check_up_to_date_exits_0`: latest equals `CARGO_PKG_VERSION`, exit 0, `up to date`.
3. `check_dev_build_newer_exits_0`: latest `0.0.1`, exit 0, says running build is newer.
4. `check_without_checksums_notes_it`: latest `0.3.0`, exit 10, stdout says it cannot be applied automatically because it has no checksums.
5. `check_failure_exits_1`: override points at an empty dir, exit 1, one-line cause first.
6. `update_up_to_date_changes_nothing`: exit 0, binary bytes and mtime unchanged.
7. `update_success_prints_old_and_new`: exit 0, stdout has both versions, copy now prints `0.5.0`.
8. `update_refuses_old_release`: latest `0.3.0`, exit 1, R7 message, copy unchanged.
9. `update_never_downloads_on_check`: fake asset path is a directory, `--check` still exits 10 (proves no asset read).
10. `cli_help_update`: snapshot of `agr update --help` lists `--check`, exit codes 0, 10, 1, `curl`, Windows limitation, and does not contain `AGR_UPDATE_RELEASE_BASE`.
11. `main_help_lists_update`: `agr --help` contains `update`.

RED (`update_boundary_test.rs`):
12. `no_network_outside_update_module`: every `.rs` under `src/` outside `src/update/` contains neither `"curl"` nor `AGR_UPDATE_RELEASE_BASE`.
13. `override_not_documented`: `README.md`, `docs/` and help output do not contain `AGR_UPDATE_RELEASE_BASE`.

GREEN:
- [ ] `Commands::Update { check: bool }` with help text, dispatch arm, handler printing per ADR 3.3 and `std::process::exit(10)` for update available on `--check`.
- [ ] Update existing help snapshots that change because a command was added.

### Stage 4: README and docs

Goal: R17 docs, generated man and wiki pages.
Owner: implementer
Depends on: Stage 3
Files: `README.md`, `docs/COMMANDS.md`, `docs/man/agr.1`, `docs/man/agr-update.1`, `docs/wiki/Command-update.md`, `docs/wiki/Home.md`

- [ ] README "Updating" section: `agr update`, `--check`, exit codes 0, 10, 1, supported platforms, `curl` prerequisite, Windows not supported, manual fallback link, minimum updatable release 0.4.0.
- [ ] README Development section: one line that releases publish `SHA256SUMS` from `release.yml`, and that the checksum catches corruption, not a compromised release.
- [ ] `cargo xtask gen-docs`, commit the generated files.
- [ ] Stage 3 test 13 still passes.

### Stage 5: Cargo.toml repository fix (R19)

Goal: the release source in metadata is `thiscantbeserious/agent-session-recorder`.
Owner: implementer
Depends on: Stage 3
Files: `Cargo.toml`, `tests/integration/update_boundary_test.rs`

RED:
1. `cargo_repository_points_at_release_source`: `Cargo.toml` `repository` equals `https://github.com/thiscantbeserious/agent-session-recorder`.
2. `no_old_release_source_left`: `Cargo.toml`, `README.md`, `docs/`, `src/`, `.github/` contain no `github.com/simon/agent-session-recorder`.

GREEN:
- [ ] Fix line 12 of `Cargo.toml`. No version change here.

### Stage 6: Gates

Goal: everything CI runs passes locally, plus an end-to-end run.
Owner: implementer
Depends on: Stage 4, Stage 5
Files: none

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] MSRV: `cargo +1.88.0 check --all-targets --locked`
- [ ] `cargo build --release`
- [ ] e2e: copy `target/release/agr` into a temp dir, build the fake release with `update_fake.rs` data, run `--check` (exit 10) and `update` (exit 0, new version printed), then `update` again on a tampered asset (exit 1, binary unchanged).
- [ ] `cargo audit -D warnings` and `cargo deny check`, as `.github/workflows/audit.yml` runs them.
- [ ] Reviewed file count under 100.

### Stage 7: Version bump to 0.4.0

Goal: the release that first ships `SHA256SUMS` and the updater. Last commit of the PR.
Owner: implementer
Depends on: Stage 6
Files: `Cargo.toml`, `Cargo.lock`

- [ ] `version = "0.4.0"` in `Cargo.toml`, `cargo check` updates `Cargo.lock`.
- [ ] `cargo test` passes (tests that compare to `CARGO_PKG_VERSION` adapt by themselves).
- [ ] Commit as the last commit. No tag, the user releases after merge.
