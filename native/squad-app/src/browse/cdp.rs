//! A DevTools protocol connection to a browser's own endpoint, with pages reached
//! through flattened sessions. Replies go back to whoever asked; events go to one
//! listener, on the reader thread.

use super::ws::{self, WsWriter};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Who waits for which reply.
type Replies = Arc<Mutex<HashMap<u64, Sender<Result<Value, String>>>>>;

/// Called with every event: its method, params and session.
pub type Listener = Box<dyn Fn(&str, &Value, &str) + Send>;

pub struct Cdp {
    writer: WsWriter,
    next: AtomicU64,
    replies: Replies,
    closed: Arc<AtomicBool>,
}

impl Cdp {
    /// Connects to the browser at `port`, at `path` (its `/devtools/browser/…`).
    pub fn connect(port: u16, path: &str, listener: Listener) -> Result<Arc<Cdp>, String> {
        let (writer, mut reader) = ws::connect(port, path)?;
        let replies: Replies = Arc::default();
        let closed = Arc::new(AtomicBool::new(false));
        let (r, done) = (replies.clone(), closed.clone());
        std::thread::Builder::new()
            .name("squad-cdp".into())
            .spawn(move || {
                while let Some(text) = reader.next() {
                    let Ok(msg) = serde_json::from_str::<Value>(&text) else {
                        continue;
                    };
                    if let Some(id) = msg["id"].as_u64() {
                        let tx = r.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                        if let Some(tx) = tx {
                            let got = match msg.get("error") {
                                Some(e) => {
                                    Err(e["message"].as_str().unwrap_or("failed").to_string())
                                }
                                None => Ok(msg["result"].clone()),
                            };
                            let _ = tx.send(got);
                        }
                        continue;
                    }
                    if let Some(method) = msg["method"].as_str() {
                        listener(
                            method,
                            &msg["params"],
                            msg["sessionId"].as_str().unwrap_or_default(),
                        );
                    }
                }
                done.store(true, Ordering::Relaxed);
                // Whoever still waits hears that the browser went away.
                r.lock().unwrap_or_else(|e| e.into_inner()).clear();
            })
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(Cdp {
            writer,
            next: AtomicU64::new(1),
            replies,
            closed,
        }))
    }

    pub fn alive(&self) -> bool {
        !self.closed.load(Ordering::Relaxed)
    }

    fn message(id: u64, method: &str, params: Value, session: &str) -> String {
        let mut msg = json!({"id": id, "method": method, "params": params});
        if !session.is_empty() {
            msg["sessionId"] = json!(session);
        }
        msg.to_string()
    }

    /// Sends a command without waiting for its answer.
    pub fn send(&self, method: &str, params: Value, session: &str) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let _ = self
            .writer
            .text(&Self::message(id, method, params, session));
    }

    /// Sends a command and waits up to `wait` for its result.
    pub fn call_for(
        &self,
        method: &str,
        params: Value,
        session: &str,
        wait: Duration,
    ) -> Result<Value, String> {
        if !self.alive() {
            return Err("the browser closed".into());
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        self.replies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, tx);
        self.writer
            .text(&Self::message(id, method, params, session))?;
        match rx.recv_timeout(wait) {
            Ok(got) => got,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                self.replies
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                Err(format!("{method} took too long"))
            }
            Err(_) => Err("the browser closed".into()),
        }
    }

    pub fn call(&self, method: &str, params: Value, session: &str) -> Result<Value, String> {
        self.call_for(method, params, session, Duration::from_secs(15))
    }

    pub fn close(&self) {
        self.writer.close();
    }
}

impl Drop for Cdp {
    fn drop(&mut self) {
        self.writer.close();
    }
}

/// The page to drive or watch: the first open page, made when there's none. It's the
/// one page the bot and the person share. Returns its target id.
pub fn the_page(cdp: &Cdp) -> Result<String, String> {
    let targets = cdp.call("Target.getTargets", json!({}), "")?;
    let page = targets["targetInfos"].as_array().and_then(|list| {
        list.iter()
            .find(|t| t["type"] == "page")
            .and_then(|t| t["targetId"].as_str())
            .map(str::to_string)
    });
    match page {
        Some(id) => Ok(id),
        None => cdp.call("Target.createTarget", json!({"url": "about:blank"}), "")?["targetId"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "the browser didn't open a page".into()),
    }
}

/// Attaches to `target` and gives back the session to send its commands to.
pub fn attach(cdp: &Cdp, target: &str) -> Result<String, String> {
    cdp.call(
        "Target.attachToTarget",
        json!({"targetId": target, "flatten": true}),
        "",
    )?["sessionId"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "couldn't attach to the page".into())
}

/// Sizes the page to what bots and people see, `host::WIDTH` by `host::HEIGHT`. The
/// window's own size leaves the page whatever its frame doesn't take, which differs
/// between browsers; every session that attaches sets this, so all see the same.
pub fn fit(cdp: &Cdp, session: &str) {
    cdp.send(
        "Emulation.setDeviceMetricsOverride",
        json!({"width": super::host::WIDTH, "height": super::host::HEIGHT,
            "deviceScaleFactor": 1, "mobile": false}),
        session,
    );
}
