//! Setting a Cove server up from Runo, given only its SSH login: Runo checks the login,
//! installs or updates Cove there, keeps an SSH tunnel to it open, pairs with it, and
//! then offers to sign the agents in.
//!
//! Cove's API stays on the server's 127.0.0.1, so nothing new is opened to the internet:
//! Runo reaches it through `ssh -L`, which it starts again whenever the tunnel is gone
//! (after sleep or a network change).

use crate::connections::Connection;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Where Cove listens on the server.
const SERVER_PORT: u16 = 7457;

/// SSH options for the setup's own commands and the tunnel. Password logins can't be
/// typed here, so the key (or the SSH agent) has to work.
fn ssh_args(conn: &Connection) -> Vec<String> {
    let mut args = vec!["-o".to_string(), "BatchMode=yes".into()];
    args.extend(ssh_common(conn));
    args
}

/// The options both kinds of SSH command share.
fn ssh_common(conn: &Connection) -> Vec<String> {
    let mut args = vec![
        "-o".to_string(),
        "ConnectTimeout=15".into(),
        "-o".into(),
        "ServerAliveInterval=30".into(),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
    ];
    if conn.port != 0 {
        args.extend(["-p".into(), conn.port.to_string()]);
    }
    let key = conn.identity.trim();
    if !key.is_empty() {
        args.extend(["-i".into(), expand_home(key)]);
    }
    args
}

fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), crate::store::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => path.to_string(),
    }
}

fn answers(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(300),
    )
    .is_ok()
}

fn tunnels() -> &'static Mutex<HashMap<u64, Instant>> {
    static T: OnceLock<Mutex<HashMap<u64, Instant>>> = OnceLock::new();
    T.get_or_init(Default::default)
}

/// Make sure a connection that goes through SSH has its tunnel open, starting it when it
/// isn't. Connections without an SSH login pass straight through.
pub fn ensure_tunnel(conn: &Connection) -> Result<(), String> {
    if conn.ssh.trim().is_empty() || conn.tunnel_port == 0 {
        return Ok(());
    }
    let host = crate::connections::ssh_host(&conn.ssh, &conn.name)?;
    let port = conn.tunnel_port;
    if answers(port) {
        return Ok(());
    }
    {
        // Two callers at once start one tunnel; the second waits for it below.
        let mut started = tunnels().lock().map_err(|_| "tunnel state is poisoned")?;
        let recent = started
            .get(&conn.id)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(15));
        if !recent {
            started.insert(conn.id, Instant::now());
            let mut ssh = Command::new("ssh");
            ssh.args(["-N", "-o", "ExitOnForwardFailure=yes"])
                .args(ssh_args(conn))
                .args([
                    "-L",
                    &format!("127.0.0.1:{port}:127.0.0.1:{SERVER_PORT}"),
                    host,
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                // Its own process group: it keeps running when Runo's turn or Runo ends.
                ssh.process_group(0);
            }
            let child = ssh
                .spawn()
                .map_err(|e| format!("couldn't start ssh: {e}"))?;
            let id = conn.id;
            std::thread::spawn(move || {
                // Reap it when it ends, and let the next call start it again at once.
                let out = child.wait_with_output();
                if let Ok(mut t) = tunnels().lock() {
                    t.remove(&id);
                }
                if let Ok(out) = out {
                    let err = String::from_utf8_lossy(&out.stderr);
                    if let Some(line) = err.lines().rev().find(|l| !l.trim().is_empty()) {
                        crate::applog::warn("cove", format!("SSH tunnel ended: {line}"));
                    }
                }
            });
        }
    }
    for _ in 0..60 {
        if answers(port) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "Couldn't open the SSH tunnel to {host}. Check that `ssh {host}` works in a terminal."
    ))
}
