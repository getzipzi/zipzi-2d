# Shared path setup for Editor shell entrypoints. Source before changing directory.
caller_dir=$PWD
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
# Preserve caller-relative Cargo output paths before entering the workspace.
case "${CARGO_TARGET_DIR:-}" in
  ''|/*) ;;
  *) CARGO_TARGET_DIR="$caller_dir/$CARGO_TARGET_DIR"; export CARGO_TARGET_DIR ;;
esac
caller_path() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s/%s\n' "$caller_dir" "$1" ;;
  esac
}
cd "$repo_root"
