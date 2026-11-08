# Contributing

Thanks for helping with Runo. It is a Rust workspace; the app lives in `native/squad-app`.

## Build and run

```bash
cargo run -p squadcode
```

`rust-toolchain.toml` pins the Rust release; rustup installs it the first time you build.

On Linux you may need the UI toolkit's system libraries (libxkbcommon, Wayland or X11, and Vulkan development packages).

## Layout

- `native/squad-app/src/main.rs` - window, sidebar, conversation, composer, right pane
- `native/squad-app/src/settings_ui.rs` - the Settings screen
- `native/squad-app/src/settings.rs` - persisted settings, palette, skills and MCP discovery
- `native/squad-app/src/agent.rs` - launching agent CLIs and shell commands, building their arguments
- `native/squad-app/src/stream.rs` and `native/squad-app/src/harness/` - turning each CLI's output into typed events, and the per-session event log
- `native/squad-app/src/caps.rs` - discovering each CLI's models, modes and levels
- `native/squad-app/src/git.rs` - worktrees, status and diffs
- `native/squad-app/src/store.rs` - projects and sessions on disk
- `vendor/portable-pty` - vendored PTY library, kept for the terminal

## Before you open a pull request

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```

Agent CLIs change their flags between versions. Prefer discovering options from the CLI over hard-coding them, and add a test for any argument mapping you change. When a CLI's output format changes, record a real turn and add it under `native/squad-app/tests/fixtures` (see the README there).
