#!/usr/bin/env bash
# Build the tester DMG (M4b): a universal (Apple Silicon + Intel, D7) fuxx.app in
# target/dist/fuxx-<version>.dmg. Ad-hoc signed only: Developer ID signing and notarization are
# M12, so testers click "Open Anyway" once per version. The version comes from Cargo.toml.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(cargo pkgid | sed 's/.*[#@]//')
dmg="target/dist/fuxx-$version.dmg"
# Options (any order):
#   --app-only        just target/dist/fuxx.app, no DMG.
#   --build <target>  only compile that one slice (target/<target>/release/fuxx) and stop; the Release
#                     workflow builds the two slices in parallel jobs this way.
#   --prebuilt        don't compile: bundle the two slices already in target/<target>/release/fuxx.
app_only=0
prebuilt=0
only=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --app-only) app_only=1 ;;
        --prebuilt) prebuilt=1 ;;
        --build) only="${2:?--build needs a target}"; shift ;;
        *) echo "unknown option: $1" >&2; exit 1 ;;
    esac
    shift
done
# One DMG per version, so a tester's copy always says which build it is. This only sees DMGs still in
# target/dist: after `cargo clean` (or in another checkout) the same version can be built again.
[[ -z "$only" && $app_only == 0 && -e "$dmg" ]] && { echo "$dmg already exists: bump version in Cargo.toml" >&2; exit 1; }

# Both slices target the same macOS floor as Info.plist's LSMinimumSystemVersion.
export MACOSX_DEPLOYMENT_TARGET=11.0
targets=(aarch64-apple-darwin x86_64-apple-darwin)
build_slice() {
    rustup target list --installed | grep -qx "$1" \
        || { echo "missing Rust target: rustup target add $1" >&2; exit 1; }
    cargo build --release --target "$1"
}
if [[ -n "$only" ]]; then
    [[ " ${targets[*]} " == *" $only "* ]] || { echo "not a fuxx target: $only" >&2; exit 1; }
    build_slice "$only"
    echo "target/$only/release/fuxx"
    exit 0
fi
for target in "${targets[@]}"; do
    if [[ $prebuilt == 1 ]]; then
        [[ -f "target/$target/release/fuxx" ]] || { echo "missing slice: target/$target/release/fuxx" >&2; exit 1; }
    else
        build_slice "$target"
    fi
done

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
app="$stage/fuxx.app"
mkdir -p "$app/Contents/MacOS"
lipo -create -output "$app/Contents/MacOS/fuxx" \
    target/aarch64-apple-darwin/release/fuxx target/x86_64-apple-darwin/release/fuxx
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
# The app icon (Dock, ⌘Tab, Finder, notification banners), named by Info.plist's CFBundleIconFile.
# Made from the 3D Cyber logo by scripts/make_app_icon.py (M11c Task 6).
mkdir -p "$app/Contents/Resources"
cp packaging/macos/fuxx.icns "$app/Contents/Resources/fuxx.icns"
# The bundled fonts and themes are compiled into the binary; their licences require the notices to
# travel with every copy, so they ship in Resources/Licenses.
licenses="$app/Contents/Resources/Licenses"
mkdir -p "$licenses"
cp assets/fonts/LICENSE-* assets/fonts/NOTICE-* "$licenses/"
cp assets/themes/LICENSE-* "$licenses/"
cp assets/themes/NOTICE.md "$licenses/NOTICE-Themes.md"
# Ad-hoc seal over the whole bundle (binary + Info.plist). Not a Developer ID signature: M12.
codesign --force --sign - "$app"
if [[ $app_only == 1 ]]; then
    # M10a spike: just the app, no DMG.
    mkdir -p target/dist
    rm -rf target/dist/fuxx.app
    cp -R "$app" target/dist/fuxx.app
    echo target/dist/fuxx.app
    exit 0
fi
ln -s /Applications "$stage/Applications"

mkdir -p target/dist
hdiutil create -quiet -volname "fuxx $version" -srcfolder "$stage" -format UDZO "$dmg"
echo "$dmg"
