//! Ghostty's quick terminal: a floating panel, not a window, so `open -b`
//! cannot bring it forward. With no regular window open, `open -b` makes
//! Ghostty open a new window instead.
//!
//! When Ghostty has no regular window, Herdr is taken to run in the quick
//! terminal: it is shown if hidden and Ghostty is activated, which gives the
//! panel keyboard focus. Its toggle action would hide a panel already shown,
//! so it runs only when no panel-sized Ghostty window is on screen.

use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) const BUNDLE_ID: &str = "com.mitchellh.ghostty";

/// Room for the first run, when macOS may ask whether `osascript` may
/// control Ghostty.
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(5);

/// Prints `raised`, or `no-quick-terminal` when Ghostty is not running or has
/// a regular window. Ghostty's other on-screen windows, such as overlays, are
/// smaller than any quick terminal.
const SHOW_QUICK_TERMINAL_JXA: &str = r#"
function run() {
  ObjC.import("AppKit");
  ObjC.import("CoreGraphics");
  const running = $.NSRunningApplication.runningApplicationsWithBundleIdentifier("com.mitchellh.ghostty");
  if (running.count === 0) return "no-quick-terminal";
  const pid = running.objectAtIndex(0).processIdentifier;
  const ghostty = Application("com.mitchellh.ghostty");
  if (ghostty.windows.length > 0 || ghostty.terminals.length === 0) return "no-quick-terminal";
  const onScreen = ObjC.deepUnwrap(ObjC.castRefToObject(
    $.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, $.kCGNullWindowID)));
  const shown = onScreen.some(w => w.kCGWindowOwnerPID === pid && w.kCGWindowLayer > 0
    && w.kCGWindowBounds.Width >= 200 && w.kCGWindowBounds.Height >= 100);
  if (!shown) ghostty.performAction("toggle_quick_terminal", { on: ghostty.terminals[0] });
  ghostty.activate();
  return "raised";
}
"#;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum QuickTerminal {
    /// Shown and holding keyboard focus.
    Raised,
    /// Ghostty is not running a quick terminal alone; activate it as usual.
    Absent,
    /// The script failed or timed out. The panel may or may not be shown, but
    /// `open -b` must not run: it would open a new window.
    Failed,
}

/// Shows Ghostty's quick terminal and gives it keyboard focus.
pub(crate) fn raise_quick_terminal() -> QuickTerminal {
    let Ok(mut child) = Command::new("/usr/bin/osascript")
        .args(["-l", "JavaScript", "-e", SHOW_QUICK_TERMINAL_JXA])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return QuickTerminal::Failed;
    };

    let deadline = Instant::now() + SCRIPT_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return QuickTerminal::Failed;
            }
        }
    }
    match child.wait_with_output() {
        Ok(output) => outcome(&String::from_utf8_lossy(&output.stdout)),
        Err(_) => QuickTerminal::Failed,
    }
}

fn outcome(stdout: &str) -> QuickTerminal {
    match stdout.trim() {
        "raised" => QuickTerminal::Raised,
        "no-quick-terminal" => QuickTerminal::Absent,
        _ => QuickTerminal::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_script_outcome() {
        assert_eq!(outcome("raised\n"), QuickTerminal::Raised);
        assert_eq!(outcome("no-quick-terminal\n"), QuickTerminal::Absent);
        assert_eq!(outcome(""), QuickTerminal::Failed);
    }
}
