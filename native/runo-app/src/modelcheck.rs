//! Why a turn failed when the agent itself is the problem (signed out, a model the account
//! can't use, a model that needs a newer CLI), and a Test that tries each model with a
//! tiny prompt so the person can see which ones work before picking one.

use std::path::PathBuf;

/// Codex's config file: $CODEX_HOME/config.toml, else ~/.codex/config.toml.
pub fn codex_config() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("CODEX_HOME").filter(|h| !h.is_empty()) {
        return Some(PathBuf::from(home).join("config.toml"));
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| PathBuf::from(h).join(".codex").join("config.toml"))
}

/// The model Codex's config file picks, when it picks one.
pub fn codex_config_model() -> Option<String> {
    let text = std::fs::read_to_string(codex_config()?).ok()?;
    let table: toml::Table = text.parse().ok()?;
    table.get("model")?.as_str().map(str::to_string)
}
