#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

CARGO_TOML="Cargo.toml"
PKG_JSON="web/package.json"
PKG_LOCK="web/package-lock.json"
IMAGE="ghcr.io/fronalabs/frona"

die() { echo "error: $*" >&2; exit 1; }

current_version() {
  grep -m1 '^version' "$CARGO_TOML" | sed 's/.*"\(.*\)"/\1/'
}

parse_version() {
  local ver="$1"
  local base pre
  if [[ "$ver" == *-* ]]; then
    base="${ver%%-*}"
    pre="${ver#*-}"
  else
    base="$ver"
    pre=""
  fi
  IFS='.' read -r MAJOR MINOR PATCH <<< "$base"
  PRE_TAG="" PRE_NUM=""
  if [[ -n "$pre" ]]; then
    PRE_TAG="${pre%%[0-9]*}"
    PRE_NUM=$(echo "$pre" | grep -o '[0-9]*$')
  fi
}

format_version() {
  local ver="${MAJOR}.${MINOR}.${PATCH}"
  if [[ -n "${PRE_TAG:-}" ]]; then
    ver="${ver}-${PRE_TAG}${PRE_NUM}"
  fi
  echo "$ver"
}

bump_segment() {
  case "$1" in
    patch) PATCH=$((PATCH + 1)) ;;
    minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
    major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
  esac
}

update_files() {
  local new_ver="$1"
  local old_ver
  old_ver=$(current_version)

  sed -i.bak "s/^version = \"${old_ver}\"/version = \"${new_ver}\"/" "$CARGO_TOML"
  rm -f "${CARGO_TOML}.bak"

  sed -i.bak "s/\"version\": \"${old_ver}\"/\"version\": \"${new_ver}\"/" "$PKG_JSON"
  rm -f "${PKG_JSON}.bak"

  sed -i.bak "s/\"version\": \"${old_ver}\"/\"version\": \"${new_ver}\"/" "$PKG_LOCK"
  rm -f "${PKG_LOCK}.bak"
}

DRY_RUN=false
SKIP_DOCKER=false
SKIP_TESTS=false
COMMAND=""
PRE_RELEASE_TYPE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)    DRY_RUN=true; shift ;;
    --skip-docker) SKIP_DOCKER=true; shift ;;
    --skip-tests) SKIP_TESTS=true; shift ;;
    -*)           die "Unknown flag: $1" ;;
    *)
      if [[ -z "$COMMAND" ]]; then
        COMMAND="$1"
      elif [[ -z "$PRE_RELEASE_TYPE" ]]; then
        PRE_RELEASE_TYPE="$1"
      else
        die "Unexpected argument: $1"
      fi
      shift
      ;;
  esac
done

[[ -n "$COMMAND" ]] || die "Usage: release.sh <command> [pre-release] [--dry-run] [--skip-docker] [--skip-tests]

Commands:
  patch / minor / major       Bump to stable release
  minor alpha / major beta    Bump + start pre-release at 1
  alpha / beta / rc           Increment existing pre-release number
  stable                      Promote pre-release to stable
  <version>                   Set exact version (e.g., 1.0.0-RC1)"

OLD_VERSION=$(current_version)
parse_version "$OLD_VERSION"

case "$COMMAND" in
  patch)
    [[ -z "$PRE_RELEASE_TYPE" ]] || die "Pre-releases are not supported for patch bumps. Use 'minor $PRE_RELEASE_TYPE' or 'major $PRE_RELEASE_TYPE'."
    if [[ -n "$PRE_TAG" ]]; then
      die "Current version is a pre-release ($OLD_VERSION). Use 'stable' to promote or specify a pre-release type."
    fi
    bump_segment "patch"
    PRE_TAG="" PRE_NUM=""
    ;;
  minor|major)
    if [[ -n "$PRE_RELEASE_TYPE" ]]; then
      bump_segment "$COMMAND"
      PRE_TAG=$(echo "$PRE_RELEASE_TYPE" | tr '[:lower:]' '[:upper:]')
      PRE_NUM=1
    else
      if [[ -n "$PRE_TAG" ]]; then
        die "Current version is a pre-release ($OLD_VERSION). Use 'stable' to promote or specify a pre-release type."
      fi
      bump_segment "$COMMAND"
      PRE_TAG="" PRE_NUM=""
    fi
    ;;
  alpha|beta|rc)
    REQUESTED_TAG=$(echo "$COMMAND" | tr '[:lower:]' '[:upper:]')
    if [[ -z "$PRE_TAG" ]]; then
      die "Current version ($OLD_VERSION) is not a pre-release. Use 'minor $COMMAND' to start one."
    fi
    if [[ "$PRE_TAG" == "$REQUESTED_TAG" ]]; then
      PRE_NUM=$((PRE_NUM + 1))
    else
      PRE_TAG="$REQUESTED_TAG"
      PRE_NUM=1
    fi
    ;;
  stable)
    [[ -n "$PRE_TAG" ]] || die "Current version ($OLD_VERSION) is already stable."
    PRE_TAG="" PRE_NUM=""
    ;;
  *)
    if [[ "$COMMAND" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z]+[0-9]+)?$ ]]; then
      parse_version "$COMMAND"
      if [[ -n "$PRE_TAG" ]]; then
        PRE_TAG=$(echo "$PRE_TAG" | tr '[:lower:]' '[:upper:]')
      fi
    else
      die "Invalid version or command: $COMMAND"
    fi
    ;;
esac

NEW_VERSION=$(format_version)
IS_PRERELEASE=false
[[ -n "${PRE_TAG:-}" ]] && IS_PRERELEASE=true

echo "Version: $OLD_VERSION → $NEW_VERSION"
echo "Pre-release: $IS_PRERELEASE"

if [[ -n "$(git status --porcelain)" ]]; then
  die "Working tree is not clean. Commit or stash changes first."
fi

if [[ "$IS_PRERELEASE" == "false" ]]; then
  BRANCH=$(git rev-parse --abbrev-ref HEAD)
  [[ "$BRANCH" == "main" ]] || die "Stable releases must be from main branch (currently on '$BRANCH')."
fi

TAG="v${NEW_VERSION}"
if git rev-parse "$TAG" >/dev/null 2>&1; then
  die "Tag $TAG already exists."
fi

if [[ "$DRY_RUN" == "true" ]]; then
  echo ""
  echo "Dry run — no changes will be made."
  echo ""
  echo "  Version:  $OLD_VERSION → $NEW_VERSION"
  echo "  Tag:      $TAG"
  echo "  Commit:   release: $TAG"
  if [[ "$IS_PRERELEASE" == "false" ]]; then
    echo "  Docker:   $IMAGE:$TAG, $IMAGE:latest"
  else
    echo "  Docker:   $IMAGE:$TAG"
  fi
  echo ""
  echo "  Files:"
  echo "    $CARGO_TOML"
  echo "    $PKG_JSON"
  echo "    $PKG_LOCK"
  exit 0
fi

if [[ "$SKIP_TESTS" == "false" ]]; then
  echo "Running tests..."
  cargo test --workspace
fi

echo "Updating version files..."
update_files "$NEW_VERSION"

echo "Committing and tagging..."
git add "$CARGO_TOML" "$PKG_JSON" "$PKG_LOCK"
git commit -m "$(cat <<EOF
release: $TAG
EOF
)"
git tag -a "$TAG" -m "Release $TAG"
git push origin HEAD --tags

if [[ "$SKIP_DOCKER" == "false" ]]; then
  echo "Building and pushing Docker image..."

  docker buildx inspect multiarch >/dev/null 2>&1 || \
    docker buildx create --name multiarch --use
  docker buildx use multiarch

  TAGS=(-t "$IMAGE:$TAG")
  if [[ "$IS_PRERELEASE" == "false" ]]; then
    TAGS+=(-t "$IMAGE:latest")
  fi

  docker buildx build --platform linux/amd64,linux/arm64 \
    -f build/Dockerfile --target prod \
    "${TAGS[@]}" \
    --push .
fi

echo ""
echo "Released $TAG"
