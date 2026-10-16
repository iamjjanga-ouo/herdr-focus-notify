#!/bin/sh
# Registers a stub ~/Applications/Herdr.app so notifications say "Herdr".
#
# alerter shows a notification as coming from the app named by --sender
# (Terminal by default). The plugin passes this stub's bundle id once the app
# exists; delete the app to go back to Terminal.
set -eu

app="$HOME/Applications/Herdr.app"
mkdir -p "$app/Contents/MacOS"

cat >"$app/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>local.herdr.focus-notify</string>
  <key>CFBundleName</key><string>Herdr</string>
  <key>CFBundleDisplayName</key><string>Herdr</string>
  <key>CFBundleExecutable</key><string>herdr-notify-stub</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSUIElement</key><true/>
</dict>
</plist>
EOF

# LaunchServices wants an executable; the app is never meant to run.
printf '#!/bin/sh\nexit 0\n' >"$app/Contents/MacOS/herdr-notify-stub"
chmod +x "$app/Contents/MacOS/herdr-notify-stub"

/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$app"
echo "registered $app"
