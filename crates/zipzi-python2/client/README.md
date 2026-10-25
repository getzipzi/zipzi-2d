# zipzi-python2

Pure Python authoring client. Import as `zipzi`. The optional
`zipzi-python2-native` distribution supplies `zipzi_native._native` for
`zipzi.local()`; connected clients can import and run without it.

See the [package source](https://github.com/getzipzi/zipzi-2d/tree/main/crates/zipzi-python2)
for the currently implemented scope and validation commands.

`zipzi.connect()` discovers a resident desktop host. Use `start=True` with an
absolute `host=Path(...)` or `ZIPZI_EDITOR_HOST` to launch it under the same
endpoint ownership rule. `runtime.sessions.list()` and `sessions.attach(info.id)`
bind existing unsaved sessions; `session.show_window()` displays one in the
desktop host. Typed parameter create/patch/batch reads, mesh batch reads and
independent cursor subscriptions are implemented. Runtime R4 full Modeling,
handles/addresses, CPU evaluation and resource/IO coverage remain in progress.
