//! Read-only sessions on a database. Postgres sessions set
//! `default_transaction_read_only`, so the server refuses any write; SQLite files are
//! opened read-only with `query_only` on. Both have a statement timeout, and the grid's
//! filters become SQL with every value quoted or bound, never pasted in.

use super::{Cell, CellKind, Column, Kind, Rows, Table};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// How long one statement may run before the database stops it.
pub(crate) const TIMEOUT: Duration = Duration::from_secs(30);

/// Longest text kept for one cell; the inspector shows this much.
const CELL_MAX: usize = 4000;

pub(crate) enum Session {
    Postgres(postgres::Client),
    Sqlite(rusqlite::Connection),
}

/// What opening a session learned about the server.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Opened {
    /// "Postgres 16.4" or "SQLite 3.46.0".
    pub server: String,
    /// The server says the session can't write.
    pub read_only: bool,
    /// How the connection travels.
    pub transport: Transport,
    /// Signed in as a role that bypasses every permission check.
    pub superuser: bool,
}

/// How a session reaches its database.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Transport {
    /// A file on this computer.
    #[default]
    File,
    /// TLS with the server's certificate checked.
    Verified,
    /// TLS without checking the certificate.
    Encrypted,
    /// No TLS: fine on this computer, readable on the network elsewhere.
    Plain,
}

/// How long a statement waits for a lock before giving up, so a read never queues behind
/// a migration and holds up everything that queues behind it.
const LOCK_WAIT: Duration = Duration::from_secs(3);

/// `text` with the password in `target` taken out, for errors that might echo it.
pub(crate) fn scrub(text: &str, target: &str) -> String {
    let Ok(url) = url::Url::parse(target) else {
        return text.to_string();
    };
    let Some(pw) = url.password().filter(|p| !p.is_empty()) else {
        return text.to_string();
    };
    let decoded = url::form_urlencoded::parse(format!("p={pw}").as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default();
    let mut out = text.replace(pw, "•••");
    if decoded.len() > 1 {
        out = out.replace(&decoded, "•••");
    }
    out
}

/// `name` as a quoted SQL identifier.
pub(crate) fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// How a column's values look, from its type name.
pub(crate) fn kind_of_type(data_type: &str) -> CellKind {
    let t = data_type.to_ascii_lowercase();
    if t.starts_with("bool") {
        CellKind::Bool
    } else if t.starts_with("json") {
        CellKind::Json
    } else if t == "bytea" || t == "blob" {
        CellKind::Bytes
    } else if [
        "int", "serial", "numeric", "decimal", "real", "double", "float", "money", "bigint",
        "smallint",
    ]
    .iter()
    .any(|n| t.starts_with(n))
    {
        CellKind::Number
    } else {
        CellKind::Text
    }
}

pub(super) fn clip(mut text: String) -> String {
    if text.len() > CELL_MAX {
        let mut end = CELL_MAX;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
    text
}

/// How a Postgres connection uses TLS, following libpq's `sslmode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TlsCheck {
    /// Encrypt when the server offers it, without checking its certificate: libpq's
    /// `prefer` and `require`, and what a database on this machine gets.
    Encrypt,
    /// Encrypt and check the certificate against the system's trusted ones.
    Verify,
}

/// The TLS check for `target` and the URL the driver gets. A database elsewhere is
/// checked unless its URL asks for less, and never falls back to no TLS unless the URL
/// says `sslmode=disable`; the driver only knows `disable`, `prefer` and `require`, so
/// the stricter modes become `require` with the check on.
pub(crate) fn tls_plan(target: &str) -> (TlsCheck, String) {
    let mode = target
        .split(['?', '&'])
        .find_map(|p| p.strip_prefix("sslmode="))
        .unwrap_or("")
        .to_string();
    let (host, _, _) = super::url_parts(target);
    let check = match mode.as_str() {
        "verify-full" | "verify-ca" => TlsCheck::Verify,
        "disable" | "allow" | "prefer" | "require" => TlsCheck::Encrypt,
        _ if super::local_host(&host) => TlsCheck::Encrypt,
        _ => TlsCheck::Verify,
    };
    let remote = !super::local_host(&host);
    let url = match mode.as_str() {
        "verify-full" | "verify-ca" => {
            target.replace(&format!("sslmode={mode}"), "sslmode=require")
        }
        // Elsewhere, "use TLS if offered" would quietly send everything in the clear to
        // whoever answers, so it has to be TLS. `sslmode=disable` still means no TLS.
        "allow" | "prefer" if remote => {
            target.replace(&format!("sslmode={mode}"), "sslmode=require")
        }
        "allow" => target.replace("sslmode=allow", "sslmode=prefer"),
        "" if check == TlsCheck::Verify => {
            let sep = if target.contains('?') { '&' } else { '?' };
            format!("{target}{sep}sslmode=require")
        }
        _ => target.to_string(),
    };
    (check, url)
}

/// Accepts any certificate but still checks the handshake's signatures, as libpq does
/// below `verify-ca`.
#[derive(Debug)]
struct Unchecked(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for Unchecked {
    fn verify_server_cert(
        &self,
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

pub(super) fn tls(check: TlsCheck) -> Result<tokio_postgres_rustls::MakeRustlsConnect, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?;
    let config = match check {
        TlsCheck::Verify => {
            let mut roots = rustls::RootCertStore::empty();
            for cert in rustls_native_certs::load_native_certs().certs {
                let _ = roots.add(cert);
            }
            builder.with_root_certificates(roots).with_no_client_auth()
        }
        TlsCheck::Encrypt => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Unchecked(provider)))
            .with_no_client_auth(),
    };
    Ok(tokio_postgres_rustls::MakeRustlsConnect::new(config))
}

/// A database's own error, in a sentence the place can show.
pub(super) fn pg_error(e: postgres::Error) -> String {
    if let Some(db) = e.as_db_error() {
        let mut msg = db.message().to_string();
        if let Some(hint) = db.hint() {
            msg.push_str(". ");
            msg.push_str(hint);
        }
        return msg;
    }
    let text = e.to_string();
    if text.contains("password authentication failed") {
        "The password was refused.".into()
    } else if text.contains("Connection refused") || text.contains("error connecting to server") {
        "Nothing answered at that address. Is the database running?".into()
    } else if text.contains("invalid peer certificate") || text.contains("UnknownIssuer") {
        "The server's TLS certificate isn't trusted by this computer. Add its CA to your system's certificates, or put sslmode=require in the URL to encrypt without checking it.".into()
    } else if text.contains("TLS handshake") {
        "The TLS handshake failed. If the server doesn't use TLS, put sslmode=disable in the URL."
            .into()
    } else {
        text
    }
}

impl Session {
    /// Open a read-only session on the database at `target`: a URL, or a SQLite file's path.
    pub(crate) fn open(kind: Kind, target: &str) -> Result<(Session, Opened), String> {
        match kind {
            Kind::Postgres => {
                let (check, url) = tls_plan(target);
                let scrubbed = |e: String| scrub(&e, &url);
                let mut config: postgres::Config = url
                    .parse()
                    .map_err(|e: postgres::Error| scrubbed(e.to_string()))?;
                config.application_name("Runo");
                config.connect_timeout(Duration::from_secs(10));
                let mut client = config
                    .connect(tls(check)?)
                    .map_err(|e| scrubbed(pg_error(e)))?;
                client
                    .batch_execute(&format!(
                        "SET default_transaction_read_only = on; SET statement_timeout = {}; SET lock_timeout = {}; SET idle_in_transaction_session_timeout = {}; SET standard_conforming_strings = on;",
                        TIMEOUT.as_millis(),
                        LOCK_WAIT.as_millis(),
                        TIMEOUT.as_millis() * 2,
                    ))
                    .map_err(pg_error)?;
                let read_only = client
                    .simple_query("SHOW transaction_read_only")
                    .map_err(pg_error)?
                    .iter()
                    .find_map(|m| match m {
                        postgres::SimpleQueryMessage::Row(r) => r.get(0).map(|v| v == "on"),
                        _ => None,
                    })
                    .unwrap_or(false);
                let version = client
                    .simple_query("SHOW server_version")
                    .ok()
                    .and_then(|ms| {
                        ms.iter().find_map(|m| match m {
                            postgres::SimpleQueryMessage::Row(r) => r.get(0).map(str::to_string),
                            _ => None,
                        })
                    })
                    .unwrap_or_default();
                let version = version.split_whitespace().next().unwrap_or("").to_string();
                let first = |client: &mut postgres::Client, sql: &str| {
                    client.simple_query(sql).ok().and_then(|ms| {
                        ms.iter().find_map(|m| match m {
                            postgres::SimpleQueryMessage::Row(r) => r.get(0).map(str::to_string),
                            _ => None,
                        })
                    })
                };
                let superuser = first(&mut client, "SHOW is_superuser").as_deref() == Some("on");
                let ssl = first(
                    &mut client,
                    "SELECT ssl::text FROM pg_stat_ssl WHERE pid = pg_backend_pid()",
                );
                let transport = match (ssl.as_deref(), check) {
                    (Some("true"), TlsCheck::Verify) => Transport::Verified,
                    (Some("true"), TlsCheck::Encrypt) => Transport::Encrypted,
                    _ => Transport::Plain,
                };
                Ok((
                    Session::Postgres(client),
                    Opened {
                        server: format!("Postgres {version}").trim().to_string(),
                        read_only,
                        transport,
                        superuser,
                    },
                ))
            }
            Kind::Sqlite => {
                use rusqlite::OpenFlags;
                let path = target
                    .trim_start_matches("sqlite:")
                    .trim_start_matches("file:");
                if !std::path::Path::new(path).is_file() {
                    return Err(format!("There's no file at {path}."));
                }
                let conn = rusqlite::Connection::open_with_flags(
                    path,
                    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )
                .map_err(|e| e.to_string())?;
                conn.pragma_update(None, "query_only", 1)
                    .map_err(|e| e.to_string())?;
                // A file from anywhere is untrusted: its views and triggers can't call
                // functions with side effects, and its schema can't be bent to corrupt
                // memory.
                use rusqlite::config::DbConfig;
                conn.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)
                    .map_err(|e| e.to_string())?;
                conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
                    .map_err(|e| e.to_string())?;
                conn.pragma_update(None, "cell_size_check", 1)
                    .map_err(|e| e.to_string())?;
                conn.busy_timeout(Duration::from_secs(3))
                    .map_err(|e| e.to_string())?;
                let read_only = conn
                    .pragma_query_value(None, "query_only", |r| r.get::<_, i64>(0))
                    .map(|v| v == 1)
                    .unwrap_or(false);
                Ok((
                    Session::Sqlite(conn),
                    Opened {
                        server: format!("SQLite {}", rusqlite::version()),
                        read_only,
                        transport: Transport::File,
                        superuser: false,
                    },
                ))
            }
            other => Err(format!("{} isn't supported yet.", other.label())),
        }
    }

    /// Whether the server ended the session: it restarted, or the network dropped it
    /// while the computer slept.
    pub(crate) fn is_closed(&self) -> bool {
        match self {
            Session::Postgres(client) => client.is_closed(),
            Session::Sqlite(_) => false,
        }
    }

    pub(crate) fn kind(&self) -> Kind {
        match self {
            Session::Postgres(..) => Kind::Postgres,
            Session::Sqlite(_) => Kind::Sqlite,
        }
    }

    /// Every table and view with its columns, keys and estimated size.
    pub(crate) fn tables(&mut self) -> Result<Vec<Table>, String> {
        match self {
            Session::Postgres(client) => pg_tables(client),
            Session::Sqlite(conn) => sqlite_tables(conn).map_err(|e| e.to_string()),
        }
    }
}

/// The rows of a simple query, as cells; `kind` gives each column's look by name.
pub(super) fn pg_text_rows(
    messages: Vec<postgres::SimpleQueryMessage>,
    kind: impl Fn(usize, &str) -> CellKind,
) -> Rows {
    let mut columns: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    for m in messages {
        match m {
            postgres::SimpleQueryMessage::RowDescription(cols) => {
                columns = cols.iter().map(|c| c.name().to_string()).collect();
            }
            postgres::SimpleQueryMessage::Row(r) => {
                if columns.is_empty() {
                    columns = r.columns().iter().map(|c| c.name().to_string()).collect();
                }
                let cells = (0..r.len())
                    .map(|i| match r.get(i) {
                        None => Cell::null(),
                        Some(v) => {
                            let kind = columns.get(i).map(|n| kind(i, n)).unwrap_or(CellKind::Text);
                            let text = if kind == CellKind::Bool {
                                match v {
                                    "t" => "true".into(),
                                    "f" => "false".into(),
                                    v => v.to_string(),
                                }
                            } else {
                                v.to_string()
                            };
                            Cell::text(clip(text), kind)
                        }
                    })
                    .collect();
                rows.push(cells);
            }
            _ => {}
        }
    }
    Rows {
        columns,
        rows,
        ..Rows::default()
    }
}

pub(super) fn pg_rows(
    client: &mut postgres::Client,
    sql: &str,
) -> Result<Vec<Vec<Option<String>>>, String> {
    Ok(client
        .simple_query(sql)
        .map_err(pg_error)?
        .into_iter()
        .filter_map(|m| match m {
            postgres::SimpleQueryMessage::Row(r) => {
                Some((0..r.len()).map(|i| r.get(i).map(str::to_string)).collect())
            }
            _ => None,
        })
        .collect())
}

const PG_SKIP: &str = "n.nspname NOT IN ('pg_catalog', 'information_schema') AND n.nspname NOT LIKE 'pg_toast%' AND n.nspname NOT LIKE 'pg_temp%'";

fn pg_tables(client: &mut postgres::Client) -> Result<Vec<Table>, String> {
    let rels = pg_rows(
        client,
        &format!(
            "SELECT n.nspname, c.relname, c.relkind, c.reltuples::bigint FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relkind IN ('r','p','v','m','f') AND {PG_SKIP} ORDER BY n.nspname <> 'public', n.nspname, c.relname"
        ),
    )?;
    let cols = pg_rows(
        client,
        &format!(
            "SELECT n.nspname, c.relname, a.attname, format_type(a.atttypid, a.atttypmod), (NOT a.attnotnull)::text FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid JOIN pg_namespace n ON n.oid = c.relnamespace WHERE a.attnum > 0 AND NOT a.attisdropped AND c.relkind IN ('r','p','v','m','f') AND {PG_SKIP} ORDER BY n.nspname, c.relname, a.attnum"
        ),
    )?;
    let keys = pg_rows(
        client,
        &format!(
            "SELECT n.nspname, c.relname, con.contype::text, a.attname, fn.nspname, fc.relname, fa.attname FROM pg_constraint con JOIN pg_class c ON c.oid = con.conrelid JOIN pg_namespace n ON n.oid = c.relnamespace CROSS JOIN LATERAL unnest(con.conkey) WITH ORDINALITY AS k(attnum, ord) JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = k.attnum LEFT JOIN pg_class fc ON fc.oid = con.confrelid LEFT JOIN pg_namespace fn ON fn.oid = fc.relnamespace LEFT JOIN pg_attribute fa ON fa.attrelid = con.confrelid AND fa.attnum = con.confkey[k.ord] WHERE con.contype IN ('p','f') AND {PG_SKIP}"
        ),
    )?;
    let s = |v: &Option<String>| v.clone().unwrap_or_default();
    let mut tables: Vec<Table> = rels
        .iter()
        .map(|r| Table {
            schema: s(&r[0]),
            name: s(&r[1]),
            view: matches!(r[2].as_deref(), Some("v" | "m")),
            estimate: r[3]
                .as_deref()
                .and_then(|v| v.parse().ok())
                .filter(|n: &i64| *n >= 0),
            columns: Vec::new(),
        })
        .collect();
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    for (i, t) in tables.iter().enumerate() {
        index.insert((t.schema.clone(), t.name.clone()), i);
    }
    for r in &cols {
        if let Some(&i) = index.get(&(s(&r[0]), s(&r[1]))) {
            tables[i].columns.push(Column {
                name: s(&r[2]),
                data_type: s(&r[3]),
                nullable: r[4].as_deref() == Some("true"),
                ..Column::default()
            });
        }
    }
    for r in &keys {
        let Some(&i) = index.get(&(s(&r[0]), s(&r[1]))) else {
            continue;
        };
        let Some(col) = tables[i].columns.iter_mut().find(|c| c.name == s(&r[3])) else {
            continue;
        };
        match r[2].as_deref() {
            Some("p") => col.primary = true,
            Some("f") => {
                let schema = s(&r[4]);
                let name = if schema == "public" || schema.is_empty() {
                    s(&r[5])
                } else {
                    format!("{schema}.{}", s(&r[5]))
                };
                col.refers = Some((name, s(&r[6])));
            }
            _ => {}
        }
    }
    Ok(tables)
}

fn sqlite_tables(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<Table>> {
    let mut stmt = conn.prepare(
        "SELECT name, type FROM sqlite_schema WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let file_size = conn
        .path()
        .and_then(|p| std::fs::metadata(p).ok())
        .map_or(u64::MAX, |m| m.len());
    let mut tables = Vec::new();
    for (name, kind) in names {
        let mut columns: Vec<Column> = conn
            .prepare(&format!("PRAGMA table_info({})", ident(&name)))?
            .query_map([], |r| {
                Ok(Column {
                    name: r.get(1)?,
                    data_type: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    nullable: r.get::<_, i64>(3)? == 0,
                    primary: r.get::<_, i64>(5)? > 0,
                    refers: None,
                    hidden: false,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let fks: Vec<(String, String, Option<String>)> = conn
            .prepare(&format!("PRAGMA foreign_key_list({})", ident(&name)))?
            .query_map([], |r| Ok((r.get(2)?, r.get(3)?, r.get(4)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (to_table, from, to) in fks {
            if let Some(c) = columns.iter_mut().find(|c| c.name == from) {
                c.refers = Some((to_table, to.unwrap_or_else(|| "rowid".into())));
            }
        }
        // SQLite keeps no row counts; count small files, where it's quick.
        let estimate = if kind == "table" && file_size < 64 << 20 {
            conn.query_row(&format!("SELECT count(*) FROM {}", ident(&name)), [], |r| {
                r.get(0)
            })
            .ok()
        } else {
            None
        };
        tables.push(Table {
            schema: String::new(),
            name,
            view: kind == "view",
            estimate,
            columns,
        });
    }
    Ok(tables)
}

/// One SQLite value as a cell, shown as its column's declared type says.
pub(super) fn sqlite_cell(value: rusqlite::types::ValueRef<'_>, declared: CellKind) -> Cell {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Null => Cell::null(),
        ValueRef::Integer(n) if declared == CellKind::Bool => {
            Cell::text(if n == 0 { "false" } else { "true" }, CellKind::Bool)
        }
        ValueRef::Integer(n) => Cell::text(n.to_string(), CellKind::Number),
        ValueRef::Real(f) => Cell::text(f.to_string(), CellKind::Number),
        ValueRef::Text(t) => {
            let text = String::from_utf8_lossy(t).to_string();
            let kind =
                if declared == CellKind::Json || (text.starts_with('{') && text.ends_with('}')) {
                    CellKind::Json
                } else {
                    CellKind::Text
                };
            Cell::text(clip(text), kind)
        }
        ValueRef::Blob(b) => Cell::text(format!("{} bytes", b.len()), CellKind::Bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_databases_never_drop_to_plain_text() {
        for mode in ["prefer", "allow"] {
            let (check, url) = tls_plan(&format!("postgres://u@db.example.com/app?sslmode={mode}"));
            assert_eq!(check, TlsCheck::Encrypt);
            assert_eq!(url, "postgres://u@db.example.com/app?sslmode=require");
        }
        // Asked for in so many words, it's allowed (and the page says so).
        let (_, url) = tls_plan("postgres://u@db.example.com/app?sslmode=disable");
        assert!(url.ends_with("sslmode=disable"));
        // On this computer, prefer stays prefer.
        let (_, url) = tls_plan("postgres://u@localhost/app?sslmode=prefer");
        assert!(url.ends_with("sslmode=prefer"));
    }

    #[test]
    fn passwords_never_reach_an_error() {
        let url = "postgres://app:s3cr%40t!@db.example.com/app";
        assert_eq!(
            scrub("bad password s3cr%40t! and s3cr@t! here", url),
            "bad password ••• and ••• here"
        );
        assert_eq!(
            scrub("nothing to hide", "postgres://app@db/app"),
            "nothing to hide"
        );
    }

    #[test]
    fn certificates_are_checked_away_from_this_machine() {
        let (check, url) = tls_plan("postgres://u@localhost/app");
        assert_eq!(
            (check, url.as_str()),
            (TlsCheck::Encrypt, "postgres://u@localhost/app")
        );
        let (check, url) = tls_plan("postgres://u@db.example.com/app");
        assert_eq!(check, TlsCheck::Verify);
        assert_eq!(url, "postgres://u@db.example.com/app?sslmode=require");
        let (check, url) = tls_plan("postgres://u@db.example.com/app?sslmode=require");
        assert_eq!(
            (check, url.as_str()),
            (
                TlsCheck::Encrypt,
                "postgres://u@db.example.com/app?sslmode=require"
            )
        );
        let (check, url) = tls_plan("postgres://u@localhost/app?x=1&sslmode=verify-full");
        assert_eq!(
            (check, url.as_str()),
            (
                TlsCheck::Verify,
                "postgres://u@localhost/app?x=1&sslmode=require"
            )
        );
        let (_, url) = tls_plan("postgres://u@db.example.com/app?connect_timeout=5");
        assert!(url.ends_with("connect_timeout=5&sslmode=require"));
    }

    #[test]
    fn types_decide_how_cells_look() {
        assert_eq!(kind_of_type("bigint"), CellKind::Number);
        assert_eq!(kind_of_type("numeric(10,2)"), CellKind::Number);
        assert_eq!(kind_of_type("jsonb"), CellKind::Json);
        assert_eq!(kind_of_type("boolean"), CellKind::Bool);
        assert_eq!(kind_of_type("timestamp with time zone"), CellKind::Text);
        assert_eq!(kind_of_type("character varying(255)"), CellKind::Text);
    }
}
