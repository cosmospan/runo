//! The Data place's tools for agents, served by Runo's MCP server: a project's database
//! connections, their tables, and read-only queries. Agents get the same read-only
//! sessions people do, never a hidden column's values, a refusal instead of a heavy read
//! on production, and every query goes in the activity log the Data place shows.

use super::engine::Session;
use super::query::{about, check, hidden_columns};
use super::saved::{self, Activity, Saved};
use super::{mask, CellKind, Env, Table};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub(crate) const TOOLS: [&str; 3] = ["data_connections", "data_tables", "data_query"];

/// Rows an agent gets back from one query.
const ROWS: usize = 200;
/// Characters kept of one value.
const CELL: usize = 300;
/// Characters of one answer.
const TEXT: usize = 24_000;

/// Sessions this server opened, by connection id, kept for the next call.
fn sessions() -> &'static Mutex<HashMap<String, Session>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
    SESSIONS.get_or_init(Default::default)
}

/// The connections of project `pid` that agents can open.
fn connections(pid: u64) -> Vec<Saved> {
    saved::load()
        .into_iter()
        .filter(|s| s.project == pid && s.kind.supported())
        .collect()
}

/// Whether to offer the tools: the project has a database in the Data place.
pub(crate) fn offered(pid: u64) -> bool {
    !connections(pid).is_empty()
}

pub(crate) fn tools() -> Vec<Value> {
    let connection = json!({ "type": "string", "description": "The connection's name, as data_connections lists it." });
    vec![
        json!({
            "name": "data_connections",
            "description": "List the databases this project has in Runo's Data place: name, kind, and whether it's local, staging or production. Everything the data tools do is read-only.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "data_tables",
            "description": "A database's tables with their estimated row counts; with `table`, that table's columns, types, primary key and foreign keys. Columns whose values Runo hides (emails, tokens, password hashes and the like) are marked.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "connection": connection,
                    "table": { "type": "string", "description": "A table's name, to describe it." }
                },
                "required": ["connection"]
            }
        }),
        json!({
            "name": "data_query",
            "description": "Run one read-only SQL statement (SELECT, WITH, VALUES, EXPLAIN, SHOW) and get up to 200 rows back. It runs in a read-only transaction that is rolled back, hidden columns come back masked, and on production a statement that would read every row of a big table is refused: add a filter on an indexed column, or a LIMIT.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "connection": connection,
                    "sql": { "type": "string" }
                },
                "required": ["connection", "sql"]
            }
        }),
    ]
}

fn text(t: impl Into<String>) -> Value {
    let mut t: String = t.into();
    if t.len() > TEXT {
        let mut end = TEXT;
        while !t.is_char_boundary(end) {
            end -= 1;
        }
        t.truncate(end);
        t.push_str("\n… (cut short)");
    }
    json!({ "content": [{ "type": "text", "text": t }] })
}

fn failure(t: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": t.into() }], "isError": true })
}

/// Answer a call to one of `TOOLS` for project `pid`, whose folder is `root`.
pub(crate) fn call(pid: u64, root: &str, name: &str, args: &Value) -> Value {
    let all = connections(pid);
    if name == "data_connections" {
        if all.is_empty() {
            return text("This project has no databases in Runo's Data place yet.");
        }
        let lines: Vec<String> = all
            .iter()
            .map(|s| {
                format!(
                    "- {} ({}, {}): {}",
                    s.name,
                    s.kind.label(),
                    s.env.label(),
                    s.shown
                )
            })
            .collect();
        return text(format!(
            "{}\n\nEverything runs read-only.",
            lines.join("\n")
        ));
    }
    let wanted = args["connection"].as_str().unwrap_or("").trim();
    let Some(conn) = all
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(wanted))
        .or_else(|| (all.len() == 1 && wanted.is_empty()).then(|| &all[0]))
    else {
        return failure(format!(
            "No connection called \"{wanted}\". This project has: {}.",
            all.iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    let mut open = sessions().lock().unwrap_or_else(|e| e.into_inner());
    if !open.contains_key(&conn.id) || open.get(&conn.id).is_some_and(|s| s.is_closed()) {
        match open_session(conn, root) {
            Ok(s) => {
                open.insert(conn.id.clone(), s);
            }
            Err(e) => return failure(e),
        }
    }
    let Some(session) = open.get_mut(&conn.id) else {
        return failure("The connection didn't open.");
    };
    match name {
        "data_tables" => match session.tables() {
            Ok(tables) => match args["table"].as_str().filter(|t| !t.trim().is_empty()) {
                Some(t) => match find_table(&tables, t) {
                    Some(table) => text(describe(table)),
                    None => failure(format!("No table called {t}.")),
                },
                None => text(list_tables(&tables)),
            },
            Err(e) => failure(e),
        },
        "data_query" => {
            let sql = args["sql"].as_str().unwrap_or("");
            let mut entry = Activity {
                at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs()),
                conn: conn.id.clone(),
                sql: sql.trim().to_string(),
                rows: 0,
                millis: 0,
                refused: None,
            };
            let answer = query(session, conn, sql, &mut entry);
            saved::log_activity(&entry);
            answer
        }
        other => failure(format!("Unknown tool {other}.")),
    }
}

fn open_session(conn: &Saved, root: &str) -> Result<Session, String> {
    let target = saved::target(conn, Path::new(root))?;
    let (session, opened) = Session::open(conn.kind, &target)?;
    if !opened.read_only {
        return Err(
            "The database didn't confirm a read-only session, so Runo didn't open it.".into(),
        );
    }
    Ok(session)
}

fn query(session: &mut Session, conn: &Saved, sql: &str, entry: &mut Activity) -> Value {
    let checked = match check(conn.kind, sql) {
        Ok(s) => s,
        Err(e) => {
            entry.refused = Some(e.clone());
            return failure(e);
        }
    };
    if conn.env == Env::Prod {
        let tables = session.tables().unwrap_or_default();
        if let Ok(Some(w)) = session.scan_warning(&checked, &tables) {
            let e = format!("Refused on production: {w} Narrow it with a filter on an indexed column, or ask the person to run it in the Data place.");
            entry.refused = Some(e.clone());
            return failure(e);
        }
    }
    let start = Instant::now();
    match session.run(&checked, ROWS) {
        Ok(ran) => {
            entry.rows = ran.rows.rows.len();
            entry.millis = start.elapsed().as_millis();
            let tables = session.tables().unwrap_or_default();
            let hidden = hidden_columns(&ran, &checked, &tables);
            let mut out = String::new();
            out.push_str(&ran.rows.columns.join("\t"));
            out.push('\n');
            for row in &ran.rows.rows {
                let cells: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let v = if c.kind == CellKind::Null {
                            "NULL".to_string()
                        } else if hidden[i] {
                            mask(&c.text)
                        } else {
                            c.text.replace(['\t', '\n'], " ")
                        };
                        v.chars().take(CELL).collect()
                    })
                    .collect();
                out.push_str(&cells.join("\t"));
                out.push('\n');
            }
            let n = ran.rows.rows.len();
            out.push_str(&format!(
                "\n{n} row{}{} in {} ms.",
                if n == 1 { "" } else { "s" },
                if ran.rows.more {
                    format!("; more matched, only the first {ROWS} are shown")
                } else {
                    String::new()
                },
                ran.rows.millis
            ));
            if hidden.iter().any(|h| *h) {
                out.push_str(" Columns that hold personal or secret values come back masked.");
            }
            text(out)
        }
        Err(e) => {
            entry.refused = Some(e.clone());
            failure(e)
        }
    }
}

fn find_table<'a>(tables: &'a [Table], name: &str) -> Option<&'a Table> {
    let name = name.trim().trim_matches('"');
    tables
        .iter()
        .find(|t| t.label().eq_ignore_ascii_case(name))
        .or_else(|| tables.iter().find(|t| t.name.eq_ignore_ascii_case(name)))
}

fn list_tables(tables: &[Table]) -> String {
    let mut out = Vec::new();
    for t in tables {
        out.push(format!(
            "- {}{}: {} columns{}",
            t.label(),
            if t.view { " (view)" } else { "" },
            t.columns.len(),
            t.estimate
                .map(|n| format!(", about {} rows", about(n)))
                .unwrap_or_default()
        ));
    }
    out.join("\n")
}

/// A table's columns, one a line, as an agent reads them.
pub(crate) fn describe(table: &Table) -> String {
    let mut out = vec![format!(
        "{}{}{}",
        table.label(),
        if table.view { " (view)" } else { "" },
        table
            .estimate
            .map(|n| format!(", about {} rows", about(n)))
            .unwrap_or_default()
    )];
    for c in &table.columns {
        let mut notes = Vec::new();
        if c.primary {
            notes.push("primary key".to_string());
        }
        if let Some((t, col)) = &c.refers {
            notes.push(format!("references {t}.{col}"));
        }
        if !c.nullable {
            notes.push("not null".into());
        }
        if c.sensitive() {
            notes.push("values hidden".into());
        }
        out.push(format!(
            "- {} {}{}",
            c.name,
            c.data_type,
            if notes.is_empty() {
                String::new()
            } else {
                format!(" ({})", notes.join(", "))
            }
        ));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Column, Kind};

    #[test]
    fn tables_read_well_for_agents() {
        let t = Table {
            schema: "public".into(),
            name: "orders".into(),
            estimate: Some(5000),
            columns: vec![
                Column {
                    name: "id".into(),
                    data_type: "bigint".into(),
                    primary: true,
                    ..Column::default()
                },
                Column {
                    name: "customer_id".into(),
                    data_type: "bigint".into(),
                    nullable: true,
                    refers: Some(("customers".into(), "id".into())),
                    ..Column::default()
                },
                Column {
                    name: "email".into(),
                    data_type: "text".into(),
                    nullable: true,
                    ..Column::default()
                },
            ],
            view: false,
        };
        assert_eq!(
            describe(&t),
            "orders, about 5.0k rows\n- id bigint (primary key, not null)\n- customer_id bigint (references customers.id)\n- email text (values hidden)"
        );
        assert_eq!(find_table(std::slice::from_ref(&t), "ORDERS"), Some(&t));
        let _ = Kind::Postgres;
    }
}
