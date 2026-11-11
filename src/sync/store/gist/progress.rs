//! Percent of the `git push` and `git clone` progress lines, pure text parsing.

/// Percent of a `git push --progress` line like `Writing objects:  45% (9/20), 12.00 MiB | 3.00 MiB/s`.
pub(crate) fn writing_percent(line: &str) -> Option<u8> {
    percent_after(line, "Writing objects:")
}

/// Percent of a `git clone --progress` line like `Receiving objects:  45% (9/20), 12.00 MiB | 3.00 MiB/s`.
pub(crate) fn receiving_percent(line: &str) -> Option<u8> {
    percent_after(line, "Receiving objects:")
}

fn percent_after(line: &str, label: &str) -> Option<u8> {
    let rest = line.strip_prefix(label)?.trim_start();
    let (digits, _) = rest.split_once('%')?;
    digits.parse::<u8>().ok().filter(|p| *p <= 100)
}

#[cfg(test)]
mod tests {
    use super::{receiving_percent, writing_percent};

    /// Lines as `git push --progress` prints them, `\r` separated in real life.
    const PUSH_OUTPUT: [(&str, Option<u8>); 9] = [
        ("Enumerating objects: 5, done.", None),
        ("Counting objects: 100% (5/5), done.", None),
        ("Delta compression using up to 8 threads", None),
        ("Compressing objects: 100% (3/3), done.", None),
        ("Writing objects:   0% (0/3)", Some(0)),
        (
            "Writing objects:  45% (1/3), 12.00 MiB | 3.00 MiB/s",
            Some(45),
        ),
        (
            "Writing objects:  99% (3/3), 44.90 MiB | 3.10 MiB/s",
            Some(99),
        ),
        (
            "Writing objects: 100% (3/3), 45.00 MiB | 3.00 MiB/s, done.",
            Some(100),
        ),
        ("Total 3 (delta 0), reused 0 (delta 0), pack-reused 0", None),
    ];

    #[test]
    fn writing_percent_reads_real_git_push_output() {
        for (line, want) in PUSH_OUTPUT {
            assert_eq!(writing_percent(line), want, "{line}");
        }
    }

    #[test]
    fn writing_percent_ignores_other_lines_and_nonsense() {
        for line in [
            "",
            "Writing objects:",
            "Writing objects: abc% (1/3)",
            "Writing objects: 101% (3/3)",
            "Receiving objects:  45% (9/20)",
            "remote: Writing objects: 45%",
        ] {
            assert_eq!(writing_percent(line), None, "{line}");
        }
    }

    #[test]
    fn receiving_percent_reads_real_git_clone_output() {
        let want = [
            ("Cloning into 'clone'...", None),
            ("remote: Enumerating objects: 20, done.", None),
            ("Receiving objects:   0% (1/20)", Some(0)),
            (
                "Receiving objects:  45% (9/20), 12.00 MiB | 3.00 MiB/s",
                Some(45),
            ),
            (
                "Receiving objects: 100% (20/20), 45.00 MiB | 3.00 MiB/s, done.",
                Some(100),
            ),
            ("Resolving deltas: 100% (2/2), done.", None),
            ("Writing objects: 45% (9/20)", None),
        ];
        for (line, pct) in want {
            assert_eq!(receiving_percent(line), pct, "{line}");
        }
    }
}
