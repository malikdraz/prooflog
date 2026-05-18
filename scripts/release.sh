#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="${PROOFLOG_RELEASE_ROOT:-$(pwd)}"
CHANGELOG="${PROJECT_ROOT}/CHANGELOG.md"
CARGO_TOML="${PROJECT_ROOT}/Cargo.toml"
CARGO_LOCK="${PROJECT_ROOT}/Cargo.lock"

usage() {
  cat <<'USAGE'
Usage:
  scripts/release.sh next patch|minor|major
  scripts/release.sh prepare patch|minor|major|X.Y.Z
  scripts/release.sh verify-tag [vX.Y.Z]
  scripts/release.sh extract-notes [vX.Y.Z] [output]
  scripts/release.sh publish-tap [vX.Y.Z]
USAGE
}

die() {
  echo "release: $*" >&2
  exit 1
}

current_version() {
  sed -n 's/^version = "\(.*\)"/\1/p' "$CARGO_TOML" | head -n 1
}

is_semver() {
  [[ "${1:-}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

split_version() {
  IFS=. read -r major minor patch <<<"$1"
}

compare_versions() {
  split_version "$1"
  local a_major="$major" a_minor="$minor" a_patch="$patch"
  split_version "$2"
  local b_major="$major" b_minor="$minor" b_patch="$patch"

  if (( a_major != b_major )); then
    (( a_major > b_major )) && echo 1 || echo -1
  elif (( a_minor != b_minor )); then
    (( a_minor > b_minor )) && echo 1 || echo -1
  elif (( a_patch != b_patch )); then
    (( a_patch > b_patch )) && echo 1 || echo -1
  else
    echo 0
  fi
}

next_version() {
  local bump="$1"
  local version
  version="$(current_version)"
  is_semver "$version" || die "Cargo.toml version is not stable semver: $version"
  split_version "$version"

  case "$bump" in
    patch) patch=$((patch + 1)) ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    major) major=$((major + 1)); minor=0; patch=0 ;;
    *) die "expected bump type patch, minor, or major; got $bump" ;;
  esac

  echo "${major}.${minor}.${patch}"
}

version_from_arg() {
  local arg="$1"
  case "$arg" in
    patch|minor|major) next_version "$arg" ;;
    *)
      is_semver "$arg" || die "version must be patch, minor, major, or X.Y.Z; got $arg"
      echo "$arg"
      ;;
  esac
}

ensure_clean_tree() {
  git -C "$PROJECT_ROOT" diff --quiet || die "working tree has unstaged changes"
  git -C "$PROJECT_ROOT" diff --cached --quiet || die "working tree has staged changes"
  [[ -z "$(git -C "$PROJECT_ROOT" status --porcelain)" ]] || die "working tree has untracked changes"
}

update_cargo_version() {
  local version="$1"
  ruby -0777 -pi -e "sub(/^version = \".*\"$/, 'version = \"${version}\"')" "$CARGO_TOML"
  (cd "$PROJECT_ROOT" && cargo check >/dev/null)
}

update_changelog() {
  local version="$1"
  local release_date="${PROOFLOG_RELEASE_DATE:-$(date +%F)}"
  PROOFLOG_VERSION="$version" PROOFLOG_DATE="$release_date" ruby -0777 -pi -e '
    version = ENV.fetch("PROOFLOG_VERSION")
    date = ENV.fetch("PROOFLOG_DATE")
    text = $_
    match = text.match(/^## \[Unreleased\]\n\n(?<body>.*?)(?=\n## \[|\z)/m)
    abort "release: CHANGELOG.md is missing an Unreleased section" unless match
    body = match[:body].strip
    abort "release: CHANGELOG.md Unreleased section has no release notes" if body.empty?
    abort "release: CHANGELOG.md Unreleased section still contains placeholder guidance" if body.include?("Keep this section")
    text.sub!(match[0], "## [Unreleased]\n\n## [#{version}] - #{date}\n\n#{body}\n\n")
    $_ = text
  ' "$CHANGELOG"
}

changelog_has_version() {
  local version="$1"
  grep -Eq "^## \\[${version}\\]( - [0-9]{4}-[0-9]{2}-[0-9]{2})?$" "$CHANGELOG"
}

cargo_lock_version() {
  awk '
    $0 == "name = \"prooflog\"" { in_pkg = 1; next }
    in_pkg && /^version = / { gsub(/"/, "", $3); print $3; exit }
  ' "$CARGO_LOCK"
}

extract_notes() {
  local tag="${1:-${GITHUB_REF_NAME:-}}"
  local output="${2:-release-notes.md}"
  [[ -n "$tag" ]] || die "tag is required"
  [[ "$tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)$ ]] || die "tag must match vX.Y.Z: $tag"
  local version="${BASH_REMATCH[1]}"

  awk -v version="$version" '
    $0 ~ "^## \\[" version "\\]" {
      found = 1
      print
      next
    }
    found && /^## \[/ {
      exit
    }
    found {
      print
    }
    END {
      if (!found) {
        exit 1
      }
    }
  ' "$CHANGELOG" > "$output" || die "CHANGELOG.md entry for $version is missing"

  [[ "$(wc -l < "$output")" -ge 3 ]] || die "CHANGELOG.md entry for $version is missing release notes"
}

verify_tag() {
  local tag="${1:-${GITHUB_REF_NAME:-}}"
  [[ -n "$tag" ]] || die "tag is required"
  [[ "$tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)$ ]] || die "tag must match vX.Y.Z: $tag"
  local version="${BASH_REMATCH[1]}"
  local cargo_version
  cargo_version="$(current_version)"
  [[ "$version" == "$cargo_version" ]] || die "tag $tag does not match Cargo.toml version $cargo_version"
  [[ "$(cargo_lock_version)" == "$version" ]] || die "Cargo.lock prooflog version does not match $version"
  changelog_has_version "$version" || die "CHANGELOG.md is missing section [$version]"

  if command -v gh >/dev/null 2>&1 && [[ -n "${GH_TOKEN:-}" ]]; then
    local repo="${PROOFLOG_RELEASE_REPO:-malikdraz/prooflog}"
    if gh release view "$tag" --repo "$repo" >/dev/null 2>&1; then
      die "GitHub release $tag already exists in $repo"
    fi
  fi
}

run_release_gate() {
  (cd "$PROJECT_ROOT" && cargo fmt --check)
  (cd "$PROJECT_ROOT" && cargo clippy --all-targets -- -D warnings)
  (cd "$PROJECT_ROOT" && cargo test)
  (cd "$PROJECT_ROOT" && cargo build --release)
}

prepare_release() {
  local requested="$1"
  ensure_clean_tree
  local next current
  next="$(version_from_arg "$requested")"
  current="$(current_version)"
  [[ "$(compare_versions "$next" "$current")" == "1" ]] || die "release version $next must be greater than current version $current"
  git -C "$PROJECT_ROOT" tag --list "v${next}" | grep -q . && die "local tag v${next} already exists"
  update_cargo_version "$next"
  update_changelog "$next"
  run_release_gate
  echo "Prepared v${next}"
}

remove_bottle_block() {
  local formula="$1"
  ruby -0777 -pi -e 'gsub(/\n\s*bottle do\n.*?\n\s*end\n/m, "\n")' "$formula"
}

update_formula() {
  local formula="$1" url="$2" sha="$3"
  ruby -0777 -pi -e "sub(%r{url \".*prooflog/archive/refs/tags/.*\\.tar\\.gz\"}, 'url \"${url}\"'); sub(/sha256 \".*\"/, 'sha256 \"${sha}\"')" "$formula"
  remove_bottle_block "$formula"
}

publish_tap() {
  local tag="${1:-${GITHUB_REF_NAME:-}}"
  [[ "$tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)$ ]] || die "tag must match vX.Y.Z: $tag"
  local version="${BASH_REMATCH[1]}"
  local repo="${PROOFLOG_TAP_REPO:-malikdraz/homebrew-tap}"
  if [[ -z "${PROOFLOG_TAP_PATH:-}" && -z "${HOMEBREW_TAP_TOKEN:-}" ]]; then
    die "HOMEBREW_TAP_TOKEN is required to update $repo"
  fi

  local url="https://github.com/malikdraz/prooflog/archive/refs/tags/${tag}.tar.gz"
  local sha
  sha="$(curl -fsSL "$url" | shasum -a 256 | awk '{print $1}')"
  [[ -n "$sha" ]] || die "failed to compute sha256 for $url"

  local tap_dir
  if [[ -n "${PROOFLOG_TAP_PATH:-}" ]]; then
    tap_dir="$PROOFLOG_TAP_PATH"
  else
    tap_dir="$(mktemp -d)"
    gh repo clone "$repo" "$tap_dir" -- --quiet
    git -C "$tap_dir" remote set-url origin "https://x-access-token:${HOMEBREW_TAP_TOKEN}@github.com/${repo}.git"
  fi

  local formula="${tap_dir}/Formula/prooflog.rb"
  [[ -f "$formula" ]] || die "missing tap formula: $formula"
  update_formula "$formula" "$url" "$sha"

  (cd "$tap_dir" && ruby -c Formula/prooflog.rb)
  (cd "$tap_dir" && env HOMEBREW_NO_AUTO_UPDATE=1 brew test-bot --only-tap-syntax)

  if git -C "$tap_dir" diff --quiet -- Formula/prooflog.rb; then
    echo "Homebrew tap already targets ${tag}"
    return 0
  fi

  git -C "$tap_dir" config user.name "${GIT_AUTHOR_NAME:-Malik Draz}"
  git -C "$tap_dir" config user.email "${GIT_AUTHOR_EMAIL:-engineering@malikdraz.io}"
  git -C "$tap_dir" add Formula/prooflog.rb
  git -C "$tap_dir" commit -m "prooflog ${version}"
  git -C "$tap_dir" push origin HEAD:main
}

cmd="${1:-}"
case "$cmd" in
  next)
    [[ $# -eq 2 ]] || die "next requires patch, minor, or major"
    next_version "$2"
    ;;
  prepare)
    [[ $# -eq 2 ]] || die "prepare requires patch, minor, major, or X.Y.Z"
    prepare_release "$2"
    ;;
  verify-tag)
    verify_tag "${2:-${GITHUB_REF_NAME:-}}"
    ;;
  extract-notes)
    extract_notes "${2:-${GITHUB_REF_NAME:-}}" "${3:-release-notes.md}"
    ;;
  publish-tap)
    publish_tap "${2:-${GITHUB_REF_NAME:-}}"
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    usage
    exit 1
    ;;
esac
