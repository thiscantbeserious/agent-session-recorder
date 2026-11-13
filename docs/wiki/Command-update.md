# agr update

Update agr to the latest GitHub release

## Usage

```
agr update [OPTIONS]
```

## Options

| Option | Description |
|--------|-------------|
| `--check` | Only report whether an update is available, exit 10 if so |

## Description

Update agr to the latest GitHub release.

Downloads the release binary for your platform, verifies its SHA-256
checksum, runs it once with --version and then replaces the running binary.
Installs managed by cargo, Homebrew or a package manager are refused, update
those with their own tool.

EXAMPLES:
    agr update              Update to the latest release
    agr update --check      Only report, download nothing

EXIT CODES:
    0     Up to date (or running a newer dev build), or updated
    10    Update available (only with --check)
    1     Failure (network, checksum, permissions, unsupported platform)

REQUIREMENTS:
    curl must be installed, agr uses it for all downloads.
    Windows is not supported, install releases manually there.
    Only releases from 0.4.0 on can be applied, older ones have no checksums.

Manual install: https://github.com/thiscantbeserious/agent-session-recorder/releases

