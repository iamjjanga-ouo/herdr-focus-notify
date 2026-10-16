#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusNotification {
    pub(crate) pane_id: String,
    pub(crate) status: String,
    pub(crate) title: String,
    pub(crate) body: String,
    /// Secondary line shown under the message (alerter `--subtitle`): the agent,
    /// then the pane's git state once it is known.
    pub(crate) subtitle: Option<String>,
    pub(crate) group: String,
    pub(crate) app_icon: Option<String>,
    /// Bundle id alerter shows the notification as coming from; `None` keeps
    /// alerter's default, Terminal.
    pub(crate) sender: Option<String>,
}

/// What Herdr and git add to a notification about its pane, beyond the event's
/// own copy. Every field is best-effort: one that was not answered in time
/// stays None and leaves the event's text in place.
#[derive(Debug, Default)]
pub(crate) struct PaneLabels {
    pub(crate) workspace: Option<String>,
    pub(crate) tab: Option<String>,
    pub(crate) terminal_title: Option<String>,
    /// The pane's agent as Herdr names it, such as `claude`.
    pub(crate) agent: Option<String>,
    /// For a Claude Code pane, the first line of Claude's latest closing
    /// message: what a finished turn did.
    pub(crate) summary: Option<String>,
    /// The pane's branch and its changes versus `HEAD`: `main* · +120/-45`.
    pub(crate) git: Option<String>,
}
