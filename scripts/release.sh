#!/bin/sh
set -eu

usage() {
    echo "Usage: $0 <patch|minor|major> \"<title>\"" >&2
    exit 1
}

if [ $# -ne 2 ]; then
    usage
fi

BUMP_TYPE="$1"
TITLE="$2"

case "$BUMP_TYPE" in
    patch|minor|major)
        ;;
    *)
        usage
        ;;
esac

# Check for dirty working tree
if ! git diff-index --quiet HEAD --; then
    echo "Error: working tree is dirty. Commit or stash changes first." >&2
    exit 1
fi

if [ -n "$(git ls-files --others --exclude-standard)" ]; then
    echo "Error: untracked files present. Commit or stash them first." >&2
    exit 1
fi

# Get current version from Cargo.toml
CURRENT_VERSION=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
if [ -z "$CURRENT_VERSION" ]; then
    echo "Error: could not read version from Cargo.toml" >&2
    exit 1
fi

# Parse version
IFS='.' read -r MAJOR MINOR PATCH <<EOF
$CURRENT_VERSION
EOF

# Compute next version
case "$BUMP_TYPE" in
    patch)
        PATCH=$((PATCH + 1))
        ;;
    minor)
        MINOR=$((MINOR + 1))
        PATCH=0
        ;;
    major)
        MAJOR=$((MAJOR + 1))
        MINOR=0
        PATCH=0
        ;;
esac

NEW_VERSION="${MAJOR}.${MINOR}.${PATCH}"

# Get commits since last tag
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")

if [ -z "$LAST_TAG" ]; then
    COMMITS=$(git log --pretty=format:"- %h %s" --reverse)
else
    COMMITS=$(git log "${LAST_TAG}..HEAD" --pretty=format:"- %h %s" --reverse)
fi

# Create release notes directory if it doesn't exist
mkdir -p doc/release-notes

# Write release notes
cat > "doc/release-notes/${NEW_VERSION}.md" <<EOF
# $TITLE

$COMMITS
EOF

# Update Cargo.toml
sed -i "s/^version = \"${CURRENT_VERSION}\"/version = \"${NEW_VERSION}\"/" Cargo.toml
sed -i "s/^version: ${CURRENT_VERSION}$/version: ${NEW_VERSION}/" CITATION.cff

# Update Cargo.lock
cargo update -p satex 2>/dev/null || true

# Print summary
echo "Prepared release v${NEW_VERSION}"
echo ""
echo "Files changed:"
echo "  - Cargo.toml (version: $CURRENT_VERSION -> $NEW_VERSION)"
echo "  - Cargo.lock (updated)"
echo "  - doc/release-notes/${NEW_VERSION}.md (created)"
echo ""
echo "Next steps:"
echo "  git commit -am 'Bump version to $NEW_VERSION'"
echo "  git tag -a v${NEW_VERSION} -m \"Release v${NEW_VERSION}\""
