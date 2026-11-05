# agr status

Show storage statistics

## Usage

```
agr status [OPTIONS]
```

## Description

Display storage statistics for recorded sessions.

Shows the size on disk, the uncompressed size, disk usage percentage,
session count by agent and age of the oldest recording. When plain .cast
sessions are left, a last line shows what `agr compress` would save.

EXAMPLE:
    agr status

OUTPUT:
    Agent Sessions: 1.2 GiB on disk, 6.8 GiB uncompressed (0.50% of disk)
       Sessions: 23 total (claude: 15, codex: 8)
       Oldest: 2025-01-01 (20 days ago)
    3 plain, ~410.00 MiB saved by agr compress

