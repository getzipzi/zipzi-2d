# zipzi

Pure Python authoring client for Zipzi. Import it as `zipzi` and use
`zipzi.connect()` to attach to a resident desktop host. Install `zipzi-native`
to use the in-process `zipzi.local()` backend.

Use `runtime.sessions.list()` and `sessions.attach(info.id)` to work with an
existing document. Changes made through an Edit share the Editor's document and
undo history; saving is separate. Reads return source-bound `Snapshot` or `Batch`
values. Jobs must be waited or closed to release their resources.

If a response is lost, recover the original call with `OutcomeUnknown.recover()`
or `zipzi.recover_call(bound_sdk_method, ...)`. A timed-out `job.wait()` does
not cancel the job. Close an unresolved Runtime only after recovery or with
`abandon_pending=True`.

The package includes `zipzi/docs/guide.md`, `reference.md` and `recipes.py`;
`zipzi.describe("zipzi")` works offline. See the [package source](https://github.com/getzipzi/zipzi-2d/tree/main/crates/zipzi-python2)
for setup, examples and tests.

For an Editor handoff, run:

```sh
python -I -m zipzi.docs.editor_handoff list
python -I -m zipzi.docs.editor_handoff rename \
  --runtime RUNTIME_UUID --session SESSION_SERIAL --mesh MESH_UUID --name "Refined face"
```

Pass `--endpoint /absolute/path/to/profile.sock` before the subcommand to select
a particular host.
