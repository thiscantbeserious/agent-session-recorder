//! Recovery phrase export and import, with its checksum.

use super::{Key, KEY_LEN};
use data_encoding::BASE32_NOPAD;
use sha2::{Digest, Sha256};
use thiserror::Error;

const CHECKSUM_LEN: usize = 2;
const PHRASE_PREFIX: &str = "agr1";
const PHRASE_BODY_LEN: usize = 55;

/// Errors from importing a recovery phrase.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PhraseError {
    #[error("recovery phrase must start with agr1-")]
    Prefix,
    #[error("recovery phrase has the wrong length")]
    Length,
    #[error("recovery phrase has invalid characters")]
    Encoding,
    #[error("recovery phrase checksum does not match, check for typos")]
    Checksum,
}

fn checksum(key: &[u8]) -> [u8; CHECKSUM_LEN] {
    let digest = Sha256::digest(key);
    [digest[0], digest[1]]
}

/// `agr1-` plus the base32 of `key || sha256(key)[..2]` in groups of five.
pub(in crate::sync) fn export_phrase(key: &Key) -> String {
    let mut raw = key.0.to_vec();
    raw.extend_from_slice(&checksum(&key.0));
    let encoded = BASE32_NOPAD.encode(&raw);
    let groups: Vec<&str> = encoded
        .as_bytes()
        .chunks(5)
        .map(|c| std::str::from_utf8(c).expect("base32 is ascii"))
        .collect();
    format!("{PHRASE_PREFIX}-{}", groups.join("-"))
}

pub(in crate::sync) fn import_phrase(phrase: &str) -> Result<Key, PhraseError> {
    let clean: String = phrase
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_uppercase();
    let body = clean
        .strip_prefix(&PHRASE_PREFIX.to_uppercase())
        .ok_or(PhraseError::Prefix)?;
    if body.len() != PHRASE_BODY_LEN {
        return Err(PhraseError::Length);
    }
    let raw = BASE32_NOPAD
        .decode(body.as_bytes())
        .map_err(|_| PhraseError::Encoding)?;
    let (key, sum) = raw.split_at(KEY_LEN);
    if checksum(key) != sum {
        return Err(PhraseError::Checksum);
    }
    Ok(Key(key.try_into().expect("split at KEY_LEN")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(b: u8) -> Key {
        Key::from_bytes([b; 32])
    }

    #[test]
    fn phrase_round_trips_with_prefix_and_groups() {
        let k = key(7);
        let phrase = export_phrase(&k);
        assert!(phrase.starts_with("agr1-"));
        assert_eq!(phrase.len(), 5 + 55 + 10);
        assert_eq!(import_phrase(&phrase).unwrap().as_bytes(), k.as_bytes());
    }

    #[test]
    fn phrase_import_is_lenient_about_format() {
        let k = key(9);
        let phrase = export_phrase(&k);
        let messy = format!("  {} ", phrase.to_lowercase().replace('-', " "));
        assert_eq!(import_phrase(&messy).unwrap().as_bytes(), k.as_bytes());
        let nodash = phrase.replace('-', "");
        assert_eq!(import_phrase(&nodash).unwrap().as_bytes(), k.as_bytes());
    }

    #[test]
    fn phrase_rejects_wrong_prefix_and_length() {
        let phrase = export_phrase(&key(3));
        let bad_prefix = phrase.replacen("agr1", "agr2", 1);
        assert_eq!(import_phrase(&bad_prefix).err(), Some(PhraseError::Prefix));
        assert_eq!(
            import_phrase(&phrase[..phrase.len() - 6]).err(),
            Some(PhraseError::Length)
        );
        assert_eq!(import_phrase("agr1-").err(), Some(PhraseError::Length));
    }

    #[test]
    fn phrase_rejects_any_single_changed_character() {
        let phrase = export_phrase(&key(5));
        let body_start = 5;
        assert_eq!(phrase.len(), 70);
        for i in (body_start..phrase.len()).filter(|&i| phrase.as_bytes()[i] != b'-') {
            let mut bytes = phrase.clone().into_bytes();
            bytes[i] = if bytes[i] == b'A' { b'B' } else { b'A' };
            let changed = String::from_utf8(bytes).unwrap();
            assert!(import_phrase(&changed).is_err(), "change at {i} accepted");
        }
    }

    #[test]
    fn phrase_rejects_checksum_mismatch() {
        let phrase = export_phrase(&key(5));
        let mut bytes = phrase.into_bytes();
        let i = 11;
        bytes[i] = if bytes[i] == b'A' { b'B' } else { b'A' };
        let changed = String::from_utf8(bytes).unwrap();
        assert_eq!(import_phrase(&changed).err(), Some(PhraseError::Checksum));
    }
}
