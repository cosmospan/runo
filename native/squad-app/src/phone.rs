//! What the phone bridge used to share with the crew's server (`v2::server`): pairing
//! tokens and checking them, which addresses to listen on (Tailscale first), and reading
//! one plain HTTP request.

use std::io::{BufRead, BufReader, Read};
use std::net::{IpAddr, Ipv4Addr};

/// The longest request body accepted: prompts, answers and approvals are small.
const BODY_MAX: usize = 256 * 1024;

/// 256 random bits as hex. Each `RandomState` is keyed from the OS's random source, so
/// hashing with fresh ones gives unpredictable words without another dependency.
pub(crate) fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    (0..4)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i);
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            );
            format!("{:016x}", h.finish())
        })
        .collect()
}

/// Compare without stopping at the first difference, so timing says nothing about the token.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Whether `given` is one of the paired tokens. Every token is compared in full, with no
/// early return, so timing says nothing about which one matched.
pub(crate) fn allowed(given: &str, tokens: &[String]) -> bool {
    tokens.iter().fold(false, |ok, t| same(given, t) | ok)
}

/// Whether this is a Tailscale address (100.64.0.0/10), which only devices signed in to
/// your tailnet can reach, over WireGuard.
pub fn is_tailscale(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            a == 100 && b & 0xc0 == 64
        }
        IpAddr::V6(_) => false,
    }
}

/// This computer's IPv4 addresses, on every interface.
#[cfg(unix)]
pub(crate) fn addresses() -> Vec<IpAddr> {
    let mut out = Vec::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills `list` with a linked list we only read, then free once.
    unsafe {
        if libc::getifaddrs(&mut list) != 0 {
            return out;
        }
        let mut cur = list;
        while !cur.is_null() {
            let addr = (*cur).ifa_addr;
            if !addr.is_null() && i32::from((*addr).sa_family) == libc::AF_INET {
                let sin = &*(addr as *const libc::sockaddr_in);
                out.push(IpAddr::V4(Ipv4Addr::from(u32::from_be(
                    sin.sin_addr.s_addr,
                ))));
            }
            cur = (*cur).ifa_next;
        }
        libc::freeifaddrs(list);
    }
    out
}

/// This computer's address on the route to Tailscale's own DNS server, which goes through
/// Tailscale when it is up. Nothing is sent: connecting a UDP socket only picks the route.
#[cfg(not(unix))]
pub(crate) fn addresses() -> Vec<IpAddr> {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("100.100.100.100:53")?;
            s.local_addr()
        })
        .map(|a| vec![a.ip()])
        .unwrap_or_default()
}

/// One parsed HTTP request.
#[derive(Debug, Default)]
pub(crate) struct Http {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) auth: String,
    pub(crate) if_none_match: String,
    pub(crate) body: Vec<u8>,
}

pub(crate) fn read_request(stream: &mut impl Read) -> Option<Http> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let mut http = Http {
        method: parts.next()?.to_string(),
        path: parts.next()?.to_string(),
        ..Default::default()
    };
    let mut length = 0usize;
    let mut header_bytes = 0usize;
    loop {
        line.clear();
        let n = reader.read_line(&mut line).ok()?;
        header_bytes += n;
        if n == 0 || header_bytes > 64 * 1024 {
            return None;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        let Some((name, value)) = trimmed.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.to_ascii_lowercase().as_str() {
            "content-length" => length = value.parse().ok()?,
            "authorization" => http.auth = value.to_string(),
            "if-none-match" => http.if_none_match = value.to_string(),
            _ => {}
        }
    }
    if length > BODY_MAX {
        return None;
    }
    http.body = vec![0; length];
    reader.read_exact(&mut http.body).ok()?;
    Some(http)
}
