#!/bin/sh
# Notifies about the agents of a saved SSH machine (`herdr machine add`) by
# running `herdr-focus-notify --watch-machine <label>` as a LaunchAgent.
#
#   scripts/install-machine-watcher.sh <label>           write the LaunchAgent
#   scripts/install-machine-watcher.sh <label> --remove  delete it
#
# Herdr runs plugin event hooks on the server that hosts the panes, and its
# startup hooks run once and exit, so the long-running watcher is a LaunchAgent
# of its own. It runs this checkout's target/release/herdr-focus-notify; build
# it first. The script only writes or deletes the plist, and prints the
# launchctl command that loads or unloads it.
set -eu

usage() {
	echo "usage: $0 <machine-label> [--remove]" >&2
	exit 2
}

[ $# -ge 1 ] && [ $# -le 2 ] || usage
label=$1
case ${2-} in
'') remove= ;;
--remove) remove=1 ;;
*) usage ;;
esac
# The label names the launchd job and its files.
case $label in
'' | -* | *[!A-Za-z0-9._-]*)
	echo "machine label must use only letters, digits, '.', '_' and '-': $label" >&2
	exit 2
	;;
esac

job="local.herdr.focus-notify.watch.$label"
plist="$HOME/Library/LaunchAgents/$job.plist"
domain="gui/$(id -u)"

if [ -n "$remove" ]; then
	rm -f "$plist"
	echo "removed $plist"
	echo "stop the running watcher with:"
	echo "  launchctl bootout $domain/$job"
	exit 0
fi

repo=$(cd "$(dirname "$0")/.." && pwd)
binary="$repo/target/release/herdr-focus-notify"
# The plugin's own state directory, so the watcher finds the terminal Herdr
# runs in from the bindings the plugin learned.
state_dir=${HERDR_PLUGIN_STATE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/herdr/plugins/herdr-focus-notify}
log="$HOME/Library/Logs/herdr-focus-notify/watch-$label.log"
# launchd starts jobs with a bare PATH; herdr, alerter and ssh live here.
path="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

[ -x "$binary" ] || echo "warning: $binary does not exist yet; run 'cargo build --release' in $repo" >&2

xml() {
	printf '%s' "$1" | sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g'
}

herdr_bin_entry=
if [ -n "${HERDR_BIN_PATH-}" ]; then
	herdr_bin_entry="
    <key>HERDR_BIN_PATH</key><string>$(xml "$HERDR_BIN_PATH")</string>"
fi

mkdir -p "$(dirname "$plist")" "$(dirname "$log")"
# AbandonProcessGroup keeps notifications already on screen clickable when
# launchd restarts the watcher.
cat >"$plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$job</string>
  <key>ProgramArguments</key>
  <array>
    <string>$(xml "$binary")</string>
    <string>--watch-machine</string>
    <string>$label</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key><string>$(xml "$path")</string>
    <key>HERDR_PLUGIN_STATE_DIR</key><string>$(xml "$state_dir")</string>$herdr_bin_entry
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>AbandonProcessGroup</key><true/>
  <key>StandardOutPath</key><string>$(xml "$log")</string>
  <key>StandardErrorPath</key><string>$(xml "$log")</string>
</dict>
</plist>
EOF
plutil -lint "$plist" >/dev/null

echo "wrote $plist"
echo "start the watcher with (after 'launchctl bootout $domain/$job' if it is already loaded):"
echo "  launchctl bootstrap $domain $plist"
echo "log: $log"
