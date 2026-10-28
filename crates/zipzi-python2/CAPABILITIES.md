# Python SDK capabilities

The `zipzi` client uses the same Rust Runtime operations for local and connected
sessions. Use `runtime.capabilities()` and `runtime.limits()` to check what a
host provides and the limits it enforces. See the [README](README.md) for setup
and the [guide](client/python/zipzi/docs/guide.md),
[API reference](client/python/zipzi/docs/reference.md) and
[recipes](client/python/zipzi/docs/recipes.py) for usage.

## Connections and lifecycle

| Capability | Public entry | Notes |
| --- | --- | --- |
| Local Runtime | `zipzi.local()` | Requires the optional `zipzi-native` package. |
| Connected Runtime | `zipzi.connect()` | Discovers a resident host; accepts an explicit endpoint or can start a specified Editor host. The pure client does not require `zipzi-native`. |
| Sessions | `runtime.sessions.create/list/attach`, `session.info`, `session.detach/close` | Attached clients share the formal document and history. Detach retains the host session; close releases it subject to active work, other clients and unsaved changes. |
| Editor window | `session.show_window()` | Requires a desktop host. Edits update the resident document; saving is separate. |
| Events | `session.subscribe()`, `runtime.sessions.subscribe()`, `runtime.events_since()` | Readers have independent cursors; use `resync()` after a gap. |
| Jobs | `state`, `cancel`, `wait`, `close` | A finite wait may raise `JobTimeout` without cancelling the accepted job. Wait for or close jobs to release resources. |
| Recovery | `OutcomeUnknown.recover()`, `zipzi.recover_call()` | Recovers the original call and result after a lost response. An unresolved Runtime can be closed with `abandon_pending=True` to forfeit recovery. |
| Offline documentation | `zipzi.describe()` | Describes the installed API without a host or native extension. |

## Authoring and queries

| Capability | Public entry | Notes |
| --- | --- | --- |
| Edits and history | `session.edit()`, `edit.apply/apply_many`, commit, undo and redo | Operations share an isolated candidate and publish through the same document history. A failed write aborts its candidate. |
| Meshes | `session/edit.meshes`, `meshes` operations | Create and edit meshes, positions and topology; query individual meshes or batches. |
| Parameters and bindings | `session/edit.parameters`, `session/edit.bindings`, `parameters` and `bindings` operations | Create, patch and query parameters and mesh bindings, including keyforms. Rust allocates identities and validates inputs. |
| Scene and modeling | `session/edit.scene`, `modeling.ModelingCommand*` | Parts, groups, deformers, BlendShapes, Glue, Offscreen, organization, topology and conversions use SDK modeling operations. |
| Animation authoring | `modeling.ModelingCommandMotion/Expression/Physics/Pose` | Edit animation assets and query selected records, plans, addresses and capabilities. |
| Plans and references | `session/edit.plan()`, `session.reference()`, `session.address()` | Plans retain their source and cost/loss report. References and addresses are subject to identity and field-epoch validation. |
| Reads and collections | `get`, `get_many`, `list` | Reads return source-bound `Snapshot` or `Batch` values. Collection pages have source-bound cursors; restart pagination on `STALE_SOURCE`. |
| Attachments | `modeling.ModelingCommandAttachment` and selected records | Attachments use modeling operations and project I/O. |

## Resources, projects and observation

| Capability | Public entry | Notes |
| --- | --- | --- |
| Images and buffers | `runtime.resources.prepare_image/prepare_buffer`, `session/edit.resources` | Prepared images, buffers and image snapshots have explicit lifetimes. Large payloads use bounded binary transfer. |
| Project I/O | `runtime.sessions.import_()`, `session.project` | Jobs cover new, open, import, save, diagnostics and export. Supported formats include PSD, Model3/MOC3, Cubism projects and runtime sidecars; format-specific loss rules apply. |
| CPU modeling samples | `session/edit.modeling.sample()` | Evaluates explicit poses against a frozen source without changing history or Editor preview. |
| Animation preview | `session/edit.preview()` | Independent frozen playback supports controls and sample, seek and advance jobs. It does not change the Editor's playback state. |
| Spatial queries | `session/edit.modeling.spatial()`, Preview `queries` | Bounds and hit queries use frozen poses or animation frames. |
| GPU observation | `modeling.observe()`, Preview `view`, `inspection.save()` | Requires a host with GPU observation and a usable device. Inspection saves PNG/JSON output locally. |
| Editor mesh generation | `zipzi.mesh_generation.generate()` | Connected Editor hosts may offer this synchronous, bounded service. Local hosts do not provide it; generation returns geometry without modifying the document. |

The packages target CPython 3.14; source tests run on macOS/Unix. The desktop
host, GPU features and platform-specific installation must meet their own
requirements. Limits can also require smaller batches or payloads.

## Validation

Run from the repository root:

```sh
./tools/test-sdk.sh python2
./tools/test-sdk.sh python2 --recovery
./tools/test-sdk.sh python2 --discovery
./tools/test-sdk.sh python2-connected
# Requires a usable GPU:
./tools/test-sdk.sh python2 --observation
```

For Editor mesh generation, run the focused test against a production host:

```sh
./tools/dev-python crates/zipzi-python2/tests/editor_mesh_generation.py --host /absolute/path/to/zipzi-editor
```
