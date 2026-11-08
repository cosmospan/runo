//! `squad browser …`: the browser tools as shell commands, for engines that can't take
//! Squad's MCP server (Grok, Pi, Antigravity in print mode) and for anyone at a terminal.
//! They act on the browser of the job working in the current folder, the same one the
//! MCP tools and the Browser page use, and print the same replies.

use serde_json::{json, Value};

pub const USAGE: &str = "usage: squad browser <command> [--dir <job folder>]
  open <url>                    open a page (localhost:3000 works)
  look [--find <words>] [--wait-for <words>]
                                read the page as a numbered outline
  click <n | words> [--selector <css>]
                                click control [n], or the one that says <words>
  type <n | words> <value> [--submit] [--selector <css>]
                                type into a field (or pick a select's option)
  press <key>                   Enter, Tab, Escape, ArrowDown…
  back                          go back a page
  screenshot [--full]           save a small JPEG and print where it is
  eval <javascript>             run JavaScript in the page, print its value
  port                          start the browser and print where it listens";

/// Splits `--flag value` and `--switch` options from the words.
fn options(args: &[String]) -> (Vec<String>, Vec<(String, String)>) {
    let mut words = Vec::new();
    let mut opts = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--submit" | "--full" => opts.push((a[2..].to_string(), "true".into())),
            "--dir" | "--find" | "--wait-for" | "--selector" | "--max-chars"
                if i + 1 < args.len() =>
            {
                opts.push((a[2..].to_string(), args[i + 1].clone()));
                i += 1;
            }
            _ => words.push(a.clone()),
        }
        i += 1;
    }
    (words, opts)
}

/// The element a word names: `4` or `[4]` is a control's number, anything else its words.
fn target(word: Option<&String>, selector: Option<&String>) -> Value {
    if let Some(s) = selector {
        return json!({ "selector": s });
    }
    match word.map(|w| w.trim()) {
        Some(w) if w.trim_matches(['[', ']']).parse::<u64>().is_ok() => {
            json!({ "ref": w.trim_matches(['[', ']']) })
        }
        Some(w) if !w.is_empty() => json!({ "text": w }),
        _ => json!({}),
    }
}

/// The tool and arguments a command line asks for, and the folder it's for.
pub fn parse(args: &[String], cwd: &str) -> Result<(String, Value, String), String> {
    let (words, opts) = options(args);
    let opt = |k: &str| opts.iter().find(|(n, _)| n == k).map(|(_, v)| v);
    let dir = opt("dir").cloned().unwrap_or_else(|| cwd.to_string());
    let verb = words.first().map(String::as_str).unwrap_or("");
    let rest = |from: usize| words.get(from..).map(|w| w.join(" ")).unwrap_or_default();
    let (tool, input) = match verb {
        "open" if words.len() > 1 => ("browser_open", json!({ "url": rest(1) })),
        "look" => {
            let mut a = json!({});
            for (k, name) in [("find", "find"), ("wait-for", "wait_for")] {
                if let Some(v) = opt(k) {
                    a[name] = json!(v);
                }
            }
            if let Some(n) = opt("max-chars").and_then(|v| v.parse::<u64>().ok()) {
                a["max_chars"] = json!(n);
            }
            ("browser_look", a)
        }
        // The rest of the line, so `click Sign up` needs no quotes.
        "click" if words.len() > 1 || opt("selector").is_some() => {
            ("browser_click", target(Some(&rest(1)), opt("selector")))
        }
        "type" if words.len() > 2 || (opt("selector").is_some() && words.len() > 1) => {
            let (mut a, from) = match opt("selector") {
                Some(_) => (target(None, opt("selector")), 1),
                None => (target(words.get(1), None), 2),
            };
            a["value"] = json!(rest(from));
            a["submit"] = json!(opt("submit").is_some());
            ("browser_type", a)
        }
        "press" if words.len() > 1 => ("browser_press", json!({ "key": rest(1) })),
        "back" => ("browser_back", json!({})),
        "screenshot" => (
            "browser_screenshot",
            json!({ "full": opt("full").is_some() }),
        ),
        "eval" if words.len() > 1 => ("browser_eval", json!({ "js": rest(1) })),
        "port" => ("port", json!({})),
        _ => return Err(USAGE.into()),
    };
    Ok((tool.to_string(), input, dir))
}

/// Runs `squad browser …` in `cwd`; the text to print, or why it failed.
pub fn main(args: &[String], cwd: &str) -> Result<String, String> {
    let (tool, input, dir) = parse(args, cwd)?;
    if tool == "port" {
        return super::host::ensure(&dir).map(|e| format!("port={}\npath={}", e.port, e.path));
    }
    let reply = super::tools::call(&dir, &tool, &input);
    let mut out = Vec::new();
    for part in reply["content"].as_array().into_iter().flatten() {
        match part["type"].as_str() {
            Some("image") => {
                use base64::Engine as _;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(part["data"].as_str().unwrap_or_default())
                    .map_err(|e| e.to_string())?;
                let file = std::env::temp_dir()
                    .join(format!("squad-browser-{}.jpg", super::host::key(&dir)));
                std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
                out.push(format!("Saved a screenshot to {}", file.display()));
            }
            _ => out.push(part["text"].as_str().unwrap_or_default().to_string()),
        }
    }
    let text = out.join("\n");
    if reply["isError"].as_bool().unwrap_or(false) {
        Err(text)
    } else {
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        s.split(' ').map(str::to_string).collect()
    }

    #[test]
    fn commands_become_tool_calls() {
        let p = |s: &str| parse(&words(s), "/w").unwrap();
        assert_eq!(
            p("open localhost:3000"),
            (
                "browser_open".into(),
                json!({"url": "localhost:3000"}),
                "/w".into()
            )
        );
        assert_eq!(p("click 4").1, json!({"ref": "4"}));
        assert_eq!(p("click [4]").1, json!({"ref": "4"}));
        assert_eq!(p("click Sign up").1, json!({"text": "Sign up"}));
        assert_eq!(
            p("type 2 Ada Lovelace --submit").1,
            json!({"ref": "2", "value": "Ada Lovelace", "submit": true})
        );
        assert_eq!(
            p("type --selector #q hello").1,
            json!({"selector": "#q", "value": "hello", "submit": false})
        );
        assert_eq!(p("look --find cart").1, json!({"find": "cart"}));
        assert_eq!(p("look --dir /job").2, "/job");
        assert_eq!(p("screenshot --full").1, json!({"full": true}));
        assert!(parse(&words("click"), "/w").is_err());
        assert!(parse(&words("fly"), "/w").is_err());
    }
}
