//! Pure split and join of the sealed blob into gist part files, no process and no disk.

/// Largest part, under git's 50 MiB warning.
pub const PART_SIZE: usize = 45 * 1024 * 1024;
/// Prefix of every part file, the name is `data.NNN`.
pub const PART_PREFIX: &str = "data.";

pub fn part_name(index: usize) -> String {
    format!("{PART_PREFIX}{index:03}")
}

/// Cut `blob` into parts of at most `size` bytes, an empty blob is one empty part.
pub fn split(blob: &[u8], size: usize) -> Vec<&[u8]> {
    if blob.is_empty() {
        return vec![blob];
    }
    blob.chunks(size).collect()
}

pub fn join(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.concat()
}

/// `names` must be sorted and be exactly `part_name(0..n)`, anything else is an error.
pub fn check_sequence(names: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Err("has no data parts".into());
    }
    for (i, name) in names.iter().enumerate() {
        if *name != part_name(i) {
            return Err(format!("expected {} but found {name}", part_name(i)));
        }
    }
    Ok(())
}

/// Names of existing parts that a push of `count` parts leaves behind.
pub fn stale(existing: &[String], count: usize) -> Vec<String> {
    let keep: Vec<String> = (0..count).map(part_name).collect();
    existing
        .iter()
        .filter(|n| n.starts_with(PART_PREFIX) && !keep.contains(n))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    fn lens(blob: &[u8], size: usize) -> Vec<usize> {
        split(blob, size).iter().map(|p| p.len()).collect()
    }

    fn names(n: usize) -> Vec<String> {
        (0..n).map(part_name).collect()
    }

    #[test]
    fn names_are_zero_padded() {
        assert_eq!(part_name(0), "data.000");
        assert_eq!(part_name(12), "data.012");
    }

    #[test]
    fn split_sizes_small_parts() {
        assert_eq!(lens(&data(0), 10), [0]);
        assert_eq!(lens(&data(3), 10), [3]);
        assert_eq!(lens(&data(10), 10), [10]);
        assert_eq!(lens(&data(11), 10), [10, 1]);
        assert_eq!(lens(&data(25), 10), [10, 10, 5]);
    }

    #[test]
    fn split_at_the_real_part_size_boundary() {
        let exact = vec![7u8; PART_SIZE];
        assert_eq!(lens(&exact, PART_SIZE), [PART_SIZE]);
        let over = vec![7u8; PART_SIZE + 1];
        assert_eq!(lens(&over, PART_SIZE), [PART_SIZE, 1]);
    }

    #[test]
    fn join_of_split_is_identity() {
        for len in [0, 1, 9, 10, 11, 95] {
            let blob = data(len);
            let parts: Vec<Vec<u8>> = split(&blob, 10).iter().map(|p| p.to_vec()).collect();
            assert_eq!(join(&parts), blob, "len {len}");
        }
    }

    #[test]
    fn sequence_accepts_contiguous_from_zero() {
        assert_eq!(check_sequence(&names(1)), Ok(()));
        assert_eq!(check_sequence(&names(5)), Ok(()));
    }

    #[test]
    fn sequence_rejects_empty_gap_extra_and_out_of_order() {
        assert!(check_sequence(&[]).is_err());
        let mut gap = names(3);
        gap.remove(1);
        assert!(check_sequence(&gap).is_err());
        let mut no_start = names(3);
        no_start.remove(0);
        assert!(check_sequence(&no_start).is_err());
        let mut extra = names(2);
        extra.push("data.junk".into());
        assert!(check_sequence(&extra).is_err());
        let mut shuffled = names(3);
        shuffled.swap(0, 2);
        assert!(check_sequence(&shuffled).is_err());
    }

    #[test]
    fn stale_are_the_parts_at_or_above_the_new_count() {
        assert_eq!(stale(&names(4), 2), [part_name(2), part_name(3)]);
        assert!(stale(&names(2), 2).is_empty());
        assert!(stale(&names(2), 5).is_empty());
    }
}
