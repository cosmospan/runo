//! The browser tools of Squad's MCP server, as bots see them.

use super::bot::{self, MAX_OUTLINE};
use serde_json::{json, Value};

/// Every browser tool's name.
pub const TOOLS: [&str; 8] = [
    "browser_open",
    "browser_look",
    "browser_click",
    "browser_type",
    "browser_press",
    "browser_back",
    "browser_screenshot",
    "browser_eval",
];

/// Whether there's a browser on this machine to drive.
pub fn offered() -> bool {
    crate::browser_engine::find_chromium().is_some()
}

fn target_props() -> Value {
    json!({
        "ref": { "type": "integer", "description": "The element's [number] from the outline. Best." },
        "text": { "type": "string", "description": "Or words the element shows, like its label." },
        "selector": { "type": "string", "description": "Or a CSS selector." }
    })
}

/// The tools, as MCP lists them.
pub fn list() -> Vec<Value> {
    let with_target = |extra: Value| {
        let mut p = target_props();
        if let (Some(p), Some(e)) = (p.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        p
    };
    vec![
        json!({
            "name": "browser_open",
            "description": "Open a web page in this job's own browser inside Squad Code (headless; the person can watch it and take over there). Use it to try what you built, such as http://localhost:3000, instead of opening an outside browser or installing Playwright. Returns the page as a short text outline: headings, text, and each control with a [number] to click or type into. Errors the page logs and requests that fail are reported too.",
            "inputSchema": {
                "type": "object",
                "properties": { "url": { "type": "string", "description": "Like http://localhost:5173/login, localhost:3000 or https://example.com." } },
                "required": ["url"]
            }
        }),
        json!({
            "name": "browser_look",
            "description": "Read the page open in the job's browser as a text outline, with a [number] on each control. Cheap; prefer it to a screenshot. Pass find to get only the lines with those words, or wait_for to wait up to 15 seconds for words to show up.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "find": { "type": "string", "description": "Only lines containing this." },
                    "wait_for": { "type": "string", "description": "Wait until the page shows this." },
                    "max_chars": { "type": "integer", "description": format!("The most to return; {MAX_OUTLINE} unless set.") }
                }
            }
        }),
        json!({
            "name": "browser_click",
            "description": "Click an element in the job's browser, with a real mouse click in its middle. Name it by ref (its [number] in the outline), text or selector. Returns what changed on the page.",
            "inputSchema": { "type": "object", "properties": target_props() }
        }),
        json!({
            "name": "browser_type",
            "description": "Type into a field in the job's browser, replacing what's in it (or pick an option of a select by its text). Name the field by ref, text (its label) or selector. Set submit to press Enter after. Returns what changed.",
            "inputSchema": {
                "type": "object",
                "properties": with_target(json!({
                    "value": { "type": "string", "description": "What to type, or the option to choose." },
                    "submit": { "type": "boolean", "description": "Press Enter after typing." }
                })),
                "required": ["value"]
            }
        }),
        json!({
            "name": "browser_press",
            "description": "Press a key in the job's browser: Enter, Tab, Escape, Backspace, Delete, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Home, End, PageUp, PageDown or Space. Returns what changed.",
            "inputSchema": {
                "type": "object",
                "properties": { "key": { "type": "string" } },
                "required": ["key"]
            }
        }),
        json!({
            "name": "browser_back",
            "description": "Go back a page in the job's browser.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "browser_screenshot",
            "description": "A small picture of the job's browser. Costs far more than browser_look, so use it only when how the page looks matters (layout, colours, images). Set full for the whole page rather than what's on screen.",
            "inputSchema": {
                "type": "object",
                "properties": { "full": { "type": "boolean" } }
            }
        }),
        json!({
            "name": "browser_eval",
            "description": "Run JavaScript in the page in the job's browser and get its value back as JSON (promises are awaited). For checks the outline can't show, like localStorage or a computed style.",
            "inputSchema": {
                "type": "object",
                "properties": { "js": { "type": "string", "description": "An expression, such as document.querySelectorAll('li').length." } },
                "required": ["js"]
            }
        }),
    ]
}

fn text(t: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": t.into() }] })
}

fn failure(t: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": t.into() }], "isError": true })
}

/// What the element arguments name, as `page.js` takes them.
fn target(args: &Value) -> Value {
    let mut t = json!({});
    if let Some(r) = args.get("ref").filter(|v| !v.is_null()) {
        t["ref"] = match r {
            Value::Number(n) => json!(n.to_string()),
            other => json!(other.as_str().unwrap_or_default().trim_matches(['[', ']'])),
        };
    }
    for k in ["text", "selector"] {
        if let Some(s) = args[k].as_str().filter(|s| !s.trim().is_empty()) {
            t[k] = json!(s);
        }
    }
    t
}

/// Carries out a browser tool for the bot working in `dir`.
pub fn call(dir: &str, name: &str, args: &Value) -> Value {
    let s = |k: &str| args[k].as_str().unwrap_or_default().to_string();
    let got = match name {
        "browser_open" => bot::with(dir, |d| d.open(&s("url"))),
        "browser_look" => {
            let max = args["max_chars"]
                .as_u64()
                .map(|n| n as usize)
                .unwrap_or(MAX_OUTLINE);
            bot::with(dir, |d| d.look(&s("find"), &s("wait_for"), max))
        }
        "browser_click" => bot::with(dir, |d| d.click(&target(args))),
        "browser_type" => {
            let submit = args["submit"].as_bool().unwrap_or(false);
            // `text` names the field; what to type is `value`.
            bot::with(dir, |d| d.type_in(&target(args), &s("value"), submit))
        }
        "browser_press" => bot::with(dir, |d| d.press(&s("key"))),
        "browser_back" => bot::with(dir, |d| d.back()),
        "browser_eval" => bot::with(dir, |d| d.eval_js(&s("js"))),
        "browser_screenshot" => {
            let full = args["full"].as_bool().unwrap_or(false);
            return match bot::with(dir, |d| d.screenshot(full)) {
                Ok((data, head)) => json!({ "content": [
                    { "type": "image", "data": data, "mimeType": "image/jpeg" },
                    { "type": "text", "text": head }
                ]}),
                Err(e) => failure(e),
            };
        }
        _ => return failure(format!("No tool {name}.")),
    };
    match got {
        Ok(t) => text(t),
        Err(e) => failure(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_is_listed() {
        let names: Vec<String> = list()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, TOOLS);
    }

    #[test]
    fn targets_take_a_number_or_a_bracketed_ref() {
        assert_eq!(target(&json!({"ref": 4}))["ref"], "4");
        assert_eq!(target(&json!({"ref": "[4]"}))["ref"], "4");
        assert_eq!(target(&json!({"text": " "})), json!({}));
        assert_eq!(target(&json!({"selector": "#go"}))["selector"], "#go");
    }
}
