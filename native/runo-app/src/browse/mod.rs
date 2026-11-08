//! Browser use inside Squad Code: each job gets its own headless browser (see `host`),
//! bots drive it through Squad's MCP tools (`tools`, `bot`), and the person watches and
//! uses the same page live in Squad (`view`). Nothing opens a window outside Squad.

pub mod bot;
pub mod cdp;
pub mod cli;
pub mod host;
pub mod tools;
pub mod view;
pub mod ws;
