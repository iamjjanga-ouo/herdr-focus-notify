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
    /// The saved SSH machine (`herdr machine add`) whose server hosts the pane;
    /// `None` for a pane of the local server.
    pub(crate) machine: Option<String>,
}

impl FocusNotification {
    /// What the notification's state files and alerter group are keyed by.
    /// Pane ids are per server, so a local pane and a remote one can both be
    /// `w1:p1`: a remote pane's key names its machine, and a local pane's is
    /// its id, as it always was.
    pub(crate) fn state_key(&self) -> String {
        match &self.machine {
            Some(machine) => format!("machine-{machine}:{}", self.pane_id),
            None => self.pane_id.clone(),
        }
    }
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
