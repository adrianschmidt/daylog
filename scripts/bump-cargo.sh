#!/usr/bin/env bash
# Bump the [package] version in Cargo.toml to the value in $1, then
# regenerate Cargo.lock. Used by both the `prepare` workflow job and by
# semantic-release's @semantic-release/exec prepareCmd.
#
# Usage: scripts/bump-cargo.sh 1.2.3
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 <new-version>" >&2
  exit 64
fi

new_version="$1"
sed -i.bak "s/^version = \".*\"/version = \"${new_version}\"/" Cargo.toml
rm Cargo.toml.bak

# Reconcile Cargo.lock with the bumped Cargo.toml. Use --offline first
# (fast, uses cached registry); fall back to a non-offline run if the
# cache is cold (e.g., on a fresh CI runner before the build job has
# warmed it).
cargo update --workspace --offline 2>/dev/null || cargo update --workspace

echo "bumped Cargo.toml + Cargo.lock to ${new_version}"
