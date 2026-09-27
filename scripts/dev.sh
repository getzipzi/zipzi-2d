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

case "${2:-watch}" in
  watch)
    if ! command -v watchexec >/dev/null 2>&1; then
      printf 'watchexec is required. Install it with: brew install watchexec\n' >&2
      exit 1
    fi
    printf 'Watching %s; successful builds relaunch the app in the background.\n' "$package"
    exec watchexec --on-busy-update=queue --exts rs,toml \
      --watch . --watch ../kasane-2d/modules \
      -- ./scripts/dev.sh "${1:-editor}" reload
    ;;
  reload)
    mkdir -p target
    log="target/dev-build-$package.log"
    if ! cargo build --locked -p "$package" >"$log" 2>&1; then
      printf 'Build failed; full output: %s\n' "$log" >&2
      tail -n 80 "$log" >&2
      exit 1
    fi

    app="$PWD/target/$name Dev.app"
    executable="$app/Contents/MacOS/$package"
    pid_file="$PWD/target/dev-$package.pid"
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
    exec cargo run --locked -p "$package"
    ;;
  *)
    printf 'usage: %s [editor|playground] [watch|run]\n' "$0" >&2
    exit 2
    ;;
esac
