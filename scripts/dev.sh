#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

case "${1:-editor}" in
  editor)
    package=kasane-editor
    name='Kasane Editor'
    bundle_id=dev.kasane.editor.watch
    ;;
  playground)
    package=component-playground
    name='Kasane Component Playground'
    bundle_id=dev.kasane.component-playground.watch
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

is_dev_process() {
  case "$1" in
    ''|*[!0-9]*) return 1 ;;
  esac
  command_line=$(ps -p "$1" -o command= 2>/dev/null) || return 1
  case "$command_line" in
    "$executable"|"$executable "*) return 0 ;;
    *) return 1 ;;
  esac
}

case "${2:-watch}" in
  watch)
    if ! command -v watchexec >/dev/null 2>&1; then
      printf 'watchexec is required. Install it with: brew install watchexec\n' >&2
      exit 1
    fi
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
        # Reload writes the next PID before terminating the previous app.
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
    mkdir -p target
    log="target/dev-build-$package.log"
    # GPUI FPS is temporarily disabled; omit the optional debug-tools feature.
    if ! cargo build --locked -p "$package" >"$log" 2>&1; then
      printf 'Build failed; full output: %s\n' "$log" >&2
      tail -n 80 "$log" >&2
      exit 1
    fi

    run_log="$PWD/target/dev-run-$package.log"
    mkdir -p "$app/Contents/MacOS"
    cp "target/debug/$package" "$executable.new.$$"
    mv "$executable.new.$$" "$executable"
    cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>$bundle_id</string>
<key>CFBundleName</key><string>$name Dev</string>
<key>CFBundleExecutable</key><string>$package</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

    old_pid=
    if [ -f "$pid_file" ]; then
      old_pid=$(cat "$pid_file")
      if ! is_dev_process "$old_pid"; then
        old_pid=
      fi
    fi
    : > "$pid_file"
    if ! open -g -n \
      --env KASANE_DEV_BACKGROUND=1 \
      --env MTL_HUD_ENABLED=1 \
      --env MTL_HUD_ALIGNMENT=bottomleft \
      --env MTL_HUD_SCALE=0.16 \
      --env "KASANE_DEV_PID_FILE=$pid_file" \
      --stdout "$run_log" --stderr "$run_log" "$app" >>"$log" 2>&1; then
      printf '%s' "$old_pid" > "$pid_file"
      printf 'Build succeeded, but the background launch failed; details: %s\n' "$log" >&2
      exit 1
    fi

    attempts=0
    new_pid=
    while [ "$attempts" -lt 50 ]; do
      if [ -s "$pid_file" ]; then
        new_pid=$(cat "$pid_file")
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
    if [ -n "$old_pid" ] && [ "$old_pid" != "$new_pid" ]; then
      kill -TERM "$old_pid"
    fi
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
