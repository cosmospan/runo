//! Recording agents' raw output for fixtures. With `SQUADCODE_RECORD=<folder>` set, every line
//! a structured agent prints in a turn is copied to `<folder>/<agent>-<session>-<turn>.jsonl`
//! as it arrives, untouched, ready to become a contract test under `tests/fixtures` once
//! home folders and accounts are taken out.
