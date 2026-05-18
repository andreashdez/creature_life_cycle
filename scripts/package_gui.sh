#!/bin/sh
set -eu

if [ "$(uname -s)" != "Darwin" ]; then
    printf '%s\n' "This packaging script builds a macOS .app bundle and must run on macOS." >&2
    exit 1
fi

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target_dir="$root_dir/target/package"
app_name="Creature Life Cycle"
app_bundle="$target_dir/$app_name.app"
contents_dir="$app_bundle/Contents"
macos_dir="$contents_dir/MacOS"
resources_dir="$contents_dir/Resources"
executable_name="creature_life_cycle_gui"
zip_path="$target_dir/creature_life_cycle_gui-macos.zip"
version=$(cargo pkgid --manifest-path "$root_dir/Cargo.toml")
version=${version##*#}

cargo build --manifest-path "$root_dir/Cargo.toml" --release --bin gui

rm -rf "$app_bundle" "$zip_path"
mkdir -p "$macos_dir" "$resources_dir"
cp "$root_dir/target/release/gui" "$macos_dir/$executable_name"
chmod 755 "$macos_dir/$executable_name"

cat > "$contents_dir/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>$app_name</string>
  <key>CFBundleExecutable</key>
  <string>$executable_name</string>
  <key>CFBundleIdentifier</key>
  <string>dev.creature-life-cycle.gui</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>$app_name</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$version</string>
  <key>CFBundleVersion</key>
  <string>$version</string>
  <key>LSMinimumSystemVersion</key>
  <string>10.13</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
EOF

ditto -c -k --sequesterRsrc --keepParent "$app_bundle" "$zip_path"

printf 'Packaged %s\n' "$app_bundle"
printf 'Created %s\n' "$zip_path"
