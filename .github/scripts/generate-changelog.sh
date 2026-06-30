#!/usr/bin/env bash
# Generate a deterministic Keep-a-Changelog draft from merged PR metadata.
# Used by .github/workflows/changelog.yml (Issue #191).
set -euo pipefail

VERSION="${1:-Unreleased}"
SINCE_REF="${2:-}"

if ! command -v gh >/dev/null 2>&1; then
  echo "gh CLI is required" >&2
  exit 1
fi

if [[ -z "$SINCE_REF" ]]; then
  SINCE_REF="$(git describe --tags --abbrev=0 2>/dev/null || echo "")"
fi

if [[ -n "$SINCE_REF" ]]; then
  MERGED_SINCE="--search merged:>=$(git log -1 --format=%cs "$SINCE_REF")"
else
  MERGED_SINCE=""
fi

# Fetch merged PRs on the default branch (newest first for stable sorting).
mapfile -t PR_LINES < <(
  gh pr list \
    --state merged \
    --limit 500 \
    --json number,title,labels,mergedAt \
    --jq 'sort_by(.number) | .[] | [.number, .title, ([.labels[].name] | join(","))] | @tsv'
)

added=()
fixed=()
security=()

for line in "${PR_LINES[@]}"; do
  IFS=$'\t' read -r num title labels <<< "$line"
  labels_lower="$(echo "$labels" | tr '[:upper:]' '[:lower:]')"
  entry="- ${title} (#${num})"

  if [[ "$labels_lower" == *"security"* ]]; then
    security+=("$entry")
  elif [[ "$labels_lower" == *"bug"* ]] || [[ "$labels_lower" == *"fix"* ]]; then
    fixed+=("$entry")
  elif [[ "$labels_lower" == *"changelog:fixed"* ]]; then
    fixed+=("$entry")
  elif [[ "$labels_lower" == *"changelog:security"* ]]; then
    security+=("$entry")
  elif [[ "$labels_lower" == *"changelog:added"* ]] || [[ "$labels_lower" == *"enhancement"* ]] || [[ "$labels_lower" == *"feature"* ]]; then
    added+=("$entry")
  else
    # Default bucket for unlabeled improvements.
    added+=("$entry")
  fi
done

emit_section() {
  local heading="$1"
  shift
  local -a items=("$@")
  echo "### ${heading}"
  echo
  if ((${#items[@]} == 0)); then
    echo "- None"
  else
    printf '%s\n' "${items[@]}"
  fi
  echo
}

{
  echo "## [${VERSION}]"
  echo
  emit_section "Added" "${added[@]}"
  emit_section "Fixed" "${fixed[@]}"
  emit_section "Security" "${security[@]}"
} > CHANGELOG.draft.md

echo "Wrote CHANGELOG.draft.md (${#added[@]} added, ${#fixed[@]} fixed, ${#security[@]} security)"
