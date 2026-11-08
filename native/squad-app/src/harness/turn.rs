//! One running turn of a session: its parser, its event log and how it stands. The app
//! keeps one per session that has an agent running, from the moment the turn starts (or
//! is picked up again after a restart) until its process ends, instead of a map per
//! piece of state.
