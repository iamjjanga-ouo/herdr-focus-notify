use std::collections::BTreeMap;

use serde::Deserialize;

use crate::icons::agent_icon_path;
use crate::notification::{FocusNotification, PaneLabels};
use crate::session::session_title;
use crate::util::notification_group_id;

#[derive(Debug, Deserialize)]
struct PluginEvent {
    data: Option<EventData>,
}

#[derive(Debug, Deserialize)]
struct EventData {
    pane_id: Option<String>,
    agent_status: Option<String>,
    agent: Option<String>,
    display_agent: Option<String>,
    /// The pane's terminal title: what that agent or shell is working on.
    title: Option<String>,
    /// Labels Herdr reports per status, e.g. `{"blocked": "Needs an answer"}`.
    state_labels: Option<BTreeMap<String, String>>,
}

/// The agent statuses worth notifying about. `blocked` and `done` are the
/// only ones needing the user's action; everything else is noise.
pub(crate) fn status_is_enabled(status: &str) -> bool {
    matches!(status, "blocked" | "done")
}

pub(crate) fn notification_from_event_json(
    json: &str,
) -> Result<Option<FocusNotification>, String> {
    let event: PluginEvent =
        serde_json::from_str(json).map_err(|err| format!("invalid event json: {err}"))?;
    let Some(data) = event.data else {
        return Ok(None);
    };

    let status = data
        .agent_status
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    // Only blocked and done need user action, so they are the only statuses
    // that notify.
    if !status_is_enabled(&status) {
        return Ok(None);
    }

    let Some(pane_id) = pane_id_from_event_data(&data) else {
        return Ok(None);
    };

    let agent = first_non_empty([data.display_agent.as_deref(), data.agent.as_deref()])
        .unwrap_or("Agent")
        .to_string();
    let app_icon = agent_icon_path(&[data.display_agent.as_deref(), data.agent.as_deref()]);

    // The event's task title and status label are what tell two notifications
    // apart; the static copy below is only the fallback for panes that report
    // neither.
    let detail = first_non_empty([
        data.title.as_deref(),
        data.state_labels
            .as_ref()
            .and_then(|labels| labels.get(status.as_str()))
            .map(String::as_str),
    ]);

    let title = match status.as_str() {
        "blocked" => "Blocked",
        "done" => "Done",
        _ => unreachable!("status already filtered"),
    };
    let body = detail.unwrap_or(fallback_body(&status)).to_string();
    let group = notification_group_id(&pane_id);

    Ok(Some(FocusNotification {
        pane_id,
        status,
        title: title.to_string(),
        body,
        subtitle: Some(agent),
        group,
        app_icon,
        sender: None,
    }))
}

fn fallback_body(status: &str) -> &'static str {
    if status == "done" {
        "Open the pane to review the result."
    } else {
        "Open the pane to review and respond."
    }
}

pub(crate) fn focused_pane_id_from_event_json(json: &str) -> Result<Option<String>, String> {
    let event: PluginEvent =
        serde_json::from_str(json).map_err(|err| format!("invalid event json: {err}"))?;

    Ok(event.data.as_ref().and_then(pane_id_from_event_data))
}

fn pane_id_from_event_data(data: &EventData) -> Option<String> {
    trimmed(data.pane_id.as_deref()).map(str::to_string)
}

fn first_non_empty<const N: usize>(values: [Option<&str>; N]) -> Option<&str> {
    values.into_iter().find_map(trimmed)
}

/// Titles a named session `🏁 [claude] - <session name>` (`✋` while blocked);
/// a pane without one keeps the sidebar's state, workspace and tab. The agent
/// and the pane's git state go in the subtitle. Under a session title the
/// message is a finished turn's summary, else the status copy: the event's own
/// message is that same terminal title. Every input is optional, so a failed
/// `herdr` call, or a pane Herdr cannot describe, leaves the event-only message
/// in place.
pub(crate) fn enrich_notification(notification: &mut FocusNotification, labels: &PaneLabels) {
    let session = trimmed(labels.terminal_title.as_deref());
    match session {
        Some(session) => {
            // The event's agent, already in the subtitle, stands in for Herdr's.
            let agent = trimmed(labels.agent.as_deref())
                .map(str::to_string)
                .or_else(|| notification.subtitle.as_deref().map(str::to_lowercase))
                .unwrap_or_else(|| "agent".to_string());
            notification.title = session_title(&notification.status, &agent, session);
        }
        None => {
            for extra in [&labels.workspace, &labels.tab]
                .into_iter()
                .filter_map(|label| trimmed(label.as_deref()))
            {
                notification.title.push_str(" · ");
                notification.title.push_str(extra);
            }
        }
    }

    // The event already put the agent in the subtitle; the git state joins it.
    if let Some(git_label) = trimmed(labels.git.as_deref()) {
        match notification.subtitle.as_mut() {
            Some(subtitle) => {
                subtitle.push_str(" · ");
                subtitle.push_str(git_label);
            }
            None => notification.subtitle = Some(git_label.to_string()),
        }
    }

    if session.is_some() {
        let summary = trimmed(labels.summary.as_deref()).filter(|_| notification.status == "done");
        notification.body = summary
            .unwrap_or(fallback_body(&notification.status))
            .to_string();
    }
}

fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_blocked_notification_from_event() {
        let json = r#"{
            "event": "pane.agent_status_changed",
            "data": {
                "pane_id": "w1:p3",
                "workspace_id": "herdr",
                "agent_status": "blocked",
                "agent": "codex",
                "display_agent": "Codex",
                "title": "Implement plugin",
                "state_labels": {"blocked": "Needs an answer"}
            }
        }"#;

        let notification = notification_from_event_json(json).unwrap().unwrap();

        assert_eq!(notification.pane_id, "w1:p3");
        assert_eq!(notification.status, "blocked");
        assert_eq!(notification.title, "Blocked");
        assert_eq!(notification.body, "Implement plugin");
        assert_eq!(notification.subtitle.as_deref(), Some("Codex"));
        assert_eq!(notification.group, "herdr-w1-p3");
        assert!(notification
            .app_icon
            .as_deref()
            .unwrap()
            .ends_with("/icons/codex-color.png"));
    }

    #[test]
    fn builds_done_notification_from_title() {
        let json = r#"{
            "data": {
                "pane_id": "p1",
                "agent_status": "done",
                "agent": "Codex",
                "title": "Implement plugin"
            }
        }"#;

        let notification = notification_from_event_json(json).unwrap().unwrap();

        assert_eq!(notification.status, "done");
        assert_eq!(notification.title, "Done");
        assert_eq!(notification.body, "Implement plugin");
        assert!(notification.app_icon.is_some());
    }

    #[test]
    fn falls_back_to_static_copy_when_the_event_has_no_details() {
        let json = r#"{
            "data": {
                "pane_id": "p1",
                "agent_status": "blocked",
                "agent": "Codex"
            }
        }"#;

        let notification = notification_from_event_json(json).unwrap().unwrap();

        assert_eq!(notification.title, "Blocked");
        assert_eq!(notification.body, "Open the pane to review and respond.");
    }

    #[test]
    fn falls_back_to_the_status_label_when_the_event_has_no_title() {
        let json = r#"{
            "data": {
                "pane_id": "p1",
                "agent_status": "blocked",
                "agent": "Codex",
                "state_labels": {"blocked": "Needs an answer"}
            }
        }"#;

        let notification = notification_from_event_json(json).unwrap().unwrap();

        assert_eq!(notification.body, "Needs an answer");
    }

    #[test]
    fn enrichment_titles_a_named_session_and_keeps_git_in_the_subtitle() {
        let json = r#"{
            "data": {
                "pane_id": "w1:p3",
                "agent_status": "blocked",
                "agent": "Codex",
                "title": "Tidy up the parser tests"
            }
        }"#;
        let mut notification = notification_from_event_json(json).unwrap().unwrap();

        enrich_notification(
            &mut notification,
            &PaneLabels {
                workspace: Some(" sample-repo ".to_string()),
                tab: Some("status".to_string()),
                terminal_title: Some("Tidy up the parser tests".to_string()),
                agent: Some("codex".to_string()),
                git: Some("main* +120/-45".to_string()),
                ..PaneLabels::default()
            },
        );

        assert_eq!(notification.title, "✋ [codex] - Tidy up the parser tests");
        assert_eq!(
            notification.subtitle.as_deref(),
            Some("Codex · main* +120/-45")
        );
        // The event's message was the session name, which the title now holds.
        assert_eq!(notification.body, "Open the pane to review and respond.");
    }

    #[test]
    fn enrichment_shows_a_finished_turns_summary() {
        let json = r#"{
            "data": {
                "pane_id": "w1:p3",
                "agent_status": "done",
                "agent": "claude",
                "display_agent": "Claude"
            }
        }"#;
        let mut notification = notification_from_event_json(json).unwrap().unwrap();

        enrich_notification(
            &mut notification,
            &PaneLabels {
                terminal_title: Some("Fix the parser".to_string()),
                agent: Some("claude".to_string()),
                summary: Some("Fixed the parser.".to_string()),
                ..PaneLabels::default()
            },
        );

        assert_eq!(notification.title, "🏁 [claude] - Fix the parser");
        assert_eq!(notification.body, "Fixed the parser.");
    }

    #[test]
    fn enrichment_without_a_session_name_keeps_the_sidebar_title() {
        let json = r#"{
            "data": {
                "pane_id": "w1:p3",
                "agent_status": "blocked",
                "agent": "Codex"
            }
        }"#;
        let mut notification = notification_from_event_json(json).unwrap().unwrap();

        enrich_notification(
            &mut notification,
            &PaneLabels {
                workspace: Some(" sample-repo ".to_string()),
                tab: Some("status".to_string()),
                // A blocked pane's summary is not what it is waiting for.
                summary: Some("Earlier answer.".to_string()),
                ..PaneLabels::default()
            },
        );

        assert_eq!(notification.title, "Blocked · sample-repo · status");
        assert_eq!(notification.body, "Open the pane to review and respond.");
    }

    #[test]
    fn enrichment_keeps_the_event_message_when_metadata_is_missing() {
        let json = r#"{
            "data": {
                "pane_id": "w1:p3",
                "agent_status": "blocked",
                "agent": "Codex",
                "title": "Implement plugin"
            }
        }"#;
        let mut notification = notification_from_event_json(json).unwrap().unwrap();

        enrich_notification(
            &mut notification,
            &PaneLabels {
                tab: Some("  ".to_string()),
                terminal_title: Some(String::new()),
                ..PaneLabels::default()
            },
        );

        assert_eq!(notification.title, "Blocked");
        assert_eq!(notification.subtitle.as_deref(), Some("Codex"));
        assert_eq!(notification.body, "Implement plugin");
    }

    #[test]
    fn ignores_other_statuses() {
        let json = r#"{
            "data": {
                "pane_id": "p1",
                "agent_status": "running",
                "agent": "Codex"
            }
        }"#;

        assert!(notification_from_event_json(json).unwrap().is_none());
    }

    #[test]
    fn ignores_missing_pane_id() {
        let json = r#"{
            "data": {
                "agent_status": "blocked",
                "agent": "Codex"
            }
        }"#;

        assert!(notification_from_event_json(json).unwrap().is_none());
    }

    #[test]
    fn extracts_pane_id_from_focus_event() {
        let json = r#"{
            "event": "pane.focused",
            "data": {
                "pane_id": " w1:p2 "
            }
        }"#;

        assert_eq!(
            focused_pane_id_from_event_json(json).unwrap(),
            Some("w1:p2".to_string())
        );
    }
}
