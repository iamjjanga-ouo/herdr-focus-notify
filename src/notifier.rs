use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::executable::{find_executable, home_dir};
use crate::notification::FocusNotification;
use crate::script::write_focus_script;
use crate::state::reset_notification_clearance;
use crate::util::{notification_group_id, shell_quote};

pub(crate) fn resolve_notifier_bin() -> Result<String, String> {
    find_executable("alerter", alerter_candidate_paths()).ok_or_else(|| {
        "no alerter notifier found; install alerter with `brew install vjeantet/tap/alerter`"
            .to_string()
    })
}

/// Bundle id of the stub `Herdr.app` that `scripts/install-herdr-app.sh`
/// registers, so notifications say "Herdr" instead of alerter's default
/// sender, Terminal.
const HERDR_APP_BUNDLE_ID: &str = "local.herdr.focus-notify";

/// alerter's `--sender`: the stub `Herdr.app` once installed, else `None` for
/// alerter's default.
pub(crate) fn herdr_sender() -> Option<String> {
    let app = home_dir()?.join("Applications/Herdr.app");
    app.is_dir().then(|| HERDR_APP_BUNDLE_ID.to_string())
}

/// Writes the notification's focus script and runs it: detached, or in the
/// foreground for `--test` so a notifier failure surfaces.
pub(crate) fn deliver_notification(
    notification: &FocusNotification,
    herdr_bin: &str,
    notifier_bin: &str,
    monitor_visibility: bool,
    test_mode: bool,
) -> Result<(), String> {
    reset_notification_clearance(&notification.state_key())
        .map_err(|err| format!("failed to reset notification clearance: {err}"))?;

    let script_path = write_focus_script(
        notification,
        herdr_bin,
        notifier_bin,
        monitor_visibility,
        test_mode,
    )
    .map_err(|err| format!("failed to write focus script: {err}"))?;

    send_notification(&script_path, test_mode)
        .map_err(|err| format!("failed to send notification: {err}"))
}

fn send_notification(script_path: &Path, foreground: bool) -> io::Result<()> {
    if foreground {
        run_script_foreground(script_path)
    } else {
        spawn_detached_script(script_path)
    }
}

pub(crate) fn remove_notification(pane_id: &str, notifier_bin: &str) -> io::Result<()> {
    let group = notification_group_id(pane_id);

    let mut command = Command::new(notifier_bin);
    command.arg("--remove").arg(group);
    if let Some(sender) = herdr_sender() {
        command.arg("--sender").arg(sender);
    }
    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(io::Error::other(format!(
            "notification removal exited with {status}"
        ))),
        Err(err) => Err(err),
    }
}

fn alerter_candidate_paths() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/opt/homebrew/bin/alerter"),
        PathBuf::from("/usr/local/bin/alerter"),
    ];
    if let Some(home) = home_dir() {
        paths.push(home.join(".local/bin/alerter"));
    }
    paths
}

fn run_script_foreground(script_path: &Path) -> io::Result<()> {
    match Command::new("sh").arg(script_path).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(io::Error::other(format!(
            "notification script exited with {status}"
        ))),
        Err(err) => Err(err),
    }
}

fn spawn_detached_script(script_path: &Path) -> io::Result<()> {
    let script_str = script_path.to_string_lossy();
    let cmd = format!(
        "nohup sh {} >/dev/null 2>&1 &",
        shell_quote(script_str.as_ref())
    );

    // `sh -c` exits as soon as the script is in the background; waiting for
    // it reaps it, which a long-running `--watch-machine` needs, as it would
    // otherwise keep one zombie per notification.
    Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alerter_candidates_include_common_homebrew_paths() {
        let paths = alerter_candidate_paths();

        assert!(paths.contains(&PathBuf::from("/opt/homebrew/bin/alerter")));
        assert!(paths.contains(&PathBuf::from("/usr/local/bin/alerter")));
    }
}
