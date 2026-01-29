#!/usr/bin/env bash
# Generate changelog grouped by src/ module based on actual file paths changed
# Each commit appears in only ONE module (the one with most files touched)
# Usage: ./scripts/generate-changelog.sh [--unreleased | --tag vX.Y.Z]
# Requires: git-cliff, git

set -e

MODE="${1:---unreleased}"
TAG="${2:-}"

# Modules from src/ directory structure (order matters for tie-breaking)
MODULES="tui player terminal asciicast commands analyzer branding cli config recording shell storage"

# Capitalize first letter
capitalize() {
    echo "$1" | awk '{print toupper(substr($0,1,1)) tolower(substr($0,2))}'
}

# Get the primary module for a commit based on file count
# Priority: src/ modules > commit scope fallback > tests/docs
get_primary_module() {
    local sha="$1"
    local scope="$2"
    local max_count=0
    local primary=""

    # Get files changed in this commit
    local files=$(git diff-tree --no-commit-id --name-only -r "$sha" 2>/dev/null)

    # First pass: only count src/ modules (higher priority)
    for mod in $MODULES; do
        local count=$(echo "$files" | grep -c "^src/$mod/" 2>/dev/null || echo 0)
        if [[ $count -gt $max_count ]]; then
            max_count=$count
            primary="$mod"
        fi
    done

    # Check single-file modules in src/
    for file in src/*.rs; do
        [[ -f "$file" ]] || continue
        local filename=$(basename "$file" .rs)
        [[ "$filename" == "lib" || "$filename" == "main" ]] && continue
        echo "$MODULES" | grep -qw "$filename" && continue

        local count=$(echo "$files" | grep -c "^$file$" 2>/dev/null || echo 0)
        if [[ $count -gt $max_count ]]; then
            max_count=$count
            primary="$filename"
        fi
    done

    # Fallback: use commit scope if no src/ module found
    if [[ -z "$primary" && -n "$scope" ]]; then
        # Only use if it's a valid module
        if echo "$MODULES" | grep -qw "$scope"; then
            primary="$scope"
        fi
    fi

    # Last resort: tests/docs only if still nothing
    if [[ -z "$primary" ]]; then
        local test_count=$(echo "$files" | grep -c "^tests/" 2>/dev/null || echo 0)
        if [[ $test_count -gt 0 ]]; then
            primary="tests"
        fi

        local docs_count=$(echo "$files" | grep -c "^docs/" 2>/dev/null || echo 0)
        if [[ $docs_count -gt $test_count ]]; then
            primary="docs"
        fi
    fi

    echo "$primary"
}

# Get all relevant commits
get_commits() {
    local cliff_args=""
    if [[ "$MODE" == "--unreleased" ]]; then
        cliff_args="--unreleased"
    elif [[ "$MODE" == "--tag" ]]; then
        cliff_args="--tag $TAG"
    fi

    # Get commits with their SHAs, scopes, and messages
    # Format: sha|scope|message
    git cliff $cliff_args --body "{% for commit in commits %}{{ commit.id }}|{{ commit.scope | default(value='') }}|{{ commit.message | upper_first }}
{% endfor %}" 2>/dev/null | grep -v "^$" || true
}

# Build module -> commits mapping
declare_module_commits() {
    local commits="$1"

    # Temp files for each module
    for mod in $MODULES tests docs; do
        echo "" > "/tmp/changelog_$mod.txt"
    done

    # Process each commit (format: sha|scope|message)
    while IFS='|' read -r sha scope message; do
        [[ -z "$sha" ]] && continue
        local primary=$(get_primary_module "$sha" "$scope")
        if [[ -n "$primary" ]]; then
            echo "- $message" >> "/tmp/changelog_$primary.txt"
        fi
    done <<< "$commits"
}

# Generate changelog
generate_changelog() {
    echo "# Changelog"
    echo ""
    echo "All notable changes to this project will be documented in this file."
    echo "Grouped by module based on files changed (each commit in primary module only)."
    echo ""

    if [[ "$MODE" == "--unreleased" ]]; then
        echo "## [Unreleased]"
    elif [[ "$MODE" == "--tag" ]]; then
        echo "## [$TAG] - $(date +%Y-%m-%d)"
    fi
    echo ""

    # Get and process commits
    local commits=$(get_commits)
    declare_module_commits "$commits"

    # Output each module's commits
    for mod in $MODULES; do
        local content=$(cat "/tmp/changelog_$mod.txt" 2>/dev/null | grep -v "^$" | sort -u)
        if [[ -n "$content" ]]; then
            echo "### $(capitalize "$mod")"
            echo "$content"
            echo ""
        fi
    done

    # Tests section
    local content=$(cat "/tmp/changelog_tests.txt" 2>/dev/null | grep -v "^$" | sort -u)
    if [[ -n "$content" ]]; then
        echo "### Tests"
        echo "$content"
        echo ""
    fi

    # Docs section
    local content=$(cat "/tmp/changelog_docs.txt" 2>/dev/null | grep -v "^$" | sort -u)
    if [[ -n "$content" ]]; then
        echo "### Documentation"
        echo "$content"
        echo ""
    fi

    # Cleanup
    rm -f /tmp/changelog_*.txt
}

generate_changelog
echo "Done." >&2
