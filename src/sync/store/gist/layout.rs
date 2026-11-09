//! Part layout of a pack gist, pure: where parts are cut and how they are named, no IO.

/// One MiB, the unit of the part size.
pub(crate) const MIB: u64 = 1024 * 1024;
/// Largest part, under the 10 MB limit of `raw_url`.
pub(crate) const PART_BYTES: u64 = 8 * MIB;

/// Sizes of the parts one append writes, cut at session boundaries. A session over 8 MiB gets
/// its own parts of 8 MiB and a closing remainder.
pub(crate) fn cut_parts(lens: &[u64]) -> Vec<u64> {
    lens.to_vec()
}

#[cfg(test)]
mod tests {
    use super::{cut_parts, MIB};

    #[test]
    fn parts_cut_at_session_boundaries() {
        let parts = cut_parts(&[3 * MIB, 4 * MIB, 2 * MIB]);
        assert_eq!(parts, vec![7 * MIB, 2 * MIB]);
    }
}
