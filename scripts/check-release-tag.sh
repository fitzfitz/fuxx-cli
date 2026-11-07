#!/usr/bin/env bash
# Release guard (M11e): exits 0 only when the given tag is "v" + the Cargo.toml version, so a v*
# tag can't publish a build whose Info.plist says something else. Used by the Release workflow.
# Usage: scripts/check-release-tag.sh v0.3.0
set -euo pipefail
cd "$(dirname "$0")/.."

tag="${1:?usage: check-release-tag.sh <tag>}"
version=$(cargo pkgid | sed 's/.*[#@]//')
if [[ "$tag" != "v$version" ]]; then
    echo "tag $tag doesn't match Cargo.toml version $version" >&2
    exit 1
fi
