use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliAction {
    Event,
    Test,
    Help,
    Version,
    Cleanup,
    ClearTerminalBindings,
    CheckPaneVisibility(String),
    FocusPane(String),
    /// A notification click on a pane of a saved SSH machine.
    FocusMachinePane {
        machine: String,
        pane_id: String,
    },
    /// Polls a saved SSH machine's agents and notifies about them.
    WatchMachine {
        machine: String,
        interval: Duration,
    },
}

/// How often `--watch-machine` polls without `--interval`.
const DEFAULT_WATCH_INTERVAL: Duration = Duration::from_secs(3);

pub(crate) fn parse_cli_args<I, S>(args: I) -> Result<CliAction, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut action = CliAction::Event;
    // Modifiers: `--machine` of `--focus-pane`, `--interval` of
    // `--watch-machine`.
    let mut machine = None;
    let mut interval = None;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let arg = arg.as_ref();
        let mut value = || {
            args.next()
                .map(|value| value.as_ref().to_string())
                .ok_or_else(|| format!("missing value after {arg}"))
        };
        match arg {
            "--test" => set_action(&mut action, CliAction::Test, arg)?,
            "-h" | "--help" => set_action(&mut action, CliAction::Help, arg)?,
            "-V" | "--version" => set_action(&mut action, CliAction::Version, arg)?,
            "--cleanup" => set_action(&mut action, CliAction::Cleanup, arg)?,
            "--clear-terminal-bindings" => {
                set_action(&mut action, CliAction::ClearTerminalBindings, arg)?
            }
            "--check-pane-visibility" => {
                let pane_id = value()?;
                set_action(&mut action, CliAction::CheckPaneVisibility(pane_id), arg)?
            }
            "--focus-pane" => {
                let pane_id = value()?;
                set_action(&mut action, CliAction::FocusPane(pane_id), arg)?
            }
            "--watch-machine" => {
                let watch = CliAction::WatchMachine {
                    machine: value()?,
                    interval: DEFAULT_WATCH_INTERVAL,
                };
                set_action(&mut action, watch, arg)?
            }
            "--machine" => machine = Some(value()?),
            "--interval" => interval = Some(parse_interval(&value()?)?),
            _ => {
                return Err(format!(
                    "unknown argument: {arg}; run with --help for usage"
                ));
            }
        }
    }

    match (action, machine, interval) {
        (action, None, None) => Ok(action),
        (CliAction::FocusPane(pane_id), Some(machine), None) => {
            Ok(CliAction::FocusMachinePane { machine, pane_id })
        }
        (CliAction::WatchMachine { machine, .. }, None, Some(interval)) => {
            Ok(CliAction::WatchMachine { machine, interval })
        }
        _ => Err(
            "--machine only goes with --focus-pane, and --interval with --watch-machine"
                .to_string(),
        ),
    }
}

/// Seconds between polls; fractions are allowed.
fn parse_interval(value: &str) -> Result<Duration, String> {
    value
        .parse::<f64>()
        .ok()
        .filter(|secs| secs.is_finite() && *secs > 0.0)
        .map(Duration::from_secs_f64)
        .ok_or_else(|| format!("invalid --interval {value}; expected a positive number of seconds"))
}

fn set_action(action: &mut CliAction, next: CliAction, arg: &str) -> Result<(), String> {
    if *action != CliAction::Event {
        return Err(format!("cannot combine {arg} with another command"));
    }

    *action = next;
    Ok(())
}

pub(crate) fn print_usage() {
    println!(
        "herdr-focus-notify {}\n\nUsage:\n  herdr-focus-notify\n  herdr-focus-notify --test\n  herdr-focus-notify --cleanup\n  herdr-focus-notify --clear-terminal-bindings\n  herdr-focus-notify --watch-machine <label> [--interval <secs>]\n\nOptions:\n  --test                    Send a test focus notification\n  --cleanup                 Remove stale generated state files\n  --clear-terminal-bindings Remove all saved workspace-terminal bindings\n  --watch-machine <label>   Notify about the agents of a saved SSH machine\n                            (herdr machine add); runs until stopped\n  --interval <secs>         Seconds between --watch-machine polls (default 3)\n  -h, --help                Show this help\n  -V, --version\n                            Show the version",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cli_actions() {
        assert_eq!(
            parse_cli_args(Vec::<&str>::new()).unwrap(),
            CliAction::Event
        );
        assert_eq!(parse_cli_args(["--test"]).unwrap(), CliAction::Test);
        assert_eq!(parse_cli_args(["--help"]).unwrap(), CliAction::Help);
        assert_eq!(parse_cli_args(["-h"]).unwrap(), CliAction::Help);
        assert_eq!(parse_cli_args(["--version"]).unwrap(), CliAction::Version);
        assert_eq!(parse_cli_args(["-V"]).unwrap(), CliAction::Version);
        assert_eq!(parse_cli_args(["--cleanup"]).unwrap(), CliAction::Cleanup);
        assert_eq!(
            parse_cli_args(["--clear-terminal-bindings"]).unwrap(),
            CliAction::ClearTerminalBindings
        );
        assert_eq!(
            parse_cli_args(["--check-pane-visibility", "w1:p2"]).unwrap(),
            CliAction::CheckPaneVisibility("w1:p2".to_string())
        );
    }

    #[test]
    fn rejects_unknown_or_combined_cli_args() {
        assert!(parse_cli_args(["--wat"]).is_err());
        assert!(parse_cli_args(["--test", "--help"]).is_err());
        assert!(parse_cli_args(["--check-pane-visibility"]).is_err());
        assert!(parse_cli_args(["--focus-pane", "w1:p1", "--test"]).is_err());
    }

    #[test]
    fn parses_machine_commands() {
        assert_eq!(
            parse_cli_args(["--watch-machine", "idc-vm"]).unwrap(),
            CliAction::WatchMachine {
                machine: "idc-vm".to_string(),
                interval: Duration::from_secs(3),
            }
        );
        assert_eq!(
            parse_cli_args(["--interval", "0.5", "--watch-machine", "idc-vm"]).unwrap(),
            CliAction::WatchMachine {
                machine: "idc-vm".to_string(),
                interval: Duration::from_millis(500),
            }
        );
        assert_eq!(
            parse_cli_args(["--focus-pane", "w1:p1", "--machine", "idc-vm"]).unwrap(),
            CliAction::FocusMachinePane {
                machine: "idc-vm".to_string(),
                pane_id: "w1:p1".to_string(),
            }
        );
        assert_eq!(
            parse_cli_args(["--focus-pane", "w1:p1"]).unwrap(),
            CliAction::FocusPane("w1:p1".to_string())
        );
    }

    #[test]
    fn rejects_misplaced_machine_options() {
        assert!(parse_cli_args(["--watch-machine"]).is_err());
        assert!(parse_cli_args(["--machine", "idc-vm"]).is_err());
        assert!(parse_cli_args(["--interval", "3"]).is_err());
        assert!(parse_cli_args(["--test", "--machine", "idc-vm"]).is_err());
        assert!(parse_cli_args(["--watch-machine", "idc-vm", "--machine", "x"]).is_err());
        assert!(parse_cli_args(["--focus-pane", "w1:p1", "--interval", "3"]).is_err());
        for interval in ["0", "-1", "soon", "NaN", "inf"] {
            assert!(
                parse_cli_args(["--watch-machine", "idc-vm", "--interval", interval]).is_err(),
                "{interval}"
            );
        }
    }
}
