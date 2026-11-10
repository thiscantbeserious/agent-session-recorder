//! Part layout of a pack gist, pure: where parts are cut and how they are named, no IO.

/// One MiB, the unit of the part size.
pub(crate) const MIB: u64 = 1024 * 1024;
/// Largest part, under the 10 MB limit of `raw_url`.
pub(crate) const PART_BYTES: u64 = 8 * MIB;

/// Sizes of the parts one append writes, cut at session boundaries. A session over 8 MiB gets
/// its own parts of 8 MiB and a closing remainder.
pub(crate) fn cut_parts(lens: &[u64]) -> Vec<u64> {
    let (mut parts, mut open) = (Vec::new(), 0);
    for &len in lens {
        if open > 0 && open + len > PART_BYTES {
            parts.push(std::mem::take(&mut open));
        }
        if len > PART_BYTES {
            let (full, rest) = (len / PART_BYTES, len % PART_BYTES);
            parts.extend(std::iter::repeat_n(PART_BYTES, full as usize));
            open = rest;
        } else {
            open += len;
        }
    }
    if open > 0 {
        parts.push(open);
    }
    parts
}

/// Most files a pack gist may hold, the index included, under the 300 file limit of the listing.
pub(crate) const MAX_FILES: usize = 250;

fn part_name(n: usize) -> String {
    format!("data.{n:03}")
}

/// `existing` must be exactly `data.000` up to `data.N`, no gap and no reorder, none is fine.
pub(crate) fn check_parts(existing: &[&str]) -> Result<(), String> {
    match existing
        .iter()
        .enumerate()
        .find(|(i, n)| **n != part_name(*i))
    {
        Some((i, n)) => Err(format!("expected {} but found {n}", part_name(i))),
        None => Ok(()),
    }
}

/// Names for the parts of the next append, numbered on after the existing ones.
pub(crate) fn next_names(existing: &[&str], count: usize) -> Result<Vec<String>, String> {
    check_parts(existing)?;
    Ok((existing.len()..existing.len() + count)
        .map(part_name)
        .collect())
}

/// Whether an append of sessions with these `lens` fits a gist that holds `files` files now.
pub(crate) fn fits(files: usize, lens: &[u64]) -> bool {
    files + cut_parts(lens).len() <= MAX_FILES
}

#[cfg(test)]
mod tests {
    use super::{check_parts, cut_parts, fits, next_names, MIB};

    #[test]
    fn parts_cut_at_session_boundaries() {
        let parts = cut_parts(&[3 * MIB, 4 * MIB, 2 * MIB]);
        assert_eq!(parts, vec![7 * MIB, 2 * MIB]);
    }

    #[test]
    fn session_over_8_mib_gets_own_parts() {
        assert_eq!(cut_parts(&[20 * MIB]), vec![8 * MIB, 8 * MIB, 4 * MIB]);
    }

    #[test]
    fn append_numbers_continue_after_last_part() {
        let names = next_names(&["data.000", "data.001", "data.002"], 2).unwrap();
        assert_eq!(names[0], "data.003");
        assert_eq!(names[1], "data.004");
    }

    #[test]
    fn gap_or_order_in_parts_is_an_error() {
        assert!(check_parts(&["data.000", "data.002"]).is_err());
        assert!(check_parts(&["data.001", "data.000"]).is_err());
        assert!(check_parts(&["data.000", "data.001"]).is_ok());
        assert!(check_parts(&[]).is_ok());
    }

    #[test]
    fn fits_refuses_over_250_files() {
        // 249 files hold the index and 248 parts, a 9 MiB session needs two more.
        assert!(!fits(249, &[9 * MIB]));
        assert!(fits(249, &[MIB]));
    }

    #[test]
    fn fits_false_when_no_file_slot_left() {
        assert!(!fits(250, &[1]));
        assert!(fits(250, &[]));
    }
}
