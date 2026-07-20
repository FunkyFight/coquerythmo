//! Small, dependency-free integrity primitives shared by project metadata and
//! streamed media transfers.

/// Incremental SHA-1 digest.
///
/// SHA-1 is used here for deterministic corruption detection and prefix
/// identity, never for authentication or password storage.
#[derive(Clone)]
pub(crate) struct Sha1 {
    state: [u32; 5],
    total_bytes: u64,
    buffer: [u8; 64],
    buffer_len: usize,
}

impl Sha1 {
    pub(crate) fn new() -> Self {
        Self {
            state: [
                0x6745_2301,
                0xefcd_ab89,
                0x98ba_dcfe,
                0x1032_5476,
                0xc3d2_e1f0,
            ],
            total_bytes: 0,
            buffer: [0; 64],
            buffer_len: 0,
        }
    }

    pub(crate) fn update(&mut self, mut input: &[u8]) {
        self.total_bytes = self.total_bytes.wrapping_add(input.len() as u64);

        if self.buffer_len != 0 {
            let copied = (64 - self.buffer_len).min(input.len());
            self.buffer[self.buffer_len..self.buffer_len + copied]
                .copy_from_slice(&input[..copied]);
            self.buffer_len += copied;
            input = &input[copied..];
            if self.buffer_len < 64 {
                return;
            }
            let block = self.buffer;
            self.process_block(&block);
            self.buffer_len = 0;
        }

        let mut blocks = input.chunks_exact(64);
        for block in &mut blocks {
            let block: &[u8; 64] = block
                .try_into()
                .expect("chunks_exact always returns complete SHA-1 blocks");
            self.process_block(block);
        }
        let remainder = blocks.remainder();
        self.buffer[..remainder.len()].copy_from_slice(remainder);
        self.buffer_len = remainder.len();
    }

    pub(crate) fn finalize(mut self) -> [u8; 20] {
        let bit_len = self.total_bytes.wrapping_mul(8);
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;

        if self.buffer_len > 56 {
            self.buffer[self.buffer_len..].fill(0);
            let block = self.buffer;
            self.process_block(&block);
            self.buffer = [0; 64];
        } else {
            self.buffer[self.buffer_len..56].fill(0);
        }
        self.buffer[56..].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buffer;
        self.process_block(&block);

        let mut digest = [0_u8; 20];
        for (output, word) in digest.chunks_exact_mut(4).zip(self.state) {
            output.copy_from_slice(&word.to_be_bytes());
        }
        digest
    }

    pub(crate) fn finalize_hex(self) -> String {
        digest_to_hex(self.finalize())
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        let mut words = [0_u32; 80];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            let start = index * 4;
            *word = u32::from_be_bytes([
                block[start],
                block[start + 1],
                block[start + 2],
                block[start + 3],
            ]);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }

        let [mut a, mut b, mut c, mut d, mut e] = self.state;
        for (index, word) in words.iter().enumerate() {
            let (function, constant) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let temporary = a
                .rotate_left(5)
                .wrapping_add(function)
                .wrapping_add(e)
                .wrapping_add(constant)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temporary;
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
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
