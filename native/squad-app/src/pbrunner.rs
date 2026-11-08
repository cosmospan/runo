//! What's left of the playbook runner from the old Runo window: the background
//! job it set up (a launchd agent, a systemd user timer or a scheduled task)
//! that started `--run-due-playbooks` every few minutes. Playbooks are gone, so
//! Squad Code takes the job away where it finds one.

mod job;

/// Takes the old background job away, if it's installed. Waits for it.
pub fn remove_job() {
    job::remove_now();
}
