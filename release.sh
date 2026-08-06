#!/bin/sh
# Release script — bumps Cargo.toml version, commits, tags, and pushes.
# Usage: ./release.sh 1.3.0
set -e

if [ -z "$1" ]; then
    CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    echo "Usage: ./release.sh <version>"
    echo "Current version: ${CURRENT}"
    exit 1
fi

VERSION="$1"
TAG="v${VERSION}"

# Check tag doesn't already exist
if git rev-parse "${TAG}" >/dev/null 2>&1; then
    echo "Error: tag ${TAG} already exists"
    exit 1
fi

# Set aside uncommitted work so it can't leak into the release commit.
#
# Only stash when there is actually something to stash: `git stash` is a no-op
# on a clean tree, so an unconditional `pop` afterwards would restore whatever
# unrelated stash happened to be on top instead.
#
# Untracked files are left alone on purpose (no -u): the commit below only adds
# three known paths, so untracked files can't end up in it, and stashing them
# would sweep away build output and scratch files for no benefit.
STASHED=0
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "Stashing uncommitted changes..."
    git stash push --quiet -m "release.sh ${TAG}"
    STASHED=1
fi

# Always hand the stash back — including on failure or Ctrl-C. Without this an
# aborted release would leave the working tree silently stripped of the user's
# changes.
restore_stash() {
    if [ "${STASHED}" -eq 1 ]; then
        STASHED=0
        echo "Restoring stashed changes..."
        if ! git stash pop --quiet; then
            echo "Warning: could not restore the stash automatically."
            echo "Your changes are safe in the stash — recover them with:"
            echo "    git stash pop"
        fi
    fi
}
trap 'restore_stash' EXIT
trap 'restore_stash; exit 130' INT
trap 'restore_stash; exit 143' TERM

# Update version in workspace Cargo.toml
sed -i "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml

# Update Cargo.lock
cargo check --quiet 2>/dev/null

# Update CHANGELOG.md — replace [Unreleased] with [VERSION] - DATE
DATE=$(date +%Y-%m-%d)
if [ -f CHANGELOG.md ]; then
    sed -i "s/^## \[Unreleased\]/## [${VERSION}] - ${DATE}/" CHANGELOG.md
    echo "Updated CHANGELOG.md: [Unreleased] → [${VERSION}] - ${DATE}"
else
    echo "Warning: CHANGELOG.md not found, skipping changelog update"
fi

echo "Bumped to ${VERSION}"
echo ""

# Commit + tag + push
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "Release ${TAG}"
git tag "${TAG}"
git push
git push --tags

echo ""
echo "Released ${TAG} — CircleCI will build and publish the binaries."
