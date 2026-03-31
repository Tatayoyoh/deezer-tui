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

# Check for uncommitted changes
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "Error: you have uncommitted changes. Commit or stash them first."
    exit 1
fi

# Check tag doesn't already exist
if git rev-parse "${TAG}" >/dev/null 2>&1; then
    echo "Error: tag ${TAG} already exists"
    exit 1
fi

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
