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

# Unreleased becomes the new version in CHANGELOG.md (fails when it is empty)
python3 scripts/changelog.py release "v${NEW_VERSION}"

# The lint numbers given so far are released: a mark with their hash pins them (rule_numbers_are_stable)
python3 - "v${NEW_VERSION}" <<'PY'
import sys
p = 'src/data/lint-numbers.txt'
lines = open(p).read().rstrip('\n').split('\n')
numbered = [l for l in lines if l and not l.startswith('#')]
h = 0xcbf29ce484222325
for b in '\n'.join(numbered).encode():
    h = ((h ^ b) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
lines.append(f"# released {sys.argv[1]}: the {len(numbered)} numbered lines above never change (fnv {h:016x})")
open(p, 'w').write('\n'.join(lines) + '\n')
PY

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

$(python3 scripts/changelog.py notes "v${NEW_VERSION}")

<details><summary>All commits</summary>

$COMMITS

</details>
EOF

# Update Cargo.toml
sed -i "s/^version = \"${CURRENT_VERSION}\"/version = \"${NEW_VERSION}\"/" Cargo.toml
sed -i "s/^version: ${CURRENT_VERSION}$/version: ${NEW_VERSION}/" CITATION.cff
sed -i "s/^version = \"${CURRENT_VERSION}\"/version = \"${NEW_VERSION}\"/" editors/intellij/build.gradle.kts
sed -i "s/satex-intellij-${CURRENT_VERSION}\.zip/satex-intellij-${NEW_VERSION}.zip/" doc/src/wiki/intellij.md.in doc/wiki/intellij.md
sed -i "s/^Current version: ${CURRENT_VERSION}\./Current version: ${NEW_VERSION}./; s/^satex ${CURRENT_VERSION}$/satex ${NEW_VERSION}/" README.md
(cd editors/vscode && npm version --no-git-tag-version --allow-same-version "$NEW_VERSION" > /dev/null)

# Update Cargo.lock
cargo update --offline -p "$(sed -n 's/^name = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"

# Print summary
echo "Prepared release v${NEW_VERSION}"
echo ""
echo "Files changed:"
echo "  - Cargo.toml (version: $CURRENT_VERSION -> $NEW_VERSION)"
echo "  - Cargo.lock (updated)"
echo "  - CITATION.cff, README.md, editors/vscode, editors/intellij"
echo "  - CHANGELOG.md (Unreleased -> v${NEW_VERSION})"
echo "  - doc/release-notes/${NEW_VERSION}.md (created)"
echo ""
echo "Next steps:"
echo "  git commit -S -am 'build(release): v$NEW_VERSION, <what it brings>'"
echo "  git push origin main"
echo "  gh workflow run release.yml --ref main -f tag=main -f publish=false   # every target builds, before the tag"
echo "  git tag -s -a v${NEW_VERSION} -m \"Release v${NEW_VERSION}\" && git push origin v${NEW_VERSION}"
echo "A failed step after the tag: fix it on main, then gh workflow run release.yml --ref main -f tag=v${NEW_VERSION}"
