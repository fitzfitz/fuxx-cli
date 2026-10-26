#!/usr/bin/env bash
# Check a tester DMG (M4b): fuxx.app + Applications link, universal binary, macOS 11.0 floor,
# version (from the file name), ad-hoc seal.
# Usage: scripts/check-dmg.sh target/dist/fuxx-<version>.dmg
set -euo pipefail

dmg=${1:?usage: check-dmg.sh <dmg>}
version=$(basename "$dmg" .dmg); version=${version#fuxx-}
fail() { echo "FAIL: $*" >&2; exit 1; }

[[ -f "$dmg" ]] || fail "$dmg not found"
hdiutil verify -quiet "$dmg" || fail "image is corrupt"

mnt=$(mktemp -d)
# Cleanup must not change the verdict. `set -e` still applies inside the trap, hence `|| true`.
trap 'hdiutil detach -quiet "$mnt" 2>/dev/null || true; rmdir "$mnt" 2>/dev/null || true' EXIT
hdiutil attach -quiet -readonly -nobrowse -mountpoint "$mnt" "$dmg" \
    || fail "could not mount $dmg (already open in Finder? eject it first)"

app="$mnt/fuxx.app"
[[ -d "$app" ]] || fail "no fuxx.app in the image"
[[ "$(readlink "$mnt/Applications")" == /Applications ]] || fail "no Applications link"

archs=$(lipo -archs "$app/Contents/MacOS/fuxx")
[[ " $archs " == *" arm64 "* && " $archs " == *" x86_64 "* ]] || fail "not universal: $archs"
for arch in arm64 x86_64; do
    build=$(vtool -arch "$arch" -show-build "$app/Contents/MacOS/fuxx")
    [[ "$build" == *"minos 11.0"* ]] || fail "$arch slice does not target macOS 11.0"
done

plist="$app/Contents/Info.plist"
plutil -lint -s "$plist" || fail "Info.plist is invalid"
for key in CFBundleShortVersionString CFBundleVersion; do
    got=$(plutil -extract "$key" raw "$plist")
    [[ "$got" == "$version" ]] || fail "$key is $got, expected $version"
done

codesign --verify --strict "$app" 2>/dev/null || fail "bundle seal is broken"

echo "OK: $dmg (fuxx $version, $archs)"
