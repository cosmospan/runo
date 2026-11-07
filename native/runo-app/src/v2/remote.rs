//! The desktop app's side of `squad serve`: your own server, reached over SSH.
//!
//! You give it one thing, `you@your-server`, and [`connect`] does the rest
//! with your SSH key: it puts Squad Code on the server and keeps it running
//! as a service (scripts/install-server.sh), asks it for a pairing code, opens
//! a tunnel to it and trades the code for this computer's own token. The
//! server only ever listens on its own 127.0.0.1, so nothing is open to the
//! internet; everything goes through the tunnel.
//!
//! Once connected, a [`Link`] keeps the tunnel up on a thread of its own and
//! reads the server's state every few seconds, so the intelligence layer can
//! place jobs there ([`Link::server`]). A job placed there is sent with
//! [`Link::send`], and its copy in this app is kept up to date from the
//! server's ([`Link::take_jobs`]) until it's done. Nothing here blocks the
//! window: every call to the server happens on the link's thread.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::brain;
use super::server::PORT;
use super::store;

/// What runs on the server to install it and keep it running.
const SETUP: &str = include_str!("../../../../scripts/install-server.sh");
/// How often the server's state is read while connected.
const REFRESH: Duration = Duration::from_secs(4);
/// How long to wait before opening a tunnel again after it dropped.
const RETRY: Duration = Duration::from_secs(15);

/// This computer's pairing with a server, kept in `remote.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// What you typed: `you@host`, or a name from ~/.ssh/config.
    pub host: String,
    /// The path of SquadCode on the server.
    pub bin: String,
    /// This computer's name there, and its token.
    pub name: String,
    pub token: String,
}

impl Config {
    /// The server's name for people: the host without the user.
    pub fn label(&self) -> String {
        label_of(&self.host)
    }
}

fn label_of(host: &str) -> String {
    host.rsplit('@').next().unwrap_or(host).to_string()
}

fn path() -> PathBuf {
    crate::store::data_dir().join("remote.json")
}

pub fn load() -> Option<Config> {
    let text = std::fs::read_to_string(path()).ok()?;
    serde_json::from_str::<Config>(&text)
        .ok()
        .filter(|c| !c.host.is_empty() && !c.token.is_empty())
}

fn save(c: &Config) -> Result<(), String> {
    let text = serde_json::to_string_pretty(c).map_err(|e| e.to_string())?;
    let p = path();
    std::fs::write(&p, text).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Forgets the server here. The server keeps running; `squad unpair` there
/// revokes this computer's token.
pub fn forget() {
    let _ = std::fs::remove_file(path());
}

/// Whether `host` reads as something ssh can be given: `user@host`, `host`,
/// or a name from ~/.ssh/config, with no spaces or options.
pub fn valid_host(host: &str) -> bool {
    let h = host.trim();
    !h.is_empty()
        && !h.starts_with('-')
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || "@.-_:[]".contains(c))
}

// ---------- ssh ----------

fn ssh_base(host: &str) -> Command {
    let mut cmd = Command::new("ssh");
    cmd.args([
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "StrictHostKeyChecking=accept-new",
    ]);
    cmd.arg(host);
    cmd
}

/// Runs `command` on the server, with `input` on its stdin, and gives back
/// what it printed.
fn ssh(host: &str, command: &str, input: Option<&str>) -> Result<String, String> {
    let mut cmd = ssh_base(host);
    cmd.arg(command)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "ssh isn't installed on this computer".to_string(),
        _ => e.to_string(),
    })?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = stdin.write_all(text.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The script's own `error:` line says it best; else ssh's last line.
    let why = stderr
        .lines()
        .find_map(|l| l.strip_prefix("error: "))
        .map(str::to_string)
        .or_else(|| {
            stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
        })
        .unwrap_or_else(|| format!("it stopped with {}", out.status));
    Err(why)
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// A port nothing on this computer is using.
fn free_port() -> Option<u16> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .ok()
}

/// `ssh -N -L`: the server's 127.0.0.1:7640 at a port on this computer.
struct Tunnel {
    child: Child,
    port: u16,
}

impl Tunnel {
    fn open(host: &str) -> Result<Tunnel, String> {
        let port = free_port().ok_or("no free port on this computer")?;
        let mut cmd = ssh_base(host);
        cmd.args([
            "-N",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "ServerAliveInterval=20",
            "-o",
            "ServerAliveCountMax=3",
            "-L",
            &format!("127.0.0.1:{port}:127.0.0.1:{PORT}"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let until = Instant::now() + Duration::from_secs(15);
        while Instant::now() < until {
            if let Ok(Some(_)) = child.try_wait() {
                let mut why = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut why);
                }
                let why = why
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("ssh stopped")
                    .trim()
                    .to_string();
                return Err(why);
            }
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                // ssh is listening; stop reading its errors so it never blocks on them.
                drop(child.stderr.take());
                return Ok(Tunnel { child, port });
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        let _ = child.kill();
        Err("the tunnel didn't open in time".into())
    }

    fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------- http ----------

/// One request to the server through the tunnel.
fn http(
    port: u16,
    method: &str,
    path: &str,
    token: &str,
    body: Option<&Value>,
) -> Result<(u16, Value), String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(20)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(10)));
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    let auth = if token.is_empty() {
        String::new()
    } else {
        format!("Authorization: Bearer {token}\r\n")
    };
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    parse_reply(&raw)
}

fn parse_reply(raw: &[u8]) -> Result<(u16, Value), String> {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or("the server's answer was cut off")?;
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or("the server's answer made no sense")?;
    let value = if body.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(body).map_err(|_| "the server's answer wasn't JSON".to_string())?
    };
    Ok((status, value))
}

fn error_of(v: &Value) -> String {
    v["error"]
        .as_str()
        .unwrap_or("the server said no")
        .to_string()
}

// ---------- connecting ----------

/// What [`connect`] reports as it goes.
#[derive(Clone, Debug)]
pub enum Step {
    Say(String),
    Done(Config),
    Failed(String),
}

/// Sets the server up and pairs this computer with it, on a thread of its own.
pub fn connect(host: String, device: String) -> Receiver<Step> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let step = match setup(&host, &device, &tx) {
            Ok(c) => Step::Done(c),
            Err(why) => Step::Failed(why),
        };
        let _ = tx.send(step);
    });
    rx
}

fn setup(host: &str, device: &str, tx: &Sender<Step>) -> Result<Config, String> {
    let say = |t: String| {
        let _ = tx.send(Step::Say(t));
    };
    let host = host.trim();
    if !valid_host(host) {
        return Err("Type it the way you'd type it after ssh, like you@203.0.113.7.".into());
    }
    say(format!("Reaching {host} over SSH…"));
    let os = ssh(host, "uname -sm", None).map_err(|why| {
        format!("Couldn't reach {host} over SSH: {why}. Check that `ssh {host}` works in a terminal without asking for a password; Squad uses your SSH key.")
    })?;
    if !os.trim_start().starts_with("Linux") {
        return Err(format!(
            "{host} runs {}; a Squad server needs Linux.",
            os.trim()
        ));
    }
    say("Installing Squad Code there and keeping it running…".into());
    let out = ssh(host, "sh -s", Some(SETUP))
        .map_err(|why| format!("Setting up {host} stopped: {why}"))?;
    let bin = out
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix("bin="))
        .map(|b| b.trim().to_string())
        .ok_or_else(|| format!("Setting up {host} didn't finish."))?;
    say("Pairing this computer…".into());
    let code = ssh(
        host,
        &format!("{} v2 pair --bare {}", quote(&bin), quote(device)),
        None,
    )
    .map_err(|why| format!("Couldn't get a pairing code: {why}"))?
    .trim()
    .to_string();
    let tunnel = Tunnel::open(host).map_err(|why| format!("Couldn't open a tunnel: {why}"))?;
    // The service may still be starting.
    let mut last = String::new();
    for _ in 0..20 {
        match http(tunnel.port, "GET", "/v1/hello", "", None) {
            Ok((200, _)) => {
                last.clear();
                break;
            }
            Ok((_, v)) => last = error_of(&v),
            Err(e) => last = e,
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if !last.is_empty() {
        return Err(format!("Squad Code isn't answering on {host}: {last}"));
    }
    let (status, v) = http(
        tunnel.port,
        "POST",
        "/v1/pair",
        "",
        Some(&json!({ "code": code, "name": device })),
    )
    .map_err(|e| format!("Pairing failed: {e}"))?;
    if status != 200 {
        return Err(format!("Pairing failed: {}", error_of(&v)));
    }
    let cfg = Config {
        host: host.to_string(),
        bin,
        name: v["name"].as_str().unwrap_or(device).to_string(),
        token: v["token"].as_str().unwrap_or("").to_string(),
    };
    if cfg.token.is_empty() {
        return Err("Pairing failed: the server sent no token.".into());
    }
    save(&cfg).map_err(|e| format!("Couldn't keep the pairing: {e}"))?;
    Ok(cfg)
}

/// This computer's name, for the server's list of devices.
pub fn device_name() -> String {
    let name = std::env::var("HOSTNAME")
        .ok()
        .or_else(|| {
            Command::new("hostname")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        })
        .unwrap_or_default();
    let name = name.trim().trim_end_matches(".local").to_string();
    if name.is_empty() {
        "desktop".into()
    } else {
        name
    }
}

// ---------- the live link ----------

/// A project on the server.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Project {
    pub name: String,
    pub path: String,
    pub remote: String,
}

/// What the link last saw.
#[derive(Clone, Debug, Default)]
struct Seen {
    /// One line for Settings: "Connected", or why not.
    line: String,
    up: bool,
    engines: Vec<String>,
    projects: Vec<Project>,
    /// Jobs read since the app last took them, by their id on the server.
    jobs: HashMap<u64, store::Job>,
    /// Answers to [`Link::send`]: the local id, and the server's id or why not.
    sent: Vec<(u64, Result<u64, String>)>,
    /// Notes for the person.
    notes: Vec<String>,
}

enum Ask {
    Send(u64, Value),
    Act(u64, String, Value),
    Watch(Vec<u64>),
    AddProject(String, String),
}

/// A connection to the server, kept up on its own thread.
pub struct Link {
    pub cfg: Config,
    seen: Arc<Mutex<Seen>>,
    tx: Sender<Ask>,
}

impl Link {
    pub fn start(cfg: Config) -> Link {
        let seen = Arc::new(Mutex::new(Seen {
            line: format!("Connecting to {}…", cfg.label()),
            ..Default::default()
        }));
        let (tx, rx) = channel();
        {
            let (cfg, seen) = (cfg.clone(), seen.clone());
            std::thread::spawn(move || worker(cfg, seen, rx));
        }
        Link { cfg, seen, tx }
    }

    fn seen(&self) -> std::sync::MutexGuard<'_, Seen> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The server as the intelligence layer sees it.
    pub fn server(&self) -> brain::place::Server {
        let s = self.seen();
        brain::place::Server {
            name: self.cfg.label(),
            reachable: s.up,
            engines: s.engines.clone(),
            projects: s
                .projects
                .iter()
                .map(|p| {
                    if p.remote.is_empty() {
                        p.name.clone()
                    } else {
                        p.remote.clone()
                    }
                })
                .collect(),
        }
    }

    pub fn up(&self) -> bool {
        self.seen().up
    }

    /// One line on how the connection is.
    pub fn line(&self) -> String {
        self.seen().line.clone()
    }

    pub fn engines(&self) -> Vec<String> {
        self.seen().engines.clone()
    }

    pub fn projects(&self) -> Vec<Project> {
        self.seen().projects.clone()
    }

    /// Where `folder` is on the server, matched by its git remote, or by its
    /// name when it has none.
    pub fn path_for(&self, folder: &store::Folder, remote: &str) -> Option<String> {
        self.seen()
            .projects
            .iter()
            .find(|p| {
                if remote.is_empty() || p.remote.is_empty() {
                    p.name.eq_ignore_ascii_case(&folder.name)
                } else {
                    brain::place::same_project(&p.remote, remote)
                }
            })
            .map(|p| p.path.clone())
    }

    /// Starts a job there; [`Link::sent`] gives back its id there.
    pub fn send(&self, local: u64, body: Value) {
        let _ = self.tx.send(Ask::Send(local, body));
    }

    /// `POST /v1/jobs/{id}/{action}` there.
    pub fn act(&self, remote: u64, action: &str, body: Value) {
        let _ = self.tx.send(Ask::Act(remote, action.to_string(), body));
    }

    /// The jobs there to keep reading.
    pub fn watch(&self, remote: Vec<u64>) {
        let _ = self.tx.send(Ask::Watch(remote));
    }

    /// Clones `url` into ~/squad/<name> there and adds it as a project.
    pub fn add_project(&self, name: &str, url: &str) {
        let _ = self
            .tx
            .send(Ask::AddProject(name.to_string(), url.to_string()));
    }

    pub fn sent(&self) -> Vec<(u64, Result<u64, String>)> {
        std::mem::take(&mut self.seen().sent)
    }

    pub fn take_jobs(&self) -> HashMap<u64, store::Job> {
        std::mem::take(&mut self.seen().jobs)
    }

    pub fn take_notes(&self) -> Vec<String> {
        std::mem::take(&mut self.seen().notes)
    }
}

fn worker(cfg: Config, seen: Arc<Mutex<Seen>>, rx: Receiver<Ask>) {
    let label = cfg.label();
    let mut tunnel: Option<Tunnel> = None;
    let mut tried: Option<Instant> = None;
    let mut watching: Vec<u64> = Vec::new();
    let mut read_at = Instant::now() - REFRESH;
    let lock = || seen.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        let ask = match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(a) => Some(a),
            Err(RecvTimeoutError::Timeout) => None,
            // The app let go of the link: close the tunnel and stop.
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if let Some(Ask::Watch(ids)) = &ask {
            watching = ids.clone();
        }
        // Keep the tunnel up, trying again now and then when it's down.
        if tunnel.as_mut().is_some_and(|t| !t.alive()) {
            tunnel = None;
        }
        if tunnel.is_none() && tried.is_none_or(|t| t.elapsed() > RETRY) {
            tried = Some(Instant::now());
            match Tunnel::open(&cfg.host) {
                Ok(t) => {
                    tunnel = Some(t);
                    read_at = Instant::now() - REFRESH;
                }
                Err(why) => {
                    let mut s = lock();
                    s.up = false;
                    s.line = format!("Can't reach {label}: {why}");
                }
            }
        }
        let Some(port) = tunnel.as_ref().map(|t| t.port) else {
            if let Some(Ask::Send(local, _)) = ask {
                lock()
                    .sent
                    .push((local, Err(format!("{label} isn't reachable right now"))));
            }
            continue;
        };
        let call = |method: &str, path: &str, body: Option<&Value>| {
            http(port, method, path, &cfg.token, body)
        };
        match ask {
            Some(Ask::Send(local, body)) => {
                let result = match call("POST", "/v1/jobs", Some(&body)) {
                    Ok((200, v)) => v["job"]
                        .as_u64()
                        .ok_or_else(|| "it didn't start a job".to_string()),
                    Ok((_, v)) => Err(error_of(&v)),
                    Err(e) => Err(e),
                };
                if let Ok(id) = result {
                    if !watching.contains(&id) {
                        watching.push(id);
                    }
                }
                lock().sent.push((local, result));
                read_at = Instant::now() - REFRESH;
            }
            Some(Ask::Act(id, action, body)) => {
                let path = format!("/v1/jobs/{id}/{action}");
                match call("POST", &path, Some(&body)) {
                    Ok((200, _)) => {}
                    Ok((_, v)) => lock().notes.push(format!("{label}: {}", error_of(&v))),
                    Err(e) => lock().notes.push(format!("Couldn't reach {label}: {e}")),
                }
                read_at = Instant::now() - REFRESH;
            }
            Some(Ask::AddProject(name, url)) => {
                let note = add_project(&cfg, port, &name, &url);
                lock().notes.push(note);
                read_at = Instant::now() - REFRESH;
            }
            Some(Ask::Watch(_)) | None => {}
        }
        if read_at.elapsed() < REFRESH {
            continue;
        }
        read_at = Instant::now();
        match call("GET", "/v1/state", None) {
            Ok((200, v)) => {
                let engines = v["engines"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|e| e.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                let projects = v["folders"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|f| Project {
                                name: f["name"].as_str().unwrap_or("").to_string(),
                                path: f["path"].as_str().unwrap_or("").to_string(),
                                remote: f["remote"].as_str().unwrap_or("").to_string(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let mut s = lock();
                s.up = true;
                s.line = format!("Connected to {label}.");
                s.engines = engines;
                s.projects = projects;
            }
            Ok((401, _)) => {
                let mut s = lock();
                s.up = false;
                s.line = format!(
                    "{label} doesn't know this computer any more. Disconnect and connect again."
                );
                continue;
            }
            Ok((_, v)) => {
                let mut s = lock();
                s.up = false;
                s.line = format!("{label}: {}", error_of(&v));
                continue;
            }
            Err(e) => {
                let mut s = lock();
                s.up = false;
                s.line = format!("Can't reach {label}: {e}");
                tunnel = None;
                continue;
            }
        }
        for id in watching.clone() {
            if let Ok((200, v)) = call("GET", &format!("/v1/jobs/{id}"), None) {
                if let Ok(job) = serde_json::from_value::<store::Job>(v) {
                    lock().jobs.insert(id, job);
                }
            }
        }
    }
}

/// Clones a project onto the server and adds it there.
fn add_project(cfg: &Config, port: u16, name: &str, url: &str) -> String {
    let label = cfg.label();
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    let dir = format!("squad/{safe}");
    let script = format!(
        "set -e; mkdir -p squad; if [ ! -d {d}/.git ]; then git clone --quiet {u} {d}; fi; cd {d} && pwd",
        d = quote(&dir),
        u = quote(url)
    );
    let path = match ssh(&cfg.host, &script, None) {
        Ok(out) => out.lines().last().unwrap_or("").trim().to_string(),
        Err(why) => return format!("Couldn't copy {name} to {label}: {why}"),
    };
    match http(
        port,
        "POST",
        "/v1/folders",
        &cfg.token,
        Some(&json!({ "path": path })),
    ) {
        Ok((200, _)) => format!("{name} is on {label} now, in {path}. Jobs on it can run there."),
        Ok((_, v)) => format!("{label} wouldn't add {name}: {}", error_of(&v)),
        Err(e) => format!("Couldn't reach {label}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_read_like_ssh_takes_them() {
        assert!(valid_host("anish@203.0.113.7"));
        assert!(valid_host("my-vps"));
        assert!(valid_host("me@[2001:db8::1]"));
        assert!(!valid_host(""));
        assert!(!valid_host("-oProxyCommand=x"));
        assert!(!valid_host("me@host; rm -rf ~"));
        assert_eq!(label_of("anish@vps.example.com"), "vps.example.com");
    }

    #[test]
    fn replies_are_read_back() {
        let raw = b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 15\r\n\r\n{\"error\":\"no\"}";
        let (status, v) = parse_reply(raw).unwrap();
        assert_eq!(status, 401);
        assert_eq!(error_of(&v), "no");
        assert!(parse_reply(b"HTTP/1.1 200").is_err());
    }

    #[test]
    fn quotes_survive_the_shell() {
        assert_eq!(quote("it's"), "'it'\\''s'");
    }
}
