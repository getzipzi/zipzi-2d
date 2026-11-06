# Python SDK capabilities

Local and Connected use the same generated v4 operations. Check host support
with `runtime.capabilities()` and admission limits with `runtime.limits()`.
The installed package includes the typed `zipzi.values` module, a guide and an
executable recipe. [Setup and usage](README.md)

| Capability | Public entry | Behavior |
| --- | --- | --- |
| Local and Connected hosts | `zipzi.local()`, `zipzi.connect()` | Local loads the optional native adapter. Connected attaches to a private endpoint and can start an Editor host. |
| Session lifecycle | `runtime.sessions.create/list/attach`, `session.info/detach/close` | Attached clients share a resident document and history. Detach retains the Session. Closing checks unsaved work and active owners. |
| Desktop services | `session.editor_context()`, `session.show_window()`, `runtime.generate_editor_mesh()` | Editor context is versioned. The bounded mesh service returns detached geometry for a subsequent Draft. |
| Authoring | `session.draft()`, `draft.apply/apply_async/preview/publish/cancel` | Generated commands cover meshes, bindings, scene and organization, resources, references, deletion, and animation. A rejected gesture preview retains its last valid candidate. An accepted failing batch ends the Draft. |
| Queries and history | `session.query()`, `draft.query()`, `session.undo/redo` | Object reads use stable IDs. Collection offsets belong to an exact source. Each changed publication contributes one history step. |
| Events | `session.subscribe()`, `session.events(cursor)` | Each reader keeps its own Version cursor and resubscribes after an event gap. |
| Images and bulk transfer | `runtime.blob()`, `draft.replace_image_blob()`, `session.image_blob()`, `job.blob()` | Explicit owned handles transfer data in bounded binary chunks. |
| Projects | `session.open/import_project/save/export` | Jobs capture a source and report their actual outcome. Supported imports are PSD, Model3 and Cubism. Exports target runtime packages or Cubism. |
| Sampling and spatial queries | `session.samples()`, `SampleInputParameters`, `SampleInputAnimation` | Modeling and playback recipes can request spatial queries and mesh quality on frozen sources. Results include source identity. |
| GPU observation | `session.observe()`, `ObservationView` | A GPU-capable host returns PNG packets and a texture report. Views support texture policy, topology overlays and quality metrics. |
| Recovery | `zipzi.recover_call(error)`, `Job.wait()` | Recover a lost response with its original request ID. A wait timeout leaves the Job owned. |

## Manual UV mesh editing

Submit `CommandManualMesh` on a Draft using the exact source returned by that
Draft. Use `draft.preview()` for repeated gesture positions, followed by
`draft.publish()`. The host retains stable vertex identities and transfers base
positions, ordinary bindings and BlendShape deltas together. It validates Glue
endpoints, and reports `GLUE_VERTEX_REFERENCED` when a vertex still belongs to
a pair.

## Validation

```sh
./tools/test-sdk.sh python
./tools/test-sdk.sh python --recovery
./tools/test-sdk.sh python --discovery
./tools/test-sdk.sh python --observation
./tools/test-sdk.sh python-connected --editor-host /absolute/path/to/zipzi-editor
```

The Editor host gate includes detached mesh generation and Draft publication.
GPU observation requires a usable GPU.
