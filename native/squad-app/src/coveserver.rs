//! Setting a Cove server up from Runo, given only its SSH login: Runo checks the login,
//! installs or updates Cove there, keeps an SSH tunnel to it open, pairs with it, and
//! then offers to sign the agents in.
//!
//! Cove's API stays on the server's 127.0.0.1, so nothing new is opened to the internet:
//! Runo reaches it through `ssh -L`, which it starts again whenever the tunnel is gone
//! (after sleep or a network change).
