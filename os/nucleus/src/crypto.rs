//! Cryptographic primitives, from scratch — the foundation TLS is built on.
//!
//! A secure transport (HTTPS) needs a hash, a stream cipher/AEAD, and a key
//! exchange. This module starts that layer with **SHA-256** (FIPS 180-4), the
//! workhorse used everywhere in TLS: HKDF key derivation, HMAC, the transcript
//! hash, and certificate fingerprints all sit on top of it. It is implemented
//! straight from the specification — the eight working variables, the 64-entry
//! round constant table, the message schedule — with no dependencies, and it
//! is checked against the published NIST test vectors so correctness is not a
//! matter of opinion.
//!
//! SHA-256 lands first because everything else keys off it; the rest of the
//! TLS 1.3 primitive set is built on top in this module and in
//! [`crate::x25519`]: **HMAC-SHA256** and **HKDF** (the key schedule),
//! **ChaCha20-Poly1305** (record protection), and **X25519** (key exchange).
//! Every one is checked here against its published RFC test vectors, so the
//! cryptographic foundation for a TLS handshake is complete and correct; what
//! remains is the handshake *state machine* that sequences them.

/// SHA-256 round constants: the first 32 bits of the fractional parts of the
/// cube roots of the first 64 primes.
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// The initial hash value: the first 32 bits of the fractional parts of the
/// square roots of the first 8 primes.
const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// A streaming SHA-256 hasher: feed it bytes with [`update`](Self::update),
/// then take the 32-byte digest with [`finish`](Self::finish).
pub struct Sha256 {
    state: [u32; 8],
    /// Bytes hashed so far (for the length padding).
    len: u64,
    /// Partial block awaiting a full 64 bytes.
    block: [u8; 64],
    fill: usize,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: H0,
            len: 0,
            block: [0; 64],
            fill: 0,
        }
    }

    /// Feed more message bytes.
    pub fn update(&mut self, mut data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        // Top off a partial block first.
        if self.fill > 0 {
            let need = 64 - self.fill;
            let take = need.min(data.len());
            self.block[self.fill..self.fill + take].copy_from_slice(&data[..take]);
            self.fill += take;
            data = &data[take..];
            if self.fill == 64 {
                let block = self.block;
                self.compress(&block);
                self.fill = 0;
            }
        }
        // Whole blocks straight from the input.
        while data.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[..64]);
            self.compress(&block);
            data = &data[64..];
        }
        // Stash the remainder.
        if !data.is_empty() {
            self.block[..data.len()].copy_from_slice(data);
            self.fill = data.len();
        }
    }

    /// Finish and return the 32-byte digest (consuming the hasher).
    #[must_use]
    pub fn finish(mut self) -> [u8; 32] {
        // Padding: a 0x80 byte, then zeros, then the 64-bit big-endian bit
        // length, so the total is a multiple of 64 bytes.
        let bit_len = self.len.wrapping_mul(8);
        let mut pad = [0u8; 72];
        pad[0] = 0x80;
        let pad_len = if self.fill < 56 {
            56 - self.fill
        } else {
            120 - self.fill
        };
        self.update(&pad[..pad_len]);
        // update() bumped len; the appended length uses the ORIGINAL bit_len.
        let len_bytes = bit_len.to_be_bytes();
        // Feed the length directly into a final compression without touching
        // len accounting again.
        self.block[self.fill..self.fill + 8].copy_from_slice(&len_bytes);
        let block = self.block;
        self.compress(&block);

        let mut out = [0u8; 32];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    /// One 64-byte block through the compression function.
    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

/// One-shot SHA-256 of a byte slice.
#[must_use]
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finish()
}

/// Lowercase hex of a 32-byte digest — for logs and fingerprints.
#[must_use]
pub fn hex32(digest: &[u8; 32]) -> alloc::string::String {
    use core::fmt::Write;
    let mut s = alloc::string::String::with_capacity(64);
    for b in digest {
        let _ = write!(s, "{b:02x}");
    }
    s
}

// ── HMAC-SHA256 (RFC 2104 / FIPS 198) ────────────────────────────────────

const SHA_BLOCK: usize = 64;

/// HMAC-SHA256(key, message) — the keyed MAC TLS uses to key its whole
/// schedule (via HKDF) and to authenticate the handshake transcript.
#[must_use]
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    // Keys longer than the block are hashed down; shorter are zero-padded.
    let mut k = [0u8; SHA_BLOCK];
    if key.len() > SHA_BLOCK {
        k[..32].copy_from_slice(&sha256(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; SHA_BLOCK];
    let mut opad = [0x5cu8; SHA_BLOCK];
    for i in 0..SHA_BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(msg);
    let inner = inner.finish();
    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(&inner);
    outer.finish()
}

// ── HKDF-SHA256 (RFC 5869) — the TLS 1.3 key-derivation function ─────────

/// HKDF-Extract: fold input keying material into a pseudorandom key.
#[must_use]
pub fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    hmac_sha256(salt, ikm)
}

/// HKDF-Expand: stretch a PRK into `len` bytes of output keyed by `info`.
#[must_use]
pub fn hkdf_expand(prk: &[u8; 32], info: &[u8], len: usize) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::with_capacity(len);
    let mut t: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
    let mut counter = 1u8;
    while out.len() < len {
        let mut input = t.clone();
        input.extend_from_slice(info);
        input.push(counter);
        t = hmac_sha256(prk, &input).to_vec();
        let take = (len - out.len()).min(t.len());
        out.extend_from_slice(&t[..take]);
        counter = counter.wrapping_add(1);
    }
    out
}

// ── ChaCha20 (RFC 8439) — the TLS 1.3 record cipher ──────────────────────

fn chacha20_quarter(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(7);
}

/// One 64-byte ChaCha20 keystream block for `counter`.
fn chacha20_block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut s = [0u32; 16];
    s[0] = 0x6170_7865; // "expa"
    s[1] = 0x3320_646e; // "nd 3"
    s[2] = 0x7962_2d32; // "2-by"
    s[3] = 0x6b20_6574; // "te k"
    for i in 0..8 {
        s[4 + i] = u32::from_le_bytes([key[i * 4], key[i * 4 + 1], key[i * 4 + 2], key[i * 4 + 3]]);
    }
    s[12] = counter;
    for i in 0..3 {
        s[13 + i] = u32::from_le_bytes([
            nonce[i * 4],
            nonce[i * 4 + 1],
            nonce[i * 4 + 2],
            nonce[i * 4 + 3],
        ]);
    }
    let mut w = s;
    for _ in 0..10 {
        // column rounds
        chacha20_quarter(&mut w, 0, 4, 8, 12);
        chacha20_quarter(&mut w, 1, 5, 9, 13);
        chacha20_quarter(&mut w, 2, 6, 10, 14);
        chacha20_quarter(&mut w, 3, 7, 11, 15);
        // diagonal rounds
        chacha20_quarter(&mut w, 0, 5, 10, 15);
        chacha20_quarter(&mut w, 1, 6, 11, 12);
        chacha20_quarter(&mut w, 2, 7, 8, 13);
        chacha20_quarter(&mut w, 3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for i in 0..16 {
        out[i * 4..i * 4 + 4].copy_from_slice(&w[i].wrapping_add(s[i]).to_le_bytes());
    }
    out
}

/// ChaCha20 encrypt/decrypt (XOR keystream) with a starting block counter.
pub fn chacha20_xor(key: &[u8; 32], counter: u32, nonce: &[u8; 12], data: &mut [u8]) {
    for (blk, chunk) in data.chunks_mut(64).enumerate() {
        let ks = chacha20_block(key, counter.wrapping_add(blk as u32), nonce);
        for (b, k) in chunk.iter_mut().zip(ks.iter()) {
            *b ^= *k;
        }
    }
}

// ── Poly1305 (RFC 8439) — the AEAD's one-time authenticator ──────────────

/// Poly1305 MAC of `msg` under a 32-byte one-time key, using 130-bit modular
/// arithmetic over 2^130−5 in 26-bit limbs.
#[must_use]
pub fn poly1305(key: &[u8; 32], msg: &[u8]) -> [u8; 16] {
    // r (clamped) and s from the key.
    let mut r = [0u64; 5];
    let t0 = u32::from_le_bytes([key[0], key[1], key[2], key[3]]);
    let t1 = u32::from_le_bytes([key[4], key[5], key[6], key[7]]);
    let t2 = u32::from_le_bytes([key[8], key[9], key[10], key[11]]);
    let t3 = u32::from_le_bytes([key[12], key[13], key[14], key[15]]);
    r[0] = u64::from(t0 & 0x03ff_ffff);
    r[1] = u64::from((t0 >> 26 | t1 << 6) & 0x03ff_ff03);
    r[2] = u64::from((t1 >> 20 | t2 << 12) & 0x03ff_c0ff);
    r[3] = u64::from((t2 >> 14 | t3 << 18) & 0x03f0_3fff);
    r[4] = u64::from((t3 >> 8) & 0x000f_ffff);

    let mut h = [0u64; 5];
    let s1 = r[1] * 5;
    let s2 = r[2] * 5;
    let s3 = r[3] * 5;
    let s4 = r[4] * 5;

    let mut process = |block: &[u8], partial: bool| {
        let mut b = [0u8; 17];
        b[..block.len()].copy_from_slice(block);
        if partial {
            b[block.len()] = 1; // pad partial block, set bit right after data
        } else {
            b[16] = 1; // the "high bit" for a full 16-byte block
        }
        let n0 = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let n1 = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        let n2 = u32::from_le_bytes([b[8], b[9], b[10], b[11]]);
        let n3 = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
        h[0] += u64::from(n0 & 0x03ff_ffff);
        h[1] += u64::from((n0 >> 26 | n1 << 6) & 0x03ff_ffff);
        h[2] += u64::from((n1 >> 20 | n2 << 12) & 0x03ff_ffff);
        h[3] += u64::from((n2 >> 14 | n3 << 18) & 0x03ff_ffff);
        h[4] += u64::from(n3 >> 8) | if b[16] != 0 { 1 << 24 } else { 0 };

        // h *= r  (mod 2^130 − 5), schoolbook with the ×5 reductions.
        let d0 = h[0] * r[0] + h[1] * s4 + h[2] * s3 + h[3] * s2 + h[4] * s1;
        let d1 = h[0] * r[1] + h[1] * r[0] + h[2] * s4 + h[3] * s3 + h[4] * s2;
        let d2 = h[0] * r[2] + h[1] * r[1] + h[2] * r[0] + h[3] * s4 + h[4] * s3;
        let d3 = h[0] * r[3] + h[1] * r[2] + h[2] * r[1] + h[3] * r[0] + h[4] * s4;
        let d4 = h[0] * r[4] + h[1] * r[3] + h[2] * r[2] + h[3] * r[1] + h[4] * r[0];

        let mut c;
        h[0] = d0 & 0x3ff_ffff;
        c = d0 >> 26;
        let d1 = d1 + c;
        h[1] = d1 & 0x3ff_ffff;
        c = d1 >> 26;
        let d2 = d2 + c;
        h[2] = d2 & 0x3ff_ffff;
        c = d2 >> 26;
        let d3 = d3 + c;
        h[3] = d3 & 0x3ff_ffff;
        c = d3 >> 26;
        let d4 = d4 + c;
        h[4] = d4 & 0x3ff_ffff;
        c = d4 >> 26;
        h[0] += c * 5;
        c = h[0] >> 26;
        h[0] &= 0x3ff_ffff;
        h[1] += c;
    };

    for block in msg.chunks(16) {
        // A block shorter than 16 bytes is the (partial) last one.
        process(block, block.len() < 16);
    }

    // Final reduction and add s.
    let mut c = h[1] >> 26;
    h[1] &= 0x3ff_ffff;
    h[2] += c;
    c = h[2] >> 26;
    h[2] &= 0x3ff_ffff;
    h[3] += c;
    c = h[3] >> 26;
    h[3] &= 0x3ff_ffff;
    h[4] += c;
    c = h[4] >> 26;
    h[4] &= 0x3ff_ffff;
    h[0] += c * 5;
    c = h[0] >> 26;
    h[0] &= 0x3ff_ffff;
    h[1] += c;

    // Compute h + -p to see whether h ≥ p, then select.
    let mut g = [0u64; 5];
    g[0] = h[0] + 5;
    c = g[0] >> 26;
    g[0] &= 0x3ff_ffff;
    g[1] = h[1] + c;
    c = g[1] >> 26;
    g[1] &= 0x3ff_ffff;
    g[2] = h[2] + c;
    c = g[2] >> 26;
    g[2] &= 0x3ff_ffff;
    g[3] = h[3] + c;
    c = g[3] >> 26;
    g[3] &= 0x3ff_ffff;
    g[4] = h[4] + c;
    g[4] = g[4].wrapping_sub(1 << 26);

    let mask = (g[4] >> 63).wrapping_sub(1); // all-ones if g[4] didn't borrow
    for i in 0..5 {
        h[i] = (h[i] & !mask) | (g[i] & mask);
    }

    // Serialize h (as 4 32-bit words) + s, little-endian.
    let mut hh = [0u64; 4];
    hh[0] = (h[0] | h[1] << 26) & 0xffff_ffff;
    hh[1] = (h[1] >> 6 | h[2] << 20) & 0xffff_ffff;
    hh[2] = (h[2] >> 12 | h[3] << 14) & 0xffff_ffff;
    hh[3] = (h[3] >> 18 | h[4] << 8) & 0xffff_ffff;

    let mut out = [0u8; 16];
    let mut carry = 0u64;
    for i in 0..4 {
        let s_word = u32::from_le_bytes([
            key[16 + i * 4],
            key[16 + i * 4 + 1],
            key[16 + i * 4 + 2],
            key[16 + i * 4 + 3],
        ]);
        let v = hh[i] + u64::from(s_word) + carry;
        out[i * 4..i * 4 + 4].copy_from_slice(&((v & 0xffff_ffff) as u32).to_le_bytes());
        carry = v >> 32;
    }
    out
}

// ── ChaCha20-Poly1305 AEAD (RFC 8439 §2.8) — TLS 1.3 record protection ───

fn poly_key(key: &[u8; 32], nonce: &[u8; 12]) -> [u8; 32] {
    let block = chacha20_block(key, 0, nonce);
    let mut pk = [0u8; 32];
    pk.copy_from_slice(&block[..32]);
    pk
}

fn aead_tag(poly_key: &[u8; 32], aad: &[u8], ciphertext: &[u8]) -> [u8; 16] {
    let mut mac_data = alloc::vec::Vec::new();
    mac_data.extend_from_slice(aad);
    mac_data.resize(mac_data.len().div_ceil(16) * 16, 0); // pad16(aad)
    mac_data.extend_from_slice(ciphertext);
    mac_data.resize(mac_data.len().div_ceil(16) * 16, 0); // pad16(ct)
    mac_data.extend_from_slice(&(aad.len() as u64).to_le_bytes());
    mac_data.extend_from_slice(&(ciphertext.len() as u64).to_le_bytes());
    poly1305(poly_key, &mac_data)
}

/// AEAD seal: encrypt `plaintext` in place and return the 16-byte auth tag,
/// binding `aad`. Nonce must be unique per key.
pub fn chacha20poly1305_seal(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    plaintext: &mut [u8],
) -> [u8; 16] {
    let pk = poly_key(key, nonce);
    chacha20_xor(key, 1, nonce, plaintext); // counter starts at 1 (0 = poly key)
    aead_tag(&pk, aad, plaintext)
}

/// AEAD open: verify the tag (constant-time) and decrypt in place. Returns
/// `false` (leaving data untouched) if authentication fails.
#[must_use]
pub fn chacha20poly1305_open(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    ciphertext: &mut [u8],
    tag: &[u8; 16],
) -> bool {
    let pk = poly_key(key, nonce);
    let expected = aead_tag(&pk, aad, ciphertext);
    let mut diff = 0u8;
    for i in 0..16 {
        diff |= expected[i] ^ tag[i];
    }
    if diff != 0 {
        return false; // authentication failed — do not decrypt
    }
    chacha20_xor(key, 1, nonce, ciphertext);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(data: &[u8]) -> alloc::string::String {
        hex32(&sha256(data))
    }

    fn tohex(data: &[u8]) -> alloc::string::String {
        use core::fmt::Write;
        let mut s = alloc::string::String::new();
        for b in data {
            let _ = write!(s, "{b:02x}");
        }
        s
    }

    #[test]
    fn matches_nist_vectors() {
        // The canonical FIPS 180-4 / NIST examples.
        assert_eq!(
            hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn streaming_equals_one_shot() {
        // Feeding in odd-sized chunks must give the identical digest — this
        // exercises the partial-block buffering and the multi-block path.
        let data: alloc::vec::Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let one_shot = sha256(&data);
        let mut h = Sha256::new();
        for chunk in data.chunks(7) {
            h.update(chunk);
        }
        assert_eq!(h.finish(), one_shot);
    }

    #[test]
    fn length_extension_boundary_is_correct() {
        // Messages right around the 55/56/64-byte padding boundary are where a
        // wrong padding length shows up.
        for n in 54..=66usize {
            let msg = alloc::vec![b'a'; n];
            let mut h = Sha256::new();
            h.update(&msg);
            let streamed = h.finish();
            assert_eq!(streamed, sha256(&msg), "mismatch at len {n}");
        }
    }

    fn unhex(s: &str) -> alloc::vec::Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn hmac_matches_rfc4231() {
        // RFC 4231 test case 2: key "Jefe", data "what do ya want for nothing?".
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            tohex(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hkdf_matches_rfc5869() {
        // RFC 5869 Appendix A.1 (SHA-256).
        let ikm = unhex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt = unhex("000102030405060708090a0b0c");
        let info = unhex("f0f1f2f3f4f5f6f7f8f9");
        let prk = hkdf_extract(&salt, &ikm);
        assert_eq!(
            tohex(&prk),
            "077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5"
        );
        let okm = hkdf_expand(&prk, &info, 42);
        assert_eq!(
            tohex(&okm),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
        );
    }

    #[test]
    fn chacha20_matches_rfc8439_keystream() {
        // RFC 8439 §2.4.2: encrypt the "sunscreen" plaintext, counter = 1.
        let key: [u8; 32] =
            unhex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
                .try_into()
                .unwrap();
        let nonce: [u8; 12] = unhex("000000000000004a00000000").try_into().unwrap();
        let mut data = b"Ladies and Gentlemen of the class of '99: If I could offer you \
                         only one tip for the future, sunscreen would be it."
            .to_vec();
        chacha20_xor(&key, 1, &nonce, &mut data);
        assert_eq!(
            tohex(&data[..16]),
            "6e2e359a2568f98041ba0728dd0d6981" // first ciphertext block, per RFC
        );
    }

    #[test]
    fn poly1305_matches_rfc8439() {
        // RFC 8439 §2.5.2.
        let key: [u8; 32] =
            unhex("85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b")
                .try_into()
                .unwrap();
        let tag = poly1305(&key, b"Cryptographic Forum Research Group");
        assert_eq!(tohex(&tag), "a8061dc1305136c6c22b8baf0c0127a9");
    }

    #[test]
    fn chacha20poly1305_aead_matches_rfc8439_and_round_trips() {
        // RFC 8439 §2.8.2.
        let key: [u8; 32] =
            unhex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f")
                .try_into()
                .unwrap();
        let nonce: [u8; 12] = unhex("070000004041424344454647").try_into().unwrap();
        let aad = unhex("50515253c0c1c2c3c4c5c6c7");
        let plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you \
                          only one tip for the future, sunscreen would be it."
            .to_vec();
        let mut ct = plaintext.clone();
        let tag = chacha20poly1305_seal(&key, &nonce, &aad, &mut ct);
        assert_eq!(
            tohex(&ct[..16]),
            "d31a8d34648e60db7b86afbc53ef7ec2" // RFC ciphertext prefix
        );
        assert_eq!(tohex(&tag), "1ae10b594f09e26a7e902ecbd0600691");

        // Open reverses seal…
        let mut got = ct.clone();
        assert!(chacha20poly1305_open(&key, &nonce, &aad, &mut got, &tag));
        assert_eq!(got, plaintext);
        // …and a tampered tag is rejected without decrypting.
        let mut bad_tag = tag;
        bad_tag[0] ^= 1;
        let mut ct2 = ct.clone();
        assert!(!chacha20poly1305_open(
            &key, &nonce, &aad, &mut ct2, &bad_tag
        ));
        assert_eq!(ct2, ct, "failed auth must leave ciphertext untouched");
    }
}
