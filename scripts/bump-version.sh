#!/bin/bash
# Bump version in Cargo.toml and frontend/package.json
# Usage: ./scripts/bump-version.sh [patch|minor|major]

set -e

BUMP_TYPE="${1:-patch}"
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
CARGO_TOML="$ROOT_DIR/Cargo.toml"
PACKAGE_JSON="$ROOT_DIR/frontend/package.json"

# Get current version from Cargo.toml
CURRENT_VERSION=$(grep '^version = ' "$CARGO_TOML" | head -1 | sed 's/version = "\(.*\)"/\1/')

if [ -z "$CURRENT_VERSION" ]; then
    echo "Error: Could not find version in Cargo.toml"
    exit 1
fi

# Parse version components
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"

# Handle pre-release versions (strip -alpha, -beta, etc.)
PATCH=$(echo "$PATCH" | sed 's/-.*//')

# Bump version based on type
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
    *)
        echo "Usage: $0 [patch|minor|major]"
        echo "  patch - Increment patch version (0.1.0 -> 0.1.1)"
        echo "  minor - Increment minor version (0.1.0 -> 0.2.0)"
        echo "  major - Increment major version (0.1.0 -> 1.0.0)"
        exit 1
        ;;
esac

NEW_VERSION="$MAJOR.$MINOR.$PATCH"

echo "Bumping version: $CURRENT_VERSION -> $NEW_VERSION"

# Update Cargo.toml
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    sed -i '' "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"
else
    # Linux
    sed -i "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"
fi

# Update frontend/package.json
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    sed -i '' "s/\"version\": \"$CURRENT_VERSION\"/\"version\": \"$NEW_VERSION\"/" "$PACKAGE_JSON"
else
    # Linux
    sed -i "s/\"version\": \"$CURRENT_VERSION\"/\"version\": \"$NEW_VERSION\"/" "$PACKAGE_JSON"
fi

# Verify updates
CARGO_VERSION=$(grep '^version = ' "$CARGO_TOML" | head -1 | sed 's/version = "\(.*\)"/\1/')
PACKAGE_VERSION=$(grep '"version"' "$PACKAGE_JSON" | head -1 | sed 's/.*"version": "\(.*\)".*/\1/')

echo ""
echo "Updated versions:"
echo "  Cargo.toml:            $CARGO_VERSION"
echo "  frontend/package.json: $PACKAGE_VERSION"

if [ "$CARGO_VERSION" != "$NEW_VERSION" ] || [ "$PACKAGE_VERSION" != "$NEW_VERSION" ]; then
    echo ""
    echo "Warning: Version mismatch detected!"
    exit 1
fi

echo ""
echo "Version bump complete. Don't forget to:"
echo "  1. Review the changes: git diff"
echo "  2. Stage the files: git add Cargo.toml frontend/package.json"
echo "  3. Commit with version: git commit -m 'chore: bump version to v$NEW_VERSION'"
