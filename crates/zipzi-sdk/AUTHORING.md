# Object authoring contract

C1–C6 handoff, 2026-10-22. The native document is the authority for identities,
relationships, order and domain rules. SDK plans, transactions, Python bindings
and the Editor use those same rules. This project is unreleased; native directory
format **20**, core schema **11** and observation bundle schema **6** have no
migration promise.

## Entry points

All methods below are on `AuthoringSession`, unless explicitly qualified.
Plans own typed, read-only operation descriptions; they do not grant permission
to skip validation when applying them.

| Domain | Plan / execute | Candidate transaction method |
| --- | --- | --- |
| Expression and entries | `plan_expression_edit` / `execute_expression_plan` | `EditSession::edit_expression` |
| Motion library, registrations and owned contents | `plan_motion_edit` / `execute_motion_plan` | `edit_motion` |
| Motion curves | `plan_motion_curve_edit` / `execute_motion_curve_plan` | Curve/key methods and `replace_motion_candidate` |
| Physics | `plan_physics_edit` / `execute_physics_plan` | `edit_physics` |
| Pose | `plan_pose_edit` / `execute_pose_plan` | `edit_pose` |
| Parameter, ParameterGroup and Part organization | `plan_organization_edit` / `execute_organization_plan` | `edit_organization` |
| Scene lifecycle and relationships | `plan_scene_edit` / `execute_scene_plan` | `edit_scene` |
| Mesh/Warp/Glue/Offscreen content | `plan_scene_content` / `execute_scene_content_plan` | `edit_scene_content` |
| Binding grids, BlendShapes, parameter ranges and deformation parents | `plan_binding_authoring` / `execute_binding_plan` | `binding_authoring` |
| Behavior/reference collections and masks/constraints | `plan_reference_edit` / `execute_reference_plan` | `edit_references` |
| Texture records | `plan_resource_edit` / `execute_resource_plan` | `edit_resource` |
| Attachments, sound and UserData | `plan_attachment_edit` / `execute_attachment_plan` | `edit_attachment` |
| Document fields and imported-reference repair | `plan_document_edit` / `execute_document_plan` | `edit_document` |

`*_capabilities` and `*_collection` queries expose domain availability and native
order. `CapabilityState::Unsupported` means the type has no such operation;
`Unavailable { code }` means a current condition prevents it. Bindings additionally
provide `reparent_support`, typed conversion reports and costs. A capability is
not a guarantee that an arbitrary payload or destination will pass planning.

Field addresses capture the entity, its owner/dependencies where applicable,
the expected value and monotonic context epochs. Use the matching
`*_field_address` / `plan_*_field_patch` APIs, or the specific entry, registration,
resource-name, attachment-path, drawing-range and content-address APIs. Stable
IDs identify rows; compressed execution slots and array positions do not.
Topology coordinates and Bezier handle roles remain domain-specific addresses.

## Atomicity, identity and history

- Persistent identity is document ID, object kind and entity ID. Rename, reorder
  and legal owner moves preserve it. Copies allocate new owned identities and
  remap internal references; shared external targets remain shared.
- `ObjectHandle` adds session, generation and incarnation. Deletion retires it
  permanently, including delete/recreate within one transaction. Undo/Redo can
  restore the UUID with a new incarnation. Reload/replacement changes generation.
- A plan binds the SDK version and native snapshot. Replan after a conflicting
  change. Core snapshots with equal revisions are still distinct baselines.
  Owner/context roundtrips and Undo do not revive old field addresses or requests.
- `edit`/`begin_edit`/`begin_owned_edit` use one transaction and one history. A
  late domain failure poisons ordinary candidate edits and rolls back earlier
  edits. Explicit preview APIs support recoverable invalid previews. A no-op,
  cancellation or failed edit publishes nothing and preserves redo.
- Persistent locks are validated in core/SDK, including direct replacements,
  dependency closures and targets. Editor workspace/focus/playback conditions
  are separate host checks; a headless caller does not need UI focus.
- `EditReceipt.changes` is the composable publication record for direct setters,
  domain commands, candidates, imports and Undo/Redo. Each changed transaction
  queues one receipt in `drain_events`; the receipt returned synchronously is
  the same notification. Consume it once. Undo inverts created/deleted entities
  and every before/after edge; redo uses the forward change. Whole-document
  replacement explicitly sets `full_invalidation`.

Labels/order, references, contents, topology, evaluation inputs and editor state
are independent categories. Do not treat every write as a full runtime rebuild,
or choose one domain and discard other changes in a mixed transaction. Selection,
focus, scroll and animation activation are transient Editor behavior, outside
the SDK history and `ChangeSet`.

## Domain boundaries

ParameterGroups nest; MotionGroups have one level. Motion registrations reference
clips, and group copies share clip content. Removing the last registration keeps
a visible root entry with the same registration identity and overrides. Explicit
clip deletion handles all registrations and dependencies. Pose group dissolution
requires a receiving group; Physics nodes have topology semantics. Part
dissolution requires a proof that drawing is preserved. Organization, deformation
and drawing parentage are independent relationships.

`KeepLocalData` supports structurally valid deformation moves. The two preserving
strategies support unbound/ordinary-bound Meshes, including Mesh BlendShapes,
between root and proven static invertible Rotation chains. Captured fixed driver
values can support a declared state/domain. Unproven varying driver domains,
Rotation/Warp subtree conversion and nonlinear Warp inverses are rejected with
specific diagnostics. Guarantees stop before Glue; position preservation is not
a claim about all appearance channels. No preserving request silently falls back
to a weaker strategy.

Zero-axis bindings use the target's current base fields. Activating axes,
removing them, changing shared Blend tables and remapping parameter ranges use
explicit joint conversion policies. Topology edits jointly update all dependent
forms/mappings. Externally referenced objects reject ordinary deletion; explicit
replacement/removal policies are validated for every affected user.

Pending and imported-unresolved references are distinct native states. Native
save retains them, empty groups, stable IDs, registration overrides and order.
Runtime projections filter unusable items with native↔execution maps and reason
chains; a missing mask never falls back to unmasked drawing. Available-content
export uses the same eligibility and lists omissions; format-specific limits
are additional diagnostics. Strict external export rejects incomplete content.
Third-party formats use external IDs rather than leaking internal identities.
Opaque extensions and source metadata retain their explicit import/export
protection; generic property editing cannot rewrite them as arbitrary JSON.

## Resource work and finite defaults

`image_load_token` / `attachment_load_token` bind session, generation, incarnation,
monotonic content epoch and cancellation state. `prepare_image_load` performs IO,
decode and hashing outside the session lock; `replace_prepared_image_content`
hands over immutable pixels and checks the token again before publishing.
Name/path changes preserve requests; newer requests, content edits, Undo and
deletion/recreation retire old ones. Cancellation/failure creates no content or
history. Unchanged successful publication consumes the request without history.
Snapshots/history can retain the previous immutable blob after replacement.

| Budget | Default |
| --- | --- |
| Binding sample combinations | 65,536 |
| Estimated conversion temporary bytes | 64 MiB |
| Estimated conversion history bytes | 256 MiB |
| SDK history | 50 steps, 256 MiB |
| Encoded image | 64 MiB |
| Image axis | 16,384 pixels |
| Decoded image RGBA | 256 MiB |
| Observation retained textures (encoded + RGBA) | 256 MiB |
| Observation manifest / each encoded texture | 64 MiB / 64 MiB |

`AuthoringLimits` is session configuration, separate from `HistoryLimits`.
Conversion preflight precedes Cartesian allocation, with temporary estimate
`6 × retained + 2 × payload` and history estimate `2 × retained + payload`.
These are finite estimates for SDK work, not allocator/RSS/GPU memory caps.

## Diagnostics and recovery

| Code/family | Caller action |
| --- | --- |
| `OBJECT_LOCKED`, `OBJECT_REFERENCED` | Present the condition/users; explicitly unlock or select a dependency policy |
| `STALE_HANDLE`, `STALE_FIELD_ADDRESS`, `STALE_VERSION`, `STALE_PLAN` | Retire the old gesture/request and capture/replan from current state |
| `NEEDS_CONFIGURATION`, `SCENE_NEEDS_CONFIGURATION`, missing/unresolved target diagnostics | Repair the typed slot or explicitly accept an available-content export plan |
| `AUTHORING_CAPACITY_OVERFLOW`, `AUTHORING_GRID_LIMIT`, `AUTHORING_MEMORY_LIMIT`, `AUTHORING_HISTORY_LIMIT` | Reduce the operation or explicitly configure an appropriate budget, then replan |
| `PARAMETER_DOMAIN_UNPROVEN`, `SUBTREE_CONVERSION_UNPROVEN`, `NONLINEAR_PARENT_UNPROVEN` | Present the failed guarantee; do not claim a preserving conversion occurred |
| `SOURCE_METADATA_READ_ONLY` | Use the explicit import/source workflow |
| `EDIT_ABORTED`, cancellation diagnostics | Discard the candidate; retain the committed model/history |

Codes remain domain-specific; callers should retain the returned operation,
affected IDs, referrers, conversion report and omission chain. A new plan with
more deletion or conversion loss requires a new acceptance by the caller.

Cross-domain regression entry: `cargo test --locked -p zipzi-sdk --test integration
authoring_acceptance::`. Domain suites remain registered in `tests/integration.rs`.
Release probes: `c6_authoring_benchmark` for queries/structure, and
`c4_authoring_benchmark` for grid capacity, measured allocations and cancellation.
