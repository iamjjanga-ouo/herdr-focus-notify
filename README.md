# Herdr Focus Notify

English | [简体中文](README.zh-CN.md)

`herdr-focus-notify` is a macOS plugin for Herdr. It shows a clickable desktop notification when an agent is `blocked` or `done`. Clicking it brings the matching Herdr pane into focus.

It is designed to notify you only when the change is easy to miss: when Herdr is not frontmost, or when you are looking at a different pane.

## Changes in this fork

- **Session-named notifications.** When the pane has a terminal title, which Claude Code sets to the session's title, the notification title reads `🏁 [claude] - <session name>`, or `✋` while the agent waits for you. 🏁 marks a finished turn, not a successful one. For a finished Claude Code turn, the message is the first line of Claude's closing message, read from the session transcript. The subtitle keeps the agent and git state.
- **"Herdr" as the sender.** Run `scripts/install-herdr-app.sh` once to register a stub `~/Applications/Herdr.app`; notifications then say "Herdr" instead of "Terminal". Delete the app to go back.
- **Ghostty quick terminal.** When Ghostty has no regular window, a click shows the quick terminal if it is hidden and gives it keyboard focus, instead of `open -b`, which would open a new Ghostty window.
- **Remote machines.** A watcher on the Mac notifies about the agents of a saved SSH machine as well. See [Remote machines](#remote-machines).

## Herdr compatibility

For **Herdr 0.9.0, use plugin tag `v0.5.0` or later**. The `v0.4.0` tag predates the client focus changes: clicking a notification may activate the terminal without switching to the target pane. Later versions explicitly project the target pane into attached client views.

The minimum supported Herdr version remains `0.7.5`. Workspace-to-terminal bindings are unchanged.

## Quick start

### 1. Install the requirements

- macOS
- Herdr `0.7.5` or later
- [alerter](https://github.com/vjeantet/alerter), which displays the clickable notification

Install alerter:

```bash
brew install vjeantet/tap/alerter
```

### 2. Install the plugin

Install from GitHub:

```bash
herdr plugin install yankewei/herdr-focus-notify
```

Or build and link the local checkout:

```bash
cargo build --release
herdr plugin link .
```

### 3. Done — zero configuration

The plugin works with **zero configuration**. The first time you focus a pane in Herdr, the plugin binds the frontmost terminal to that pane's workspace, then uses it to activate the terminal on click and to recognise when you are already looking at a pane. Bindings are per-workspace: switch from kitty to Ghostty and keep working on the same pane, and clicking a notification activates Ghostty. A notification click never changes the binding, even when the browser or another app is frontmost during the click.

No configuration files needed. The only external dependency is alerter, auto-detected from `PATH` and common Homebrew locations:

```bash
brew install vjeantet/tap/alerter
```

## How notifications behave

By default, `blocked` and `done` status changes can produce a notification. The plugin sends one only when it cannot confirm that you are already looking at that pane.

| Your current view | Notification |
|---|---|
| Another app is frontmost | Sent |
| Herdr is frontmost, but a different pane is focused | Sent |
| Herdr is frontmost and the matching pane is focused | Skipped |
| The terminal bound to the pane's workspace is frontmost and the pane is focused | Skipped (you are looking at Herdr) |
| The focused app cannot be determined | Sent, to avoid missing a change |

Clicking a notification with a saved terminal binding activates that terminal, then sends Herdr's `pane.focus` socket request for the notification's pane. This atomically displays the matching workspace, tab, and pane, including ordinary shell panes without a detected agent. When multiple clients share a server, it switches all of them to that pane. If the workspace has no binding, the click does not activate an app or issue a Herdr focus request; focus the pane manually once in the terminal to establish the binding.

### Multiple terminal windows and tabs

With several windows or tabs open, activating the terminal app alone may bring forward one that is not running Herdr. In supported terminals, a click first selects the window, tab, or split running a Herdr client attached to that session, and raises it together with its OS window. When several clients are attached, the most recently used one is chosen.

| Terminal | Setup |
|---|---|
| iTerm2 | None. The plugin passes the client's `ITERM_SESSION_ID` to iTerm2's built-in reveal URL. |
| kitty | Enable remote control, as shown below. |

```conf
# kitty.conf (restart kitty afterwards)
allow_remote_control socket-only
listen_on unix:/tmp/kitty
```

In other terminals, or in kitty without these settings, the click only activates the terminal app, and macOS decides which window comes forward.

The notification is laid out like the Agent sidebar's own rows: the title is `{state} · {workspace} · {tab}`, the subtitle is the agent followed by the pane's git state, and the message is the pane's terminal title. The git state uses the short form shell prompts and diffstats share: `main* · +120/-45` means branch `main`, uncommitted changes, 120 lines inserted and 45 deleted versus `HEAD` (untracked files are not counted). Changes that touch no text lines, such as a binary file or an executable bit, show as `main*` alone. A branch long enough to push the counts off the subtitle's one line is middle-truncated. When Herdr reports no title, the message falls back to the status: blocked agents prompt you to review and respond, done agents prompt you to review the result. The plugin never reads or summarizes pane contents, and never asks Herdr to explain its detection.

When you manually focus the matching pane in Herdr while its terminal is frontmost, its pending notification is removed.

If the pane was already active when the notification arrived, returning to that terminal removes it within a few seconds.

## How it stays quiet

- Notifications only fire for `blocked` and `done` — the two statuses that actually need you.
- A notification is skipped when the pane is already focused **and** the frontmost app is the terminal bound to its workspace (learned automatically).
- If you are elsewhere when the pane becomes active, the notification auto-removes within a few seconds once you switch back to the bound terminal.

The `--test` action sends a real test notification (capped at 10 seconds) so you can verify the whole pipeline.

## Remote machines

Herdr runs a plugin's event hooks on the server that hosts the panes. The agents of a saved SSH machine (`herdr machine add`) therefore never reach the plugin on your Mac, even though the Herdr client shows them. A watcher on the Mac covers them instead. Nothing is installed on the machine.

Run this from the checkout Herdr builds the plugin from, with the machine's label:

```bash
cargo build --release
scripts/install-machine-watcher.sh idc-vm
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/local.herdr.focus-notify.watch.idc-vm.plist
```

The script writes a LaunchAgent that runs `target/release/herdr-focus-notify --watch-machine idc-vm`, restarts it if it exits, and logs to `~/Library/Logs/herdr-focus-notify/watch-idc-vm.log`. After a rebuild, restart it with `launchctl kickstart -k gui/$(id -u)/local.herdr.focus-notify.watch.idc-vm`. To remove it, run `launchctl bootout gui/$(id -u)/local.herdr.focus-notify.watch.idc-vm`, then `scripts/install-machine-watcher.sh idc-vm --remove`.

Every 3 seconds (`--interval <secs>` changes this), the watcher runs `herdr --machine idc-vm agent list` and notifies when an agent turns `blocked` or `done`. The machine's server counts its focused pane as seen, so a turn that ends there goes to `idle` instead of `done`, even while the client shows another machine; the watcher notifies an `idle` reached through `working` as `done` too. Agents already blocked or done when the watcher starts do not notify. While the machine cannot be reached, the watcher logs once and keeps trying; a change made in the meantime notifies once the machine answers again.

The notification looks like a local one, with the machine next to the agent in the subtitle: `claude @ idc-vm`. Clicking it brings forward the terminal Herdr runs in (Ghostty's quick terminal included), then focuses the pane on the machine with `herdr --machine idc-vm agent focus`.

Limits:

- A change shows up to one interval late, plus the SSH round trip.
- Herdr has no command to switch the client to another machine. If Herdr shows Local when you click, select the machine in Herdr; the pane is already focused there.
- Remote notifications are always sent, even when you are looking at the pane, because the plugin cannot tell which machine the client shows. Nothing removes them automatically while you look at the pane.
- There is no Claude Code turn summary, git state, or workspace and tab label: those come from files and commands on the machine.

## Troubleshooting

| Problem | What to check |
|---|---|
| No notification appears | Make sure `alerter` is installed and executable; it is auto-detected from `PATH` and common Homebrew locations (`brew install vjeantet/tap/alerter`). |
| Click brings the right terminal forward, but not the window or tab running Herdr | Window and tab selection works only in iTerm2 and in kitty with remote control enabled (see [Multiple terminal windows and tabs](#multiple-terminal-windows-and-tabs)). Other terminals only get app-level activation. |
| Click does not bring forward the expected terminal | Use the **Clear saved terminal bindings** plugin action, then focus a pane once in the expected terminal. |
| A workspace has a stale terminal binding | Use the **Clear saved terminal bindings** plugin action, then focus a pane once in the expected terminal. |
| Notifications appear while you are viewing Herdr | You were not in the workspace's bound terminal at that moment; the plugin errs on the side of notifying rather than missing a state change. |
| Need diagnostic information | Run the plugin with `--test` or `--check-pane-visibility <pane_id>` to exercise the pipeline and focus checks directly. |
| No notification from a remote machine | Read `~/Library/Logs/herdr-focus-notify/watch-<label>.log`, and run `herdr machine status <label>` to see whether the machine answers. |

## Bundled icons

Recognised agent names use bundled local icons, including Codex, Claude Code, Cursor, Gemini, GitHub Copilot, DeepSeek, Qwen, Kimi, OpenCode, OpenHands, Cline, Windsurf, Devin, omp, pi, and v0.

The icons are vendored from `@lobehub/icons-static-png` under the MIT license, except `omp.png` and `pi.png`, which use the official logos of Oh My Pi and the Pi coding agent. See `assets/icons/NOTICE.md`.
