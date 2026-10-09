#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

case "${1:-editor}" in
  editor)
    package=kasane-editor
    name='Kasane Editor'
    bundle_id=dev.kasane.editor.watch
    icon_asset=assets/kasane-editor-icon-vector.icns
    ;;
  playground)
    package=component-playground
    name='Kasane Component Playground'
    bundle_id=dev.kasane.component-playground.watch
    icon_asset=assets/AppIcon.icns
    ;;
  *)
    printf 'usage: %s [editor|playground] [watch|run]\n' "$0" >&2
    exit 2
    ;;
esac

if [ ! -d ../kasane-2d/modules ]; then
  printf 'missing sibling checkout: ../kasane-2d/modules\n' >&2
  exit 1
fi

# The workspace disables incremental builds; enable them for this dev session.
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}"

app="$PWD/target/$name Dev.app"
executable="$app/Contents/MacOS/$package"
pid_file="$PWD/target/dev-$package.pid"
watch_lock="$PWD/target/dev-$package.watch.lock"
reload_lock="$PWD/target/dev-$package.reload.lock"
owned_lock=
launch_pid_file=

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
    watchexec --on-busy-update=queue --exts rs,toml \
      --watch . --watch ../kasane-2d/modules \
      -- ./scripts/dev.sh "${1:-editor}" reload &
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
          sleep 0.5
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
      sleep 0.5
    done
    ;;
  reload)
    acquire_lock "$reload_lock" || exit 1
    cleanup_reload() {
      if [ -n "$launch_pid_file" ]; then rm -f "$launch_pid_file"; fi
      release_lock
    }
    trap cleanup_reload 0
    trap 'exit 130' INT
    trap 'exit 143' TERM
    trap 'exit 129' HUP
    mkdir -p target
    log="target/dev-build-$package.log"
    # GPUI FPS is temporarily disabled; omit the optional debug-tools feature.
    if ! cargo build --locked -p "$package" >"$log" 2>&1; then
      printf 'Build failed; full output: %s\n' "$log" >&2
      tail -n 80 "$log" >&2
      exit 1
    fi

    old_pid=
    if [ -s "$pid_file" ]; then
      old_pid=$(cat "$pid_file")
      if ! is_dev_process "$old_pid"; then old_pid=; fi
    fi
    # Do not guess ownership or close untracked instances with unsaved work.
    # Refuse another open -n when a previous launch or another watcher left one.
    existing_pids=$(dev_pids)
    for existing_pid in $existing_pids; do
      if [ "$existing_pid" != "$old_pid" ]; then
        printf 'Untracked %s instance %s is still running; close it before restarting dev watch.\n' "$package" "$existing_pid" >&2
        exit 1
      fi
    done
    : > "$reload_lock/restarting"
    if [ -n "$old_pid" ]; then
      if is_dev_process "$old_pid"; then kill -TERM "$old_pid" 2>/dev/null || :; fi
      attempts=0
      while is_dev_process "$old_pid" && [ "$attempts" -lt 50 ]; do
        sleep 0.1
        attempts=$((attempts + 1))
      done
      if is_dev_process "$old_pid"; then
        printf 'Previous %s instance %s did not exit; no new instance was launched.\n' "$package" "$old_pid" >&2
        exit 1
      fi
    fi

    run_log="$PWD/target/dev-run-$package.log"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "target/debug/$package" "$executable.new.$$"
    mv "$executable.new.$$" "$executable"
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

    # Each launch gets a private readiness file. Late startup from an earlier
    # attempt cannot overwrite the PID belonging to the current app.
    launch_pid_file="$pid_file.launch.$$"
    : > "$launch_pid_file"
    if ! open -g -n \
      --env KASANE_DEV_BACKGROUND=1 \
      --env "KASANE_DEV_SESSION_DIR=$PWD/target/dev-session-$package" \
      --env MTL_HUD_ENABLED=1 \
      --env MTL_HUD_ALIGNMENT=bottomleft \
      --env MTL_HUD_SCALE=0.16 \
      --env "KASANE_DEV_PID_FILE=$launch_pid_file" \
      --stdout "$run_log" --stderr "$run_log" "$app" >>"$log" 2>&1; then
      printf 'Build succeeded, but the background launch failed; details: %s\n' "$log" >&2
      exit 1
    fi

    attempts=0
    new_pid=
    while [ "$attempts" -lt 50 ]; do
      if [ -s "$launch_pid_file" ]; then
        new_pid=$(cat "$launch_pid_file")
        if is_dev_process "$new_pid"; then
          break
        fi
      fi
      sleep 0.1
      attempts=$((attempts + 1))
    done
    if ! is_dev_process "$new_pid"; then
      printf 'Build succeeded, but the new app did not start; details: %s\n' "$run_log" >&2
      exit 1
    fi
    mv "$launch_pid_file" "$pid_file"
    launch_pid_file=
    printf 'Updated %s in the background.\n' "$package"
    ;;
  run)
    exec env MTL_HUD_ENABLED=1 MTL_HUD_ALIGNMENT=bottomleft MTL_HUD_SCALE=0.16 \
      cargo run --locked -p "$package"
    ;;
  *)
    printf 'usage: %s [editor|playground] [watch|run]\n' "$0" >&2
    exit 2
    ;;
esac
