//! Takes away the background job the old playbook runner installed: a launchd agent on
//! macOS, a systemd user timer on Linux and a Task Scheduler task on Windows.

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::path::PathBuf;

/// Take the background job away now, if it's installed. Waits for it.
pub fn remove_now() {
    remove();
}

/// Run a helper quietly; Ok when it exits 0, else its error output.
#[cfg(any(target_os = "linux", windows))]
fn quiet(program: &str, args: &[&str]) -> Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashing up from the app.
        cmd.creation_flags(0x0800_0000);
    }
    match cmd.output() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        Err(e) => Err(format!("{program}: {e}")),
    }
}

/// macOS: the launchd agent `dev.runo.playbooks`.
#[cfg(target_os = "macos")]
fn remove() {
    const LABEL: &str = "dev.runo.playbooks";
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let plist = PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"));
    if !plist.exists() {
        return;
    }
    let job = format!("gui/{}/{LABEL}", unsafe { libc::getuid() });
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", job.as_str()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = std::fs::remove_file(&plist);
}

/// Linux: the systemd user units `runo-playbooks.service` and `.timer`.
#[cfg(target_os = "linux")]
fn remove() {
    const UNIT: &str = "runo-playbooks";
    let Some(config) = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|c| !c.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    else {
        return;
    };
    let dir = config.join("systemd/user");
    let (service, timer) = (
        dir.join(format!("{UNIT}.service")),
        dir.join(format!("{UNIT}.timer")),
    );
    if !service.exists() && !timer.exists() {
        return;
    }
    let unit = format!("{UNIT}.timer");
    let _ = quiet("systemctl", &["--user", "disable", "--now", unit.as_str()]);
    let _ = std::fs::remove_file(timer);
    let _ = std::fs::remove_file(service);
    let _ = quiet("systemctl", &["--user", "daemon-reload"]);
}

/// Windows: the scheduled task "Runo playbooks".
#[cfg(windows)]
fn remove() {
    const TASK: &str = "Runo playbooks";
    if quiet("schtasks", &["/Query", "/TN", TASK]).is_err() {
        return;
    }
    let _ = quiet("schtasks", &["/Delete", "/TN", TASK, "/F"]);
    let _ = std::fs::remove_file(crate::store::data_dir().join("playbook-runs/scheduled-task.txt"));
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn remove() {}
