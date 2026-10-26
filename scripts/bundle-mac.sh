#!/usr/bin/env bash
# Build the tester DMG (M4b): a universal (Apple Silicon + Intel, D7) fuxx.app in
# target/dist/fuxx-<version>.dmg. Ad-hoc signed only: Developer ID signing and notarization are
# M11, so testers click "Open Anyway" once per version. The version comes from Cargo.toml.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(cargo pkgid | sed 's/.*[#@]//')
dmg="target/dist/fuxx-$version.dmg"
# One DMG per version, so a tester's copy always says which build it is.
[[ -e "$dmg" ]] && { echo "$dmg already exists: bump version in Cargo.toml" >&2; exit 1; }

# Both slices target the same macOS floor as Info.plist's LSMinimumSystemVersion.
export MACOSX_DEPLOYMENT_TARGET=11.0
targets=(aarch64-apple-darwin x86_64-apple-darwin)
installed=$(rustup target list --installed)
for target in "${targets[@]}"; do
    grep -qx "$target" <<<"$installed" \
        || { echo "missing Rust target: rustup target add $target" >&2; exit 1; }
    cargo build --release --target "$target"
done

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
app="$stage/fuxx.app"
mkdir -p "$app/Contents/MacOS"
lipo -create -output "$app/Contents/MacOS/fuxx" \
    target/aarch64-apple-darwin/release/fuxx target/x86_64-apple-darwin/release/fuxx
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
# Ad-hoc seal over the whole bundle (binary + Info.plist). Not a Developer ID signature: M11.
codesign --force --sign - "$app"
ln -s /Applications "$stage/Applications"

mkdir -p target/dist
hdiutil create -quiet -volname "fuxx $version" -srcfolder "$stage" -format UDZO "$dmg"
echo "$dmg"
