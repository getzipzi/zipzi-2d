#!/bin/sh
set -eu

tools_dir=$(CDPATH= cd -- "$(dirname -- "$0")/." && pwd)
. "$tools_dir/workspace/workspace.sh"

case "${1:-editor}" in
  editor)
    package=zipzi-editor
    name='Zipzi Editor'
    bundle_id=dev.zipzi.editor.watch
    icon_asset="$repo_root/crates/zipzi-editor/assets/app-icons/zipzi-editor-icon-vector.icns"
    ;;
  *)
    printf 'usage: %s [editor] [watch|run]\n' "$0" >&2
    exit 2
    ;;
esac

if [ ! -d "$repo_root/crates" ]; then
  printf 'missing workspace crates directory\n' >&2
  exit 1
fi

# Keep dev builds separate from tests, even when CARGO_TARGET_DIR is inherited.
dev_target_dir="$PWD/target/dev"
# Follow Cargo's incremental settings, including per-package cache overrides.
# CARGO_INCREMENTAL can still be set explicitly by the caller.

app="$PWD/target/$name Dev.app"
executable="$app/Contents/MacOS/$package"
pid_file="$PWD/target/dev-$package.pid"
watch_lock="$PWD/target/dev-$package.watch.lock"
reload_lock="$PWD/target/dev-$package.reload.lock"
owned_lock=
launch_pid_file=
dev_attempts="${ZIPZI_DEV_ATTEMPTS:-50}"
dev_interval="${ZIPZI_DEV_SLEEP_INTERVAL:-0.1}"
dev_watch_interval="${ZIPZI_DEV_WATCH_INTERVAL:-0.5}"

# mkdir is atomic on macOS too. Never steal a lock: concurrent recovery can
# otherwise remove a new owner's lock. A crash leaves an explicit stale-lock error.
lock_live() {
  [ -r "$1/owner" ] || return 1
  lock_pid=$(cat "$1/owner")
  case "$lock_pid" in ''|*[!0-9]*) return 1 ;; esac
  kill -0 "$lock_pid" 2>/dev/null
}
acquire_lock() {
  mkdir -p target
  if ! mkdir "$1" 2>/dev/null; then
    # Allow the creator to publish its owner before considering the lock stale.
    sleep 0.1
    if lock_live "$1" || [ ! -s "$1/owner" ]; then
      printf 'A %s operation is already running (lock: %s).\n' "$package" "$1" >&2
      return 1
    fi
    printf 'Stale dev lock: %s. Remove it after checking that its owner has exited.\n' "$1" >&2
    return 1
  fi
  printf '%s\n' "$$" > "$1/owner"
  owned_lock=$1
}
release_lock() {
  if [ -n "$owned_lock" ] && [ "$(cat "$owned_lock/owner" 2>/dev/null || :)" = "$$" ]; then
    rm -f "$owned_lock/owner" "$owned_lock/restarting"
    rmdir "$owned_lock" 2>/dev/null || :
  fi
}

is_dev_process() {
  case "$1" in
    ''|*[!0-9]*) return 1 ;;
  esac
  command_line=$(ps -ww -p "$1" -o command= 2>/dev/null) || return 1
  case "$command_line" in
    "$executable"|"$executable "*) return 0 ;;
    *) return 1 ;;
  esac
}

dev_pids() {
  ps -axww -o pid=,command= | while read -r process_pid command_line; do
    case "$command_line" in
      "$executable"|"$executable "*) printf '%s\n' "$process_pid" ;;
    esac
  done
}

case "${2:-watch}" in
  watch)
    if ! command -v watchexec >/dev/null 2>&1; then
      printf 'watchexec is required. Install it with: brew install watchexec\n' >&2
      exit 1
    fi
    acquire_lock "$watch_lock" || exit 1
    printf 'Watching %s; successful builds relaunch the app in the background.\n' "$package"
    initial_pid=
    if [ -s "$pid_file" ]; then
      initial_pid=$(cat "$pid_file")
    fi
    watched_pid=
    if is_dev_process "$initial_pid"; then
      watched_pid=$initial_pid
    fi
    watchexec --on-busy-update=queue --exts rs,toml,lock,metal,svg,icns \
      --watch "$repo_root/crates" --watch "$repo_root/vendor" \
      --watch "$repo_root/Cargo.toml" --watch "$repo_root/Cargo.lock" \
      -- "$repo_root/tools/dev-editor.sh" "${1:-editor}" reload &
    watcher_pid=$!
    stop_watcher() {
      kill -TERM "$watcher_pid" 2>/dev/null || :
      wait "$watcher_pid" 2>/dev/null || :
      release_lock
    }
    trap stop_watcher 0
    trap 'exit 130' INT
    trap 'exit 143' TERM
    trap 'exit 129' HUP

    while :; do
      if [ -s "$pid_file" ]; then
        current_pid=$(cat "$pid_file")
        case "$current_pid" in
          ''|*[!0-9]*) ;;
          *)
            if [ "$current_pid" != "$initial_pid" ]; then
              watched_pid=$current_pid
            fi
            ;;
        esac
      fi
      if [ -n "$watched_pid" ] && ! is_dev_process "$watched_pid"; then
        # The reload owns the intentional gap between stopping and launching.
        if [ -f "$reload_lock/restarting" ] && lock_live "$reload_lock"; then
          sleep "$dev_watch_interval"
          continue
        fi
        if [ -s "$pid_file" ]; then
          current_pid=$(cat "$pid_file")
          if [ "$current_pid" != "$watched_pid" ] && is_dev_process "$current_pid"; then
            watched_pid=$current_pid
            continue
          fi
        fi
        printf '%s closed; stopping the file watcher.\n' "$name"
        exit 0
      fi
      watcher_state=$(ps -p "$watcher_pid" -o stat= 2>/dev/null || :)
      case "$watcher_state" in
        ''|Z*)
          wait "$watcher_pid"
          exit $?
          ;;
      esac
      sleep "$dev_watch_interval"
    done
    ;;
  reload)
    acquire_lock "$reload_lock" || exit 1
    cleanup_reload() {
      # A timed-out or interrupted launch may still publish its PID later.
      # Keep its private record so the next reload can recover ownership.
      release_lock
    }
    trap cleanup_reload 0
    trap 'exit 130' INT
    trap 'exit 143' TERM
    trap 'exit 129' HUP
    mkdir -p target
    log="target/dev-build-$package.log"
    # GPUI FPS is temporarily disabled; omit the optional debug-tools feature.
    if ! cargo build --locked --message-format=json-render-diagnostics --target-dir "$dev_target_dir" -p "$package" >"$log.json" 2>"$log"; then
      printf 'Build failed; full output: %s\n' "$log" >&2
      tail -n 80 "$log" >&2
      exit 1
    fi

    binary=$(python3 "$repo_root/tools/workspace/workspace_tools.py" executable "$log.json")

    old_pid=
    if [ -s "$pid_file" ]; then
      old_pid=$(cat "$pid_file")
      if ! is_dev_process "$old_pid"; then old_pid=; fi
    fi
    # Recover a previous launch that published its PID after the reload exited.
    # Only records created by this script establish ownership; matching the app
    # path alone is not enough to close an instance with unsaved work.
    recovered_pid_file=
    existing_pids=$(dev_pids)
    for existing_pid in $existing_pids; do
      if [ "$existing_pid" != "$old_pid" ]; then
        pending_pid_file=
        for pending in "$pid_file".launch.*; do
          if [ -s "$pending" ] && [ "$(cat "$pending")" = "$existing_pid" ]; then
            pending_pid_file=$pending
            break
          fi
        done
        if [ -z "$pending_pid_file" ]; then
          printf 'Untracked %s instance %s is still running; close it before restarting dev watch.\n' "$package" "$existing_pid" >&2
          exit 1
        fi
        if [ -n "$old_pid" ]; then
          printf 'Multiple tracked %s instances (%s, %s) are still running; close the extra instance before restarting dev watch.\n' "$package" "$old_pid" "$existing_pid" >&2
          exit 1
        fi
        old_pid=$existing_pid
        recovered_pid_file=$pending_pid_file
      fi
    done
    if [ -n "$recovered_pid_file" ]; then
      rm -f "$recovered_pid_file.ready"
      mv "$recovered_pid_file" "$pid_file"
      printf 'Recovered %s instance %s from an unfinished launch.\n' "$package" "$old_pid"
    fi
    : > "$reload_lock/restarting"
    if [ -n "$old_pid" ]; then
      if is_dev_process "$old_pid"; then kill -TERM "$old_pid" 2>/dev/null || :; fi
      attempts=0
      while is_dev_process "$old_pid" && [ "$attempts" -lt "$dev_attempts" ]; do
        sleep "$dev_interval"
        attempts=$((attempts + 1))
      done
      if is_dev_process "$old_pid"; then
        printf 'Previous %s instance %s did not exit; no new instance was launched.\n' "$package" "$old_pid" >&2
        exit 1
      fi
    fi

    run_log="$PWD/target/dev-run-$package.log"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$binary" "$executable.new.$$"
    mv "$executable.new.$$" "$executable"
    cp "$repo_root/LICENSE-GPL-3.0" "$app/Contents/Resources/LICENSE"
    if [ -f "$icon_asset" ]; then
      cp "$icon_asset" "$app/Contents/Resources/AppIcon.icns"
    fi
    cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>$bundle_id</string>
<key>CFBundleName</key><string>$name Dev</string>
<key>CFBundleExecutable</key><string>$package</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

    # Each launch gets a private PID file. Late startup from an earlier
    # attempt cannot overwrite the PID belonging to the current app.
    launch_pid_file="$pid_file.launch.$$"
    : > "$launch_pid_file"
    if ! open -g -n \
      --env ZIPZI_DEV_BACKGROUND=1 \
      --env "ZIPZI_DEV_SESSION_DIR=$PWD/target/dev-session-$package" \
      --env MTL_HUD_ENABLED=1 \
      --env MTL_HUD_ALIGNMENT=bottomleft \
      --env MTL_HUD_SCALE=0.16 \
      --env "ZIPZI_DEV_PID_FILE=$launch_pid_file" \
      --env "ZIPZI_DEV_READY_FILE=$launch_pid_file.ready" \
      --env "ZIPZI_EDITOR_HOST=$executable" \
      --stdout "$run_log" --stderr "$run_log" "$app" >>"$log" 2>&1; then
      printf 'Build succeeded, but the background launch failed; details: %s\n' "$log" >&2
      exit 1
    fi

    attempts=0
    new_pid=
    while [ "$attempts" -lt "$dev_attempts" ]; do
      if [ -s "$launch_pid_file" ]; then
        new_pid=$(cat "$launch_pid_file")
        case "$new_pid" in
          reused:*)
            printf 'Resident Runtime host %s reused; this dev launch did not restart it.\n' "${new_pid#reused:}"
            rm -f "$launch_pid_file" "$launch_pid_file.ready"
            launch_pid_file=
            exit 0
            ;;
        esac
        if is_dev_process "$new_pid" && [ -s "$launch_pid_file.ready" ]; then
          break
        fi
      fi
      sleep "$dev_interval"
      attempts=$((attempts + 1))
    done
    if ! is_dev_process "$new_pid" || [ ! -s "$launch_pid_file.ready" ]; then
      printf 'Build succeeded, but the new app did not start; details: %s\n' "$run_log" >&2
      exit 1
    fi
    rm -f "$launch_pid_file.ready"
    mv "$launch_pid_file" "$pid_file"
    launch_pid_file=
    printf 'Updated %s in the background.\n' "$package"
    ;;
  run)
    exec env "ZIPZI_EDITOR_HOST=$dev_target_dir/debug/$package" MTL_HUD_ENABLED=1 MTL_HUD_ALIGNMENT=bottomleft MTL_HUD_SCALE=0.16 \
      cargo run --locked --target-dir "$dev_target_dir" -p "$package"
    ;;
  *)
    printf 'usage: %s [editor] [watch|run]\n' "$0" >&2
    exit 2
    ;;
esac
