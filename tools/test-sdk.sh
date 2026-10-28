#!/bin/sh
set -eu
tools_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec uv run --locked --project "$tools_dir/.." python "$tools_dir/dev.py" test sdk "$@"
