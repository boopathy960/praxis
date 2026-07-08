//! httpd — the kernel serving the web, from scratch.
//!
//! Last brick made the kernel an internet *client* (DNS + TCP + HTTP fetch);
//! this one makes it a *server*: a from-scratch HTTP/1.0 daemon over the
//! kernel's own TCP listener, serving files straight out of the kernel's own
//! VFS. No sockets API, no userspace — the daemon is a polled kernel service
//! pumped by the idle loop, exactly like the net stack itself.
//!
//! Request path: accept from the TCP accept queue → accumulate bytes until the
//! header terminator → parse the request line → resolve the path in the VFS →
//! answer 200/404/405/400 → close. Responses are sent in bounded chunks so a
//! segment always fits the NIC's 2 KiB DMA buffers (and any real MTU).
//!
//! `GET /` serves `/index.html` when it exists, else a generated status page,
//! so a fresh boot has something to show a browser.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::fs::Vfs;
use crate::net::NetStack;
use crate::tcp::TcpState;

/// Keep every TCP payload comfortably under the e1000's 2 KiB DMA buffer
/// (minus 54 bytes of eth+ip+tcp headers) and any 1500-byte MTU on the path.
const SEND_CHUNK: usize = 1200;
/// A request head larger than this is refused — bounds per-connection memory.
const MAX_REQUEST_BYTES: usize = 4096;
/// Bodies are capped so a huge VFS file can't wedge the connection.
const MAX_BODY_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Default)]
pub struct HttpdStats {
    pub accepted: u64,
    pub served: u64,
    pub not_found: u64,
    pub bad_requests: u64,
    pub bytes_sent: u64,
}

struct Conn {
    id: usize,
    buf: Vec<u8>,
}

/// The polled HTTP daemon. Owns no I/O — every pump borrows the net stack and
/// the VFS from the nucleus, so it composes with everything else the kernel
/// runs.
pub struct Httpd {
    port: u16,
    conns: Vec<Conn>,
    pub stats: HttpdStats,
}

impl Httpd {
    /// Create the daemon and open its listener on `net`.
    pub fn start(port: u16, net: &mut NetStack) -> Self {
        net.tcp_listen(port);
        Self {
            port,
            conns: Vec::new(),
            stats: HttpdStats::default(),
        }
    }

    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub fn open_conns(&self) -> usize {
        self.conns.len()
    }

    /// One pump of the daemon: accept, read, answer. Returns how many
    /// responses were served this call. Call it from the idle loop.
    pub fn poll(&mut self, net: &mut NetStack, fs: &Vfs) -> usize {
        // Admit every connection the handshake completed since the last pump.
        while let Some(id) = net.tcp_accept(self.port) {
            self.stats.accepted += 1;
            self.conns.push(Conn {
                id,
                buf: Vec::new(),
            });
        }

        let mut served = 0;
        let mut keep: Vec<Conn> = Vec::new();
        for mut conn in core::mem::take(&mut self.conns) {
            conn.buf.extend_from_slice(&net.tcp_recv(conn.id));

            let header_end = find_header_end(&conn.buf);
            let dead = matches!(net.tcp_state(conn.id), Some(TcpState::Closed) | None);

            if let Some(end) = header_end {
                let head = String::from_utf8_lossy(&conn.buf[..end]).into_owned();
                let (status, body, content_type) = self.route(&head, fs);
                self.respond(net, conn.id, status, &content_type, &body);
                served += 1;
                continue; // connection answered and closed; drop it
            }
            if conn.buf.len() > MAX_REQUEST_BYTES {
                self.stats.bad_requests += 1;
                self.respond(
                    net,
                    conn.id,
                    "431 Request Header Fields Too Large",
                    "text/plain",
                    b"request too large\n",
                );
                continue;
            }
            if dead {
                continue; // peer vanished before completing a request
            }
            keep.push(conn);
        }
        self.conns = keep;
        served
    }

    /// Map a request head to (status line, body, content type).
    fn route(&mut self, head: &str, fs: &Vfs) -> (&'static str, Vec<u8>, String) {
        let mut parts = head.split_whitespace();
        let method = parts.next().unwrap_or_default();
        let raw_path = parts.next().unwrap_or_default();
        if method.is_empty() || raw_path.is_empty() {
            self.stats.bad_requests += 1;
            return (
                "400 Bad Request",
                b"malformed request line\n".to_vec(),
                "text/plain".into(),
            );
        }
        if method != "GET" && method != "HEAD" {
            self.stats.bad_requests += 1;
            return (
                "405 Method Not Allowed",
                b"only GET is served\n".to_vec(),
                "text/plain".into(),
            );
        }
        // Strip a query string; refuse traversal (the VFS is rooted anyway,
        // but a literal `..` segment has no business in a request).
        let path = raw_path.split('?').next().unwrap_or("/");
        if path.split('/').any(|seg| seg == "..") {
            self.stats.bad_requests += 1;
            return (
                "400 Bad Request",
                b"path traversal refused\n".to_vec(),
                "text/plain".into(),
            );
        }

        if path == "/" {
            if let Ok(index) = fs.read_all("/index.html") {
                self.stats.served += 1;
                return ("200 OK", bounded(index), "text/html".into());
            }
            self.stats.served += 1;
            return ("200 OK", self.status_page(fs), "text/html".into());
        }
        match fs.read_all(path) {
            Ok(bytes) => {
                self.stats.served += 1;
                ("200 OK", bounded(bytes), content_type_of(path).into())
            }
            Err(_) => {
                self.stats.not_found += 1;
                (
                    "404 Not Found",
                    format!("no such file: {path}\n").into_bytes(),
                    "text/plain".into(),
                )
            }
        }
    }

    /// The generated front page: proof the kernel itself built the response.
    fn status_page(&self, fs: &Vfs) -> Vec<u8> {
        let mut listing = String::new();
        if let Ok(entries) = fs.readdir("/") {
            for name in entries {
                listing.push_str(&format!("<li><a href=\"/{name}\">/{name}</a></li>"));
            }
        }
        format!(
            "<!doctype html><html><head><title>Praxis OS</title></head><body>\
             <h1>Praxis OS — served from the kernel</h1>\
             <p>This page was assembled by a from-scratch HTTP daemon inside a \
             from-scratch kernel: its own TCP stack, its own NIC driver, its \
             own VFS. No host OS, no sockets API, no JavaScript.</p>\
             <ul>{listing}</ul>\
             <p>requests served: {} · open connections: {}</p>\
             </body></html>",
            self.stats.served,
            self.conns.len()
        )
        .into_bytes()
    }

    /// Emit status line + headers + body in bounded chunks, then close.
    fn respond(
        &mut self,
        net: &mut NetStack,
        id: usize,
        status: &str,
        content_type: &str,
        body: &[u8],
    ) {
        let head = format!(
            "HTTP/1.0 {status}\r\nServer: praxis-nucleus/0.1\r\nContent-Type: {content_type}\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let mut wire = head.into_bytes();
        wire.extend_from_slice(body);
        for chunk in wire.chunks(SEND_CHUNK) {
            if net.tcp_send(id, chunk).is_err() {
                break; // peer reset mid-response; nothing more to do
            }
            net.poll_until_quiet();
        }
        self.stats.bytes_sent += wire.len() as u64;
        net.tcp_close(id);
        net.poll_until_quiet();
    }
}

/// Position just past the `\r\n\r\n` (or lenient `\n\n`) header terminator.
fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 4)
        .or_else(|| buf.windows(2).position(|w| w == b"\n\n").map(|p| p + 2))
}

fn bounded(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.truncate(MAX_BODY_BYTES);
    bytes
}

fn content_type_of(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".html") || lower.ends_with(".htm") {
        "text/html"
    } else if lower.ends_with(".json") {
        "application/json"
    } else if lower.ends_with(".css") {
        "text/css"
    } else {
        "text/plain"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proof::Tier;

    /// A loopback stack + VFS + daemon, with one client request driven through
    /// the real TCP state machine (handshake, data, close) end to end.
    fn serve_once(request: &str, path_setup: &[(&str, &str)]) -> (String, Httpd) {
        let mut net = NetStack::loopback([10, 0, 0, 1]);
        let mut fs = Vfs::new();
        for (path, content) in path_setup {
            if let Some(dir) = path.rfind('/') {
                if dir > 0 {
                    let _ = fs.mkdir(&path[..dir]);
                }
            }
            fs.write_all(path, content.as_bytes()).expect("seed file");
        }
        let mut httpd = Httpd::start(80, &mut net);

        // Client side: connect to ourselves over loopback and send the request.
        let conn = net
            .tcp_connect([10, 0, 0, 1], 80, Tier::Proven)
            .expect("connect");
        net.poll_until_quiet();
        httpd.poll(&mut net, &fs); // accept
        net.tcp_send(conn, request.as_bytes()).expect("send");
        net.poll_until_quiet();

        // Server pumps until the response lands; client drains it.
        let mut response = Vec::new();
        for _ in 0..8 {
            httpd.poll(&mut net, &fs);
            net.poll_until_quiet();
            response.extend_from_slice(&net.tcp_recv(conn));
        }
        (String::from_utf8_lossy(&response).into_owned(), httpd)
    }

    #[test]
    fn serves_a_vfs_file_over_real_tcp() {
        let (response, httpd) = serve_once(
            "GET /etc/motd HTTP/1.0\r\nHost: praxis\r\n\r\n",
            &[("/etc/motd", "proof buys speed\n")],
        );
        assert!(response.starts_with("HTTP/1.0 200 OK"), "{response}");
        assert!(response.contains("Content-Type: text/plain"));
        assert!(response.contains("Content-Length: 17"));
        assert!(response.ends_with("proof buys speed\n"), "{response}");
        assert_eq!(httpd.stats.served, 1);
        assert_eq!(httpd.stats.not_found, 0);
    }

    #[test]
    fn missing_file_is_a_404_not_a_hang() {
        let (response, httpd) = serve_once("GET /nope HTTP/1.0\r\n\r\n", &[]);
        assert!(response.starts_with("HTTP/1.0 404 Not Found"), "{response}");
        assert_eq!(httpd.stats.not_found, 1);
    }

    #[test]
    fn root_serves_generated_status_page() {
        let (response, _) = serve_once("GET / HTTP/1.0\r\n\r\n", &[("/etc/motd", "x")]);
        assert!(response.starts_with("HTTP/1.0 200 OK"));
        assert!(response.contains("text/html"));
        assert!(response.contains("served from the kernel"), "{response}");
    }

    #[test]
    fn root_prefers_index_html_when_present() {
        let (response, _) = serve_once(
            "GET / HTTP/1.0\r\n\r\n",
            &[("/index.html", "<h1>hello from the vfs</h1>")],
        );
        assert!(response.contains("hello from the vfs"), "{response}");
    }

    #[test]
    fn non_get_methods_are_refused() {
        let (response, httpd) = serve_once("POST /etc/motd HTTP/1.0\r\n\r\n", &[]);
        assert!(response.starts_with("HTTP/1.0 405"), "{response}");
        assert_eq!(httpd.stats.bad_requests, 1);
    }

    #[test]
    fn dotdot_traversal_is_refused() {
        let (response, httpd) = serve_once("GET /../etc/motd HTTP/1.0\r\n\r\n", &[]);
        assert!(response.starts_with("HTTP/1.0 400"), "{response}");
        assert_eq!(httpd.stats.bad_requests, 1);
    }

    #[test]
    fn request_split_across_segments_still_parses() {
        // The request arrives in two TCP segments; the daemon must buffer
        // until the header terminator, not fail on the first fragment.
        let mut net = NetStack::loopback([10, 0, 0, 1]);
        let mut fs = Vfs::new();
        fs.mkdir("/etc").unwrap();
        fs.write_all("/etc/motd", b"split ok\n").unwrap();
        let mut httpd = Httpd::start(80, &mut net);

        let conn = net
            .tcp_connect([10, 0, 0, 1], 80, Tier::Proven)
            .expect("connect");
        net.poll_until_quiet();
        httpd.poll(&mut net, &fs);
        net.tcp_send(conn, b"GET /etc/mo").unwrap();
        net.poll_until_quiet();
        httpd.poll(&mut net, &fs); // partial: must not answer yet
        assert_eq!(httpd.stats.served + httpd.stats.bad_requests, 0);
        net.tcp_send(conn, b"td HTTP/1.0\r\n\r\n").unwrap();
        net.poll_until_quiet();

        let mut response = Vec::new();
        for _ in 0..8 {
            httpd.poll(&mut net, &fs);
            net.poll_until_quiet();
            response.extend_from_slice(&net.tcp_recv(conn));
        }
        let text = String::from_utf8_lossy(&response);
        assert!(text.ends_with("split ok\n"), "{text}");
    }

    #[test]
    fn large_bodies_are_chunked_below_the_dma_limit() {
        // A body bigger than one send chunk must arrive complete — the daemon
        // chunks it so every segment fits the NIC buffer.
        let big: String = "abcdefghij".repeat(500); // 5000 bytes
        let (response, _) = serve_once(
            "GET /big.txt HTTP/1.0\r\n\r\n",
            &[("/big.txt", big.as_str())],
        );
        assert!(response.starts_with("HTTP/1.0 200 OK"));
        assert!(response.ends_with(&big), "body truncated or corrupted");
    }
}
