#!/usr/bin/env bash
# Pre-push hook: Validates version tags before push
# Prevents pushing tags that don't match Cargo.toml or aren't newer than latest release
#
# Install: ln -sf ../../scripts/pre-push-hook.sh .git/hooks/pre-push

set -e

# Read stdin for refs being pushed
while read local_ref local_sha remote_ref remote_sha; do
    # Only check version tags (v*)
    if [[ "$local_ref" =~ ^refs/tags/v[0-9] ]]; then
        TAG_NAME="${local_ref#refs/tags/}"
        TAG_VERSION="${TAG_NAME#v}"

        echo "Validating version tag: $TAG_NAME"

        # Check 1: Tag matches Cargo.toml version
        CARGO_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
        if [ "$TAG_VERSION" != "$CARGO_VERSION" ]; then
            echo "ERROR: Tag version ($TAG_VERSION) doesn't match Cargo.toml ($CARGO_VERSION)"
            echo "Update Cargo.toml version to $TAG_VERSION before tagging."
            exit 1
        fi
        echo "  Cargo.toml version matches: $CARGO_VERSION"

        # Check 2: Version is newer than latest release
        # Get latest release tag excluding current (in case re-pushing)
        LATEST=$(git tag -l 'v*' --sort=-v:refname | grep -v "^${TAG_NAME}$" | head -1 | sed 's/^v//')

        if [ -n "$LATEST" ]; then
            # Compare versions using sort -V
            HIGHER=$(printf '%s\n%s' "$LATEST" "$TAG_VERSION" | sort -V | tail -1)
            if [ "$HIGHER" = "$LATEST" ]; then
                echo "ERROR: Version $TAG_VERSION is not newer than latest release $LATEST"
                exit 1
            fi
            echo "  Version $TAG_VERSION is newer than $LATEST"
        else
            echo "  No previous releases found. First release!"
        fi

        echo "Tag validation passed!"
    fi
done

exit 0
