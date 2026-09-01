//! Integrity primitives shared by project metadata and streamed media
//! transfers.
//!
//! The SHA-1 implementation comes from the `sha1` crate, which uses
//! hardware SHA extensions when available: hashing multi-gigabyte media
//! assets at save time no longer dominates the write path.

use sha1::Digest as _;

/// Incremental SHA-1 digest.
///
/// SHA-1 is used here for deterministic corruption detection and prefix
/// identity, never for authentication or password storage.
#[derive(Clone)]
pub(crate) struct Sha1 {
    inner: sha1::Sha1,
}

impl Sha1 {
    pub(crate) fn new() -> Self {
        Self {
            inner: sha1::Sha1::new(),
        }
    }

    pub(crate) fn update(&mut self, input: &[u8]) {
        self.inner.update(input);
    }

    pub(crate) fn finalize(self) -> [u8; 20] {
        self.inner.finalize().into()
    }

    pub(crate) fn finalize_hex(self) -> String {
        digest_to_hex(self.finalize())
    }
}

impl Default for Sha1 {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn sha1_bytes(input: &[u8]) -> [u8; 20] {
    let mut digest = Sha1::new();
    digest.update(input);
    digest.finalize()
}

pub(crate) fn digest_to_hex(digest: [u8; 20]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(40);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_matches_standard_test_vectors() {
        assert_eq!(
            digest_to_hex(sha1_bytes(b"")),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        assert_eq!(
            digest_to_hex(sha1_bytes(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            digest_to_hex(sha1_bytes(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    #[test]
    fn incremental_updates_match_one_shot_digest_across_block_boundaries() {
        let input: Vec<u8> = (0..257).map(|index| (index % 251) as u8).collect();
        let expected = sha1_bytes(&input);

        let mut digest = Sha1::new();
        for chunk in input.chunks(7) {
            digest.update(chunk);
        }
        assert_eq!(digest.finalize(), expected);
    }
}
