//! DNS — a from-scratch, zero-dependency resolver codec.
//!
//! The last missing piece between the kernel and the actual internet: the
//! net stack can already move UDP datagrams over a real NIC, so all name
//! resolution needs is the wire format — an A-record query builder and a
//! response parser that understands the one genuinely tricky part of RFC
//! 1035, name compression (pointer-chasing with a loop guard, because a
//! hostile resolver must not be able to spin the kernel).
//!
//! This is a pure codec: bytes in, bytes out, no sockets and no clock, so it
//! tests exhaustively on the host. The shell's `dns`/`fetch` commands do the
//! socket work over [`crate::net::NetStack`] (slirp's resolver lives at
//! 10.0.2.3 under QEMU user networking).

use alloc::string::String;
use alloc::vec::Vec;

use crate::net::Ipv4Addr;

/// Standard query header: recursion desired, one question.
const FLAGS_RD: u16 = 0x0100;
const QTYPE_A: u16 = 1;
const QTYPE_CNAME: u16 = 5;
const QCLASS_IN: u16 = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum DnsError {
    /// Name doesn't fit the wire format (empty, label > 63, name > 255).
    BadName,
    /// Response too short / truncated mid-record.
    Truncated,
    /// Response id does not match the query id.
    WrongId,
    /// The server signalled an error (RCODE != 0), e.g. NXDOMAIN = 3.
    Rcode(u8),
    /// Compression pointers looped or ran past the message.
    BadPointer,
}

/// Build an A-record query for `name` with transaction id `id`.
pub fn build_query(id: u16, name: &str) -> Result<Vec<u8>, DnsError> {
    let mut msg = Vec::with_capacity(17 + name.len());
    msg.extend_from_slice(&id.to_be_bytes());
    msg.extend_from_slice(&FLAGS_RD.to_be_bytes());
    msg.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    msg.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
    msg.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    msg.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    encode_name(name, &mut msg)?;
    msg.extend_from_slice(&QTYPE_A.to_be_bytes());
    msg.extend_from_slice(&QCLASS_IN.to_be_bytes());
    Ok(msg)
}

/// QNAME encoding: dot-separated labels, each length-prefixed, NUL-rooted.
fn encode_name(name: &str, out: &mut Vec<u8>) -> Result<(), DnsError> {
    let name = name.strip_suffix('.').unwrap_or(name);
    if name.is_empty() || name.len() > 255 {
        return Err(DnsError::BadName);
    }
    for label in name.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(DnsError::BadName);
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    Ok(())
}

/// One parsed answer record we care about.
#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    A(Ipv4Addr),
    Cname(String),
}

/// Parse a response to query `id`: verify the transaction, walk the question
/// section, and collect A records (and CNAMEs, for visibility) out of the
/// answer section. Compression pointers are followed with a hop budget.
pub fn parse_response(msg: &[u8], id: u16) -> Result<Vec<Answer>, DnsError> {
    if msg.len() < 12 {
        return Err(DnsError::Truncated);
    }
    if u16::from_be_bytes([msg[0], msg[1]]) != id {
        return Err(DnsError::WrongId);
    }
    let rcode = msg[3] & 0x0F;
    if rcode != 0 {
        return Err(DnsError::Rcode(rcode));
    }
    let qdcount = u16::from_be_bytes([msg[4], msg[5]]);
    let ancount = u16::from_be_bytes([msg[6], msg[7]]);

    let mut at = 12usize;
    for _ in 0..qdcount {
        at = skip_name(msg, at)?;
        at = at.checked_add(4).ok_or(DnsError::Truncated)?; // qtype + qclass
        if at > msg.len() {
            return Err(DnsError::Truncated);
        }
    }

    let mut answers = Vec::new();
    for _ in 0..ancount {
        at = skip_name(msg, at)?;
        if at + 10 > msg.len() {
            return Err(DnsError::Truncated);
        }
        let rtype = u16::from_be_bytes([msg[at], msg[at + 1]]);
        let rdlen = u16::from_be_bytes([msg[at + 8], msg[at + 9]]) as usize;
        let rdata = at + 10;
        if rdata + rdlen > msg.len() {
            return Err(DnsError::Truncated);
        }
        match rtype {
            QTYPE_A if rdlen == 4 => {
                answers.push(Answer::A([
                    msg[rdata],
                    msg[rdata + 1],
                    msg[rdata + 2],
                    msg[rdata + 3],
                ]));
            }
            QTYPE_CNAME => {
                answers.push(Answer::Cname(read_name(msg, rdata)?));
            }
            _ => {}
        }
        at = rdata + rdlen;
    }
    Ok(answers)
}

/// Just the A records, in answer order — what `connect` actually needs.
pub fn a_records(answers: &[Answer]) -> Vec<Ipv4Addr> {
    answers
        .iter()
        .filter_map(|a| match a {
            Answer::A(ip) => Some(*ip),
            Answer::Cname(_) => None,
        })
        .collect()
}

/// Advance past a (possibly compressed) name starting at `at`; a pointer
/// ends the name immediately (RFC 1035 §4.1.4).
fn skip_name(msg: &[u8], mut at: usize) -> Result<usize, DnsError> {
    loop {
        let len = *msg.get(at).ok_or(DnsError::Truncated)? as usize;
        if len & 0xC0 == 0xC0 {
            return Ok(at + 2);
        }
        if len == 0 {
            return Ok(at + 1);
        }
        at += 1 + len;
    }
}

/// Decode a (possibly compressed) name into dotted text. Pointer hops are
/// budgeted so a malicious loop terminates as [`DnsError::BadPointer`].
fn read_name(msg: &[u8], mut at: usize) -> Result<String, DnsError> {
    let mut name = String::new();
    let mut hops = 0;
    loop {
        let len = *msg.get(at).ok_or(DnsError::Truncated)? as usize;
        if len & 0xC0 == 0xC0 {
            let lo = *msg.get(at + 1).ok_or(DnsError::Truncated)? as usize;
            at = (len & 0x3F) << 8 | lo;
            hops += 1;
            if hops > 16 {
                return Err(DnsError::BadPointer);
            }
            continue;
        }
        if len == 0 {
            return Ok(name);
        }
        let label = msg.get(at + 1..at + 1 + len).ok_or(DnsError::Truncated)?;
        if !name.is_empty() {
            name.push('.');
        }
        for &b in label {
            name.push(b as char);
        }
        at += 1 + len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_matches_the_wire_format() {
        let q = build_query(0xBEEF, "example.com").unwrap();
        assert_eq!(&q[0..2], &[0xBE, 0xEF]); // id
        assert_eq!(&q[2..4], &[0x01, 0x00]); // RD
        assert_eq!(&q[4..6], &[0, 1]); // one question
        let qname = &q[12..];
        assert_eq!(qname[0], 7);
        assert_eq!(&qname[1..8], b"example");
        assert_eq!(qname[8], 3);
        assert_eq!(&qname[9..12], b"com");
        assert_eq!(qname[12], 0);
        assert_eq!(&qname[13..15], &[0, 1]); // QTYPE A
        assert_eq!(&qname[15..17], &[0, 1]); // QCLASS IN
    }

    #[test]
    fn bad_names_are_refused() {
        assert_eq!(build_query(1, ""), Err(DnsError::BadName));
        assert_eq!(build_query(1, "a..b"), Err(DnsError::BadName));
        let long = alloc::format!("{}.com", "x".repeat(64));
        assert_eq!(build_query(1, &long), Err(DnsError::BadName));
        // A trailing dot (FQDN form) is fine.
        assert!(build_query(1, "example.com.").is_ok());
    }

    /// Hand-built response: question echoed, then one CNAME and one A record,
    /// both naming via a compression pointer back to the question.
    fn sample_response(id: u16) -> Vec<u8> {
        let mut m = Vec::new();
        m.extend_from_slice(&id.to_be_bytes());
        m.extend_from_slice(&[0x81, 0x80]); // response, RD+RA, rcode 0
        m.extend_from_slice(&[0, 1, 0, 2, 0, 0, 0, 0]); // 1 question, 2 answers
        encode_name("example.com", &mut m).unwrap(); // offset 12
        m.extend_from_slice(&[0, 1, 0, 1]); // qtype/qclass
                                            // answer 1: CNAME → "edge.example.com" (target written inline)
        m.extend_from_slice(&[0xC0, 12]); // name = pointer to offset 12
        m.extend_from_slice(&[0, 5, 0, 1]); // type CNAME, class IN
        m.extend_from_slice(&[0, 0, 0, 60]); // ttl
        let mut target = Vec::new();
        target.push(4);
        target.extend_from_slice(b"edge");
        target.extend_from_slice(&[0xC0, 12]); // …then point at example.com
        m.extend_from_slice(&(target.len() as u16).to_be_bytes());
        m.extend_from_slice(&target);
        // answer 2: A 93.184.216.34
        m.extend_from_slice(&[0xC0, 12]);
        m.extend_from_slice(&[0, 1, 0, 1]);
        m.extend_from_slice(&[0, 0, 0, 60]);
        m.extend_from_slice(&[0, 4]);
        m.extend_from_slice(&[93, 184, 216, 34]);
        m
    }

    #[test]
    fn response_parses_compression_and_yields_records() {
        let answers = parse_response(&sample_response(7), 7).unwrap();
        assert_eq!(answers.len(), 2);
        assert_eq!(answers[0], Answer::Cname(String::from("edge.example.com")));
        assert_eq!(answers[1], Answer::A([93, 184, 216, 34]));
        assert_eq!(a_records(&answers), alloc::vec![[93, 184, 216, 34]]);
    }

    #[test]
    fn wrong_id_and_rcode_are_rejected() {
        assert_eq!(
            parse_response(&sample_response(7), 8),
            Err(DnsError::WrongId)
        );
        let mut nx = sample_response(7);
        nx[3] |= 3; // NXDOMAIN
        assert_eq!(parse_response(&nx, 7), Err(DnsError::Rcode(3)));
    }

    #[test]
    fn a_pointer_loop_cannot_spin_the_kernel() {
        let mut m = Vec::new();
        m.extend_from_slice(&[0, 9, 0x81, 0x80, 0, 0, 0, 1, 0, 0, 0, 0]);
        // answer whose name is a pointer to itself
        m.extend_from_slice(&[0xC0, 12]); // offset 12 = this very pointer
        m.extend_from_slice(&[0, 5, 0, 1, 0, 0, 0, 60, 0, 2, 0xC0, 12]);
        // skip_name tolerates the pointer (it doesn't follow); the CNAME
        // rdata read does follow it and must hit the hop budget.
        assert_eq!(parse_response(&m, 9), Err(DnsError::BadPointer));
    }

    #[test]
    fn truncated_responses_never_panic() {
        let full = sample_response(3);
        for cut in 0..full.len() {
            // Every prefix must return an error or a (possibly shorter)
            // answer list — never panic, never loop.
            let _ = parse_response(&full[..cut], 3);
        }
    }
}
