#!/usr/bin/env bash
# Personal Intel preview; does not publish or modify the official release.
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
[[ "$(uname -s)" == Darwin ]] || { echo 'Run this script on macOS.' >&2; exit 1; }
. "$ROOT/packaging/fonts.sh"
prepare_fonts
export MACOSX_DEPLOYMENT_TARGET=13.0
cargo build --manifest-path "$ROOT/Cargo.toml" --release --locked --target x86_64-apple-darwin -p lightcraft --features lightcraft/heif
WORK="$CARGO_TARGET_DIR/edz-preview"
APP="$WORK/LightCraft EdZ Lab.app"
[[ ! -e "$APP" ]] || { echo "Remove the previous staging bundle first: $APP" >&2; exit 1; }
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/Licenses" "$DIST"
cp "$CARGO_TARGET_DIR/x86_64-apple-darwin/release/lightcraft" "$APP/Contents/MacOS/LightCraftCore"
cp "$ROOT/packaging/edz/launch.sh" "$APP/Contents/MacOS/LightCraftEdZ"
chmod 755 "$APP/Contents/MacOS/LightCraftEdZ" "$APP/Contents/MacOS/LightCraftCore"
cp "$ROOT/assets/app-icon/lightcraft.icns" "$APP/Contents/Resources/LightCraft.icns"
python3 "$ROOT/packaging/edz/prepare-bundle.py" "$ROOT/packaging/macos/Info.plist.in" "$APP" "$VERSION" "${LIGHTCRAFT_BUILD_SHA:-unknown}"
copy_docs "$APP/Contents/Resources/Licenses"
cp "$ROOT/docs/edz-macos-preview.md" "$WORK/LISEZ-MOI.md"
printf 'APPL????' > "$APP/Contents/PkgInfo"
xattr -cr "$APP"
codesign --force --sign - --timestamp=none --options runtime --entitlements "$ROOT/packaging/macos/entitlements.plist" "$APP/Contents/MacOS/LightCraftCore"
codesign --force --sign - --timestamp=none --options runtime --entitlements "$ROOT/packaging/macos/entitlements.plist" "$APP"
codesign --verify --strict --deep --verbose=2 "$APP"
plutil -lint "$APP/Contents/Info.plist"
lipo -verify_arch x86_64 "$APP/Contents/MacOS/LightCraftCore"
otool -L "$APP/Contents/MacOS/LightCraftCore"
ARCHIVE="$DIST/LightCraft-EdZ-Lab-macOS-Intel.zip"
ditto -c -k --sequesterRsrc --keepParent "$WORK" "$ARCHIVE"
(cd "$DIST" && shasum -a 256 "$(basename "$ARCHIVE")" > "$(basename "$ARCHIVE").sha256")
echo "Preview: $ARCHIVE"
