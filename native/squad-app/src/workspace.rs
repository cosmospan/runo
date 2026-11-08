//! Workspace-level features: sidebar status groups, saved layout, quick commands,
//! project notes, the activity feed and playbooks.

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
