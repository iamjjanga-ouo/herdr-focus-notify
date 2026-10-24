//! Notifications for the agents of a saved SSH machine (`herdr machine add`).
//!
//! Herdr runs a plugin's event hooks on the server that hosts the panes, so a
//! machine's agents never reach the plugin installed on this Mac.
//! `--watch-machine` polls `herdr --machine <label> agent list` instead and
//! notifies when an agent turns `blocked` or `done`, through the same focus
//! scripts as a local event. Nothing is installed on the machine.

use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::event::{
    enrich_notification, notification_from_event_data, status_is_enabled, EventData,
};
use crate::executable::resolve_herdr_bin;
use crate::focus::{herdr_result, show_terminal};
use crate::notification::{FocusNotification, PaneLabels};
use crate::notifier::{deliver_notification, herdr_sender, resolve_notifier_bin};
use crate::util::{command_stdout_until, notification_group_id};

/// How long one `herdr --machine` call may take. It goes over SSH, and a
/// machine that stops answering must not stall the watcher.
const MACHINE_CALL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct AgentListResult {
    agents: Vec<RemotePane>,
}

/// One entry of `herdr agent list`: the fields a status-change event carries,
/// and the counter Herdr bumps on every status change.
#[derive(Debug, Deserialize)]
struct RemotePane {
    #[serde(flatten)]
    event: EventData,
    #[serde(default)]
    state_change_seq: u64,
    terminal_title_stripped: Option<String>,
}

impl RemotePane {
    fn state(&self) -> PaneState {
        (self.state_change_seq, self.event.status())
    }
}

/// A pane's status change counter and status.
type PaneState = (u64, String);

/// Polls `machine` every `interval` until the process is stopped. Only a
/// missing or unusable `herdr` returns.
pub(crate) fn watch(machine: &str, interval: Duration) -> Result<(), String> {
    let herdr_bin = resolve_herdr_bin()?;
    eprintln!("herdr-focus-notify: watching machine {machine} every {interval:?}");
    let mut watcher = Watcher::default();
    loop {
        for pane in watcher.on_poll(machine, poll(machine, &herdr_bin)) {
            if let Err(err) = notify(machine, &pane, &herdr_bin) {
                eprintln!("herdr-focus-notify: {machine}: {err}");
            }
        }
        thread::sleep(interval);
    }
}

/// The machine's agents; None when the machine cannot be reached or its
/// answer cannot be read.
fn poll(machine: &str, herdr_bin: &str) -> Option<Vec<RemotePane>> {
    let json = command_stdout_until(
        herdr_bin,
        &["--machine", machine, "agent", "list"],
        &[],
        Instant::now() + MACHINE_CALL_TIMEOUT,
    )?;
    Some(herdr_result::<AgentListResult>(&json)?.agents)
}

#[derive(Default)]
struct Watcher {
    /// Each pane's state at the last good poll. None until the first good
    /// poll, the baseline: whatever is `blocked` or `done` by then was not seen
    /// happening.
    last: Option<HashMap<String, PaneState>>,
    /// Whether the previous poll failed, so a run of failures is logged once.
    failing: bool,
}

impl Watcher {
    /// The panes to notify about after a poll: those that are `blocked` or
    /// `done` with a state the last good poll did not see, including a pane it
    /// did not see at all. A `None` poll failed and changes nothing, so the
    /// next good poll is compared with the last good one.
    fn on_poll(&mut self, machine: &str, panes: Option<Vec<RemotePane>>) -> Vec<RemotePane> {
        let Some(panes) = panes else {
            if !std::mem::replace(&mut self.failing, true) {
                eprintln!(
                    "herdr-focus-notify: cannot list the agents of machine {machine}; \
                     retrying until it answers (`herdr machine status {machine}` tells why)"
                );
            }
            return Vec::new();
        };
        if std::mem::take(&mut self.failing) {
            eprintln!("herdr-focus-notify: machine {machine} answers again");
        }

        let current = panes
            .iter()
            .filter_map(|pane| Some((pane.event.pane_id()?.to_string(), pane.state())))
            .collect();
        // Panes gone since the last poll are forgotten along with it.
        let Some(last) = self.last.replace(current) else {
            return Vec::new();
        };
        panes
            .into_iter()
            .filter(|pane| {
                let state = pane.state();
                pane.event.pane_id().is_some_and(|pane_id| {
                    status_is_enabled(&state.1) && last.get(pane_id) != Some(&state)
                })
            })
            .collect()
    }
}

/// Sends the notification for a remote pane. It is never skipped: the plugin
/// cannot tell which machine the Herdr client shows, so a pane its server calls
/// focused may well be out of sight.
fn notify(machine: &str, pane: &RemotePane, herdr_bin: &str) -> Result<(), String> {
    let Some(notification) = remote_notification(machine, pane) else {
        return Ok(());
    };
    let notifier_bin = resolve_notifier_bin()?;
    deliver_notification(&notification, herdr_bin, &notifier_bin, false, false)
}

/// A remote pane's notification, titled like a local one, with the machine
/// next to the agent in the subtitle. What only works for a local pane is left
/// out: the Claude Code transcript and the git state live on the machine, and
/// the visibility monitor asks the local server.
fn remote_notification(machine: &str, pane: &RemotePane) -> Option<FocusNotification> {
    let mut notification = notification_from_event_data(&pane.event)?;
    notification.machine = Some(machine.to_string());
    notification.group = notification_group_id(&notification.state_key());
    enrich_notification(
        &mut notification,
        &PaneLabels {
            terminal_title: pane.terminal_title_stripped.clone(),
            agent: pane.event.agent.clone(),
            ..PaneLabels::default()
        },
    );
    let agent = notification.subtitle.as_deref().unwrap_or("Agent");
    notification.subtitle = Some(format!("{agent} @ {machine}"));
    notification.sender = herdr_sender();
    Some(notification)
}

/// A click on a remote pane's notification: shows the terminal running Herdr,
/// then focuses the pane on its machine. Herdr has no command that switches the
/// client to another machine, so a client showing another one keeps it.
pub(crate) fn focus_pane(machine: &str, pane_id: &str) -> Result<(), String> {
    let herdr_bin = resolve_herdr_bin()?;
    if let Some(terminal) = crate::state::herdr_terminal() {
        show_terminal(&terminal, None)?;
    }
    command_stdout_until(
        &herdr_bin,
        &["--machine", machine, "agent", "focus", pane_id],
        &[],
        Instant::now() + MACHINE_CALL_TIMEOUT,
    )
    .map(|_| ())
    .ok_or_else(|| format!("failed to focus {pane_id} on machine {machine}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(pane_id: &str, status: &str, seq: u64) -> RemotePane {
        serde_json::from_value(serde_json::json!({
            "pane_id": pane_id,
            "agent": "claude",
            "agent_status": status,
            "state_change_seq": seq,
        }))
        .unwrap()
    }

    fn ids(panes: &[RemotePane]) -> Vec<&str> {
        panes
            .iter()
            .map(|pane| pane.event.pane_id().unwrap())
            .collect()
    }

    #[test]
    fn reads_the_agent_list() {
        // Trimmed from a real `herdr agent list` reply.
        let json = r#"{"id":"cli:agent:list","result":{"agents":[
            {"agent":"claude","agent_session":{"agent":"claude","kind":"id","source":"herdr:claude","value":"33821529-a846-4b27-8c59-e690b564302b"},
             "agent_status":"idle","cwd":"/home/me/repo","focused":false,"pane_id":"w5:pE","revision":1,
             "state_change_seq":29,"tab_id":"w5:t7","terminal_id":"term_65e90e6f5ba822",
             "terminal_title":"✳ Fix the parser","terminal_title_stripped":"Fix the parser","workspace_id":"w5"},
            {"agent":"codex","agent_status":"blocked","focused":true,"pane_id":"w5:pG","tab_id":"w5:t9","workspace_id":"w5"}]}}"#;

        let panes = herdr_result::<AgentListResult>(json).unwrap().agents;

        assert_eq!(ids(&panes), ["w5:pE", "w5:pG"]);
        assert_eq!(panes[0].state(), (29, "idle".to_string()));
        assert_eq!(
            panes[0].terminal_title_stripped.as_deref(),
            Some("Fix the parser")
        );
        // A server that reports no counter still reports the status.
        assert_eq!(panes[1].state(), (0, "blocked".to_string()));
    }

    #[test]
    fn the_first_poll_is_a_baseline() {
        let mut watcher = Watcher::default();

        let changed = watcher.on_poll("vm", Some(vec![pane("w1:p1", "blocked", 4)]));

        assert!(changed.is_empty());
        // Still blocked, and nothing new happened.
        assert!(watcher
            .on_poll("vm", Some(vec![pane("w1:p1", "blocked", 4)]))
            .is_empty());
    }

    #[test]
    fn notifies_panes_that_turn_blocked_or_done() {
        let mut watcher = Watcher::default();
        watcher.on_poll(
            "vm",
            Some(vec![
                pane("w1:p1", "working", 1),
                pane("w1:p2", "working", 1),
                pane("w1:p3", "idle", 1),
            ]),
        );

        let changed = watcher.on_poll(
            "vm",
            Some(vec![
                pane("w1:p1", "blocked", 2),
                pane("w1:p2", "done", 2),
                pane("w1:p3", "working", 2),
                // Appeared since the last poll, already waiting for an answer.
                pane("w2:p1", "blocked", 1),
            ]),
        );

        assert_eq!(ids(&changed), ["w1:p1", "w1:p2", "w2:p1"]);
    }

    #[test]
    fn the_same_status_counts_again_only_with_a_new_counter() {
        let mut watcher = Watcher::default();
        watcher.on_poll("vm", Some(vec![pane("w1:p1", "working", 1)]));
        assert_eq!(
            ids(&watcher.on_poll("vm", Some(vec![pane("w1:p1", "done", 2)]))),
            ["w1:p1"]
        );
        assert!(watcher
            .on_poll("vm", Some(vec![pane("w1:p1", "done", 2)]))
            .is_empty());

        // Prompted and finished again between two polls.
        assert_eq!(
            ids(&watcher.on_poll("vm", Some(vec![pane("w1:p1", "done", 4)]))),
            ["w1:p1"]
        );
    }

    #[test]
    fn a_pane_that_disappears_is_forgotten() {
        let mut watcher = Watcher::default();
        watcher.on_poll("vm", Some(vec![pane("w1:p1", "blocked", 3)]));

        assert!(watcher.on_poll("vm", Some(Vec::new())).is_empty());
        assert!(watcher.last.as_ref().unwrap().is_empty());
        // Back with the same state, it is a new pane.
        assert_eq!(
            ids(&watcher.on_poll("vm", Some(vec![pane("w1:p1", "blocked", 3)]))),
            ["w1:p1"]
        );
    }

    #[test]
    fn failed_polls_change_nothing() {
        let mut watcher = Watcher::default();

        // A failure is no baseline: the first good poll is.
        assert!(watcher.on_poll("vm", None).is_empty());
        assert!(watcher.failing);
        assert!(watcher
            .on_poll("vm", Some(vec![pane("w1:p1", "blocked", 1)]))
            .is_empty());
        assert!(!watcher.failing);

        // A failure is no empty machine either: the pane is not forgotten, so
        // its unchanged state does not notify again.
        assert!(watcher.on_poll("vm", None).is_empty());
        assert!(watcher.on_poll("vm", None).is_empty());
        assert!(watcher
            .on_poll("vm", Some(vec![pane("w1:p1", "blocked", 1)]))
            .is_empty());

        // A change made while the machine was unreachable still counts.
        assert!(watcher.on_poll("vm", None).is_empty());
        assert_eq!(
            ids(&watcher.on_poll("vm", Some(vec![pane("w1:p1", "done", 3)]))),
            ["w1:p1"]
        );
    }

    #[test]
    fn remote_notifications_name_the_machine() {
        let mut blocked = pane("w1:p1", "blocked", 2);
        blocked.terminal_title_stripped = Some("Fix the parser".to_string());

        let notification = remote_notification("idc-vm", &blocked).unwrap();

        assert_eq!(notification.title, "✋ [claude] - Fix the parser");
        assert_eq!(notification.subtitle.as_deref(), Some("claude @ idc-vm"));
        assert_eq!(notification.body, "Open the pane to review and respond.");
        assert_eq!(notification.machine.as_deref(), Some("idc-vm"));
        // The local pane `w1:p1` keeps its own group and state files.
        assert_eq!(notification.group, "herdr-machine-idc-vm-w1-p1");
        let local = notification_from_event_data(&blocked.event).unwrap();
        assert_eq!(local.group, "herdr-w1-p1");
        assert_ne!(notification.state_key(), local.state_key());
        assert_ne!(
            crate::state::cleared_notification_marker_path(&notification.state_key()),
            crate::state::cleared_notification_marker_path(&local.state_key())
        );

        // Without a terminal title, the status names the notification; there
        // is no transcript summary to read.
        let done = remote_notification("idc-vm", &pane("w1:p2", "done", 3)).unwrap();
        assert_eq!(done.title, "Done");
        assert_eq!(done.body, "Open the pane to review the result.");

        assert!(remote_notification("idc-vm", &pane("w1:p3", "working", 3)).is_none());
    }
}
