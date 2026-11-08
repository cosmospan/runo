//! Just enough of a WebSocket client to speak the DevTools protocol to a browser on
//! this machine (or at the near end of an SSH tunnel): plain `ws://`, text messages,
//! fragments put back together, pings answered.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The sending half. Clones share the socket; each message goes out whole.
#[derive(Clone)]
pub struct WsWriter(Arc<Mutex<TcpStream>>);

/// The receiving half; only one thread reads.
pub struct WsReader {
    stream: BufReader<TcpStream>,
    writer: WsWriter,
}

/// Opens `ws://127.0.0.1:{port}{path}`.
pub fn connect(port: u16, path: &str) -> Result<(WsWriter, WsReader), String> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .map_err(|e| format!("couldn't reach the browser: {e}"))?;
    let _ = stream.set_nodelay(true);
    let key = {
        use base64::Engine as _;
        let mut raw = [0u8; 16];
        let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut raw);
        base64::engine::general_purpose::STANDARD.encode(raw)
    };
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut status = String::new();
    reader
        .read_line(&mut status)
        .map_err(|e| format!("the browser didn't answer: {e}"))?;
    if !status.contains(" 101") {
        return Err(format!("the browser refused: {}", status.trim()));
    }
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            return Err("the browser hung up".into());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }
    let _ = reader.get_ref().set_read_timeout(None);
    let writer = WsWriter(Arc::new(Mutex::new(stream)));
    Ok((
        writer.clone(),
        WsReader {
            stream: reader,
            writer,
        },
    ))
}

/// One frame's bytes as a client sends it: final, masked.
pub fn encode(opcode: u8, payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(0x80 | opcode);
    let len = payload.len();
    if len < 126 {
        out.push(0x80 | len as u8);
    } else if len <= u16::MAX as usize {
        out.push(0x80 | 126);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(0x80 | 127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    out.extend_from_slice(&mask);
    out.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    out
}

impl WsWriter {
    fn frame(&self, opcode: u8, payload: &[u8]) -> Result<(), String> {
        let mut mask = [0u8; 4];
        let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut mask);
        let bytes = encode(opcode, payload, mask);
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        s.write_all(&bytes).map_err(|e| e.to_string())
    }

    pub fn text(&self, text: &str) -> Result<(), String> {
        self.frame(0x1, text.as_bytes())
    }

    pub fn close(&self) {
        let _ = self.frame(0x8, &[]);
        let s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = s.shutdown(std::net::Shutdown::Both);
    }
}

impl WsReader {
    /// The next text message; `None` once the connection is closed.
    pub fn next(&mut self) -> Option<String> {
        let mut message: Vec<u8> = Vec::new();
        loop {
            let mut head = [0u8; 2];
            self.stream.read_exact(&mut head).ok()?;
            let fin = head[0] & 0x80 != 0;
            let opcode = head[0] & 0x0f;
            let masked = head[1] & 0x80 != 0;
            let len = match head[1] & 0x7f {
                126 => {
                    let mut b = [0u8; 2];
                    self.stream.read_exact(&mut b).ok()?;
                    u16::from_be_bytes(b) as u64
                }
                127 => {
                    let mut b = [0u8; 8];
                    self.stream.read_exact(&mut b).ok()?;
                    u64::from_be_bytes(b)
                }
                n => n as u64,
            };
            // Far more than any frame or screenshot a page sends.
            if len > 256 << 20 {
                return None;
            }
            let mut mask = [0u8; 4];
            if masked {
                self.stream.read_exact(&mut mask).ok()?;
            }
            let mut payload = vec![0u8; len as usize];
            self.stream.read_exact(&mut payload).ok()?;
            if masked {
                for (i, b) in payload.iter_mut().enumerate() {
                    *b ^= mask[i % 4];
                }
            }
            match opcode {
                0x8 => return None,
                0x9 => {
                    let _ = self.writer.frame(0xA, &payload);
                    continue;
                }
                0xA => continue,
                _ => message.extend_from_slice(&payload),
            }
            if fin {
                return Some(String::from_utf8_lossy(&message).into_owned());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_masked_and_sized() {
        let f = encode(1, b"hi", [1, 2, 3, 4]);
        assert_eq!(f, vec![0x81, 0x82, 1, 2, 3, 4, b'h' ^ 1, b'i' ^ 2]);
        let big = encode(1, &vec![0u8; 300], [0; 4]);
        assert_eq!(&big[..4], &[0x81, 0x80 | 126, 1, 44]);
        let huge = encode(2, &vec![0u8; 70_000], [0; 4]);
        assert_eq!(huge[1], 0x80 | 127);
        assert_eq!(huge.len(), 70_000 + 2 + 8 + 4);
    }

    #[test]
    fn talks_to_a_server() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (s, _) = listener.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            while r.read_line(&mut line).unwrap() > 0 && !line.ends_with("\r\n\r\n") {}
            let mut s = s;
            s.write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\r\n")
                .unwrap();
            // A ping, then "hello" in two fragments, unmasked as a server sends.
            s.write_all(&[0x89, 0]).unwrap();
            s.write_all(&[0x01, 3, b'h', b'e', b'l']).unwrap();
            s.write_all(&[0x80, 2, b'l', b'o']).unwrap();
            // The pong and the client's message come back masked.
            let mut got = [0u8; 6];
            r.read_exact(&mut got).unwrap();
            assert_eq!(got[0], 0x8A);
            let mut head = [0u8; 2];
            r.read_exact(&mut head).unwrap();
            let mut mask = [0u8; 4];
            r.read_exact(&mut mask).unwrap();
            let mut body = vec![0u8; (head[1] & 0x7f) as usize];
            r.read_exact(&mut body).unwrap();
            for (i, b) in body.iter_mut().enumerate() {
                *b ^= mask[i % 4];
            }
            assert_eq!(body, b"yo");
            s.write_all(&[0x88, 0]).unwrap();
        });
        let (w, mut r) = connect(port, "/devtools/browser/x").unwrap();
        assert_eq!(r.next().as_deref(), Some("hello"));
        w.text("yo").unwrap();
        assert_eq!(r.next(), None);
        server.join().unwrap();
    }
}
