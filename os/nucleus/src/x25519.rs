//! X25519 — the Curve25519 Diffie-Hellman key exchange (RFC 7748).
//!
//! The last cryptographic primitive TLS 1.3 needs: an elliptic-curve key
//! agreement so two parties can derive a shared secret over an open channel.
//! X25519 is a scalar multiplication on Montgomery Curve25519 over the prime
//! field `2^255 − 19`, computed with the constant-time Montgomery ladder.
//!
//! The field arithmetic is the compact, extensively-audited TweetNaCl
//! formulation (16 signed 16-bit limbs), ported to Rust — small enough to read
//! in full and checked here against the RFC 7748 §5.2 test vectors. Together
//! with [`crate::crypto`] (SHA-256, HMAC, HKDF, ChaCha20-Poly1305) this
//! completes the TLS 1.3 cipher suite's primitive set: key exchange, key
//! schedule, and record protection are all present and vector-verified.

/// A field element: 16 signed limbs, each holding ~16 bits (TweetNaCl `gf`).
type Gf = [i64; 16];

/// `121665`, the Montgomery curve constant `(A−2)/4`, as a field element.
const A24: Gf = [0xdb41, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// The standard base point u = 9 (little-endian), for public-key generation.
pub const BASE_POINT: [u8; 32] = [
    9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

fn car25519(o: &mut Gf) {
    for i in 0..16 {
        o[i] += 1 << 16;
        let c = o[i] >> 16;
        o[(i + 1) * usize::from(i < 15)] += c - 1 + 37 * (c - 1) * i64::from(i == 15);
        o[i] -= c << 16;
    }
}

/// Constant-time conditional swap of `p` and `q` when `b == 1`.
fn sel25519(p: &mut Gf, q: &mut Gf, b: i64) {
    let c = !(b - 1);
    for i in 0..16 {
        let t = c & (p[i] ^ q[i]);
        p[i] ^= t;
        q[i] ^= t;
    }
}

fn add(o: &mut Gf, a: &Gf, b: &Gf) {
    for i in 0..16 {
        o[i] = a[i] + b[i];
    }
}
fn sub(o: &mut Gf, a: &Gf, b: &Gf) {
    for i in 0..16 {
        o[i] = a[i] - b[i];
    }
}

/// Field multiply `o = a·b mod 2^255−19`.
fn mul(o: &mut Gf, a: &Gf, b: &Gf) {
    let mut t = [0i64; 31];
    for i in 0..16 {
        for j in 0..16 {
            t[i + j] += a[i] * b[j];
        }
    }
    for i in 0..15 {
        t[i] += 38 * t[i + 16]; // fold the top half back (2^256 ≡ 38)
    }
    o[..16].copy_from_slice(&t[..16]);
    car25519(o);
    car25519(o);
}

fn sqr(o: &mut Gf, a: &Gf) {
    let a2 = *a;
    mul(o, &a2, &a2);
}

/// Field inverse via Fermat: `a^(p−2)`.
fn inv25519(o: &mut Gf, i: &Gf) {
    let mut c = *i;
    for a in (0..=253).rev() {
        let cc = c;
        sqr(&mut c, &cc);
        if a != 2 && a != 4 {
            let cc = c;
            mul(&mut c, &cc, i);
        }
    }
    *o = c;
}

fn unpack25519(o: &mut Gf, n: &[u8; 32]) {
    for i in 0..16 {
        o[i] = i64::from(n[2 * i]) + (i64::from(n[2 * i + 1]) << 8);
    }
    o[15] &= 0x7fff; // clear the top bit of the u-coordinate
}

fn pack25519(o: &mut [u8; 32], n: &Gf) {
    let mut t = *n;
    car25519(&mut t);
    car25519(&mut t);
    car25519(&mut t);
    for _ in 0..2 {
        let mut m: Gf = [0; 16];
        m[0] = t[0] - 0xffed;
        for i in 1..15 {
            m[i] = t[i] - 0xffff - ((m[i - 1] >> 16) & 1);
            m[i - 1] &= 0xffff;
        }
        m[15] = t[15] - 0x7fff - ((m[14] >> 16) & 1);
        let b = (m[15] >> 16) & 1;
        m[14] &= 0xffff;
        sel25519(&mut t, &mut m, 1 - b);
    }
    for i in 0..16 {
        o[2 * i] = (t[i] & 0xff) as u8;
        o[2 * i + 1] = (t[i] >> 8) as u8;
    }
}

/// The core operation: scalar multiplication `q = scalar · point` on
/// Curve25519. Both are 32-byte little-endian; the scalar is clamped per RFC.
#[must_use]
pub fn scalarmult(scalar: &[u8; 32], point: &[u8; 32]) -> [u8; 32] {
    let mut z = *scalar;
    z[31] = (z[31] & 127) | 64;
    z[0] &= 248;

    let mut x: Gf = [0; 16];
    unpack25519(&mut x, point);

    let mut a: Gf = [0; 16];
    let mut b: Gf = x;
    let mut c: Gf = [0; 16];
    let mut d: Gf = [0; 16];
    let mut e: Gf = [0; 16];
    let mut f: Gf = [0; 16];
    a[0] = 1;
    d[0] = 1;

    for i in (0..=254).rev() {
        let r = (i64::from(z[i >> 3]) >> (i & 7)) & 1;
        sel25519(&mut a, &mut b, r);
        sel25519(&mut c, &mut d, r);
        add(&mut e, &a, &c);
        let at = a;
        sub(&mut a, &at, &c);
        add(&mut c, &b, &d);
        let bt = b;
        sub(&mut b, &bt, &d);
        sqr(&mut d, &e);
        sqr(&mut f, &a);
        let ct = c;
        let at2 = a;
        mul(&mut a, &ct, &at2);
        mul(&mut c, &b, &e);
        add(&mut e, &a, &c);
        let at = a;
        sub(&mut a, &at, &c);
        let at = a;
        sqr(&mut b, &at);
        sub(&mut c, &d, &f);
        let ct2 = c;
        mul(&mut a, &ct2, &A24);
        let at = a;
        add(&mut a, &at, &d);
        let ct3 = c;
        mul(&mut c, &ct3, &a);
        mul(&mut a, &d, &f);
        let dt = d;
        mul(&mut d, &b, &x);
        let et = e;
        sqr(&mut b, &et);
        sel25519(&mut a, &mut b, r);
        sel25519(&mut c, &mut d, r);
        let _ = (dt, bt, ct);
    }

    // q = a / c  (i.e. a · c^{-1}).
    let ci = c;
    inv25519(&mut c, &ci);
    let at = a;
    mul(&mut a, &at, &c);
    let mut out = [0u8; 32];
    pack25519(&mut out, &a);
    out
}

/// Derive the public key for a private scalar: `scalar · basepoint`.
#[must_use]
pub fn public_key(secret: &[u8; 32]) -> [u8; 32] {
    scalarmult(secret, &BASE_POINT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }
    fn tohex(b: &[u8; 32]) -> alloc::string::String {
        use core::fmt::Write;
        let mut s = alloc::string::String::new();
        for x in b {
            let _ = write!(s, "{x:02x}");
        }
        s
    }

    #[test]
    fn public_keys_match_rfc7748_6_1() {
        // RFC 7748 §6.1: the two published private keys must produce the two
        // published public keys (scalar · basepoint).
        let alice_sk = unhex("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
        assert_eq!(
            tohex(&public_key(&alice_sk)),
            "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a"
        );
        let bob_sk = unhex("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
        assert_eq!(
            tohex(&public_key(&bob_sk)),
            "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f"
        );
    }

    #[test]
    fn matches_rfc7748_vector_2() {
        let scalar = unhex("4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d");
        let u = unhex("e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493");
        assert_eq!(
            tohex(&scalarmult(&scalar, &u)),
            "95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957"
        );
    }

    #[test]
    fn diffie_hellman_agrees_on_a_shared_secret() {
        // Two parties derive the same secret from each other's public keys —
        // the property TLS actually relies on.
        let alice_sk = unhex("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
        let bob_sk = unhex("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
        let alice_pk = public_key(&alice_sk);
        let bob_pk = public_key(&bob_sk);
        let shared_a = scalarmult(&alice_sk, &bob_pk);
        let shared_b = scalarmult(&bob_sk, &alice_pk);
        assert_eq!(shared_a, shared_b, "both sides must agree");
        // And it matches the published RFC 7748 §6.1 shared secret.
        assert_eq!(
            tohex(&shared_a),
            "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742"
        );
    }
}
