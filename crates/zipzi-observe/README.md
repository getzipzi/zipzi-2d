# zipzi-observe

Frozen snapshot capture and explicit-view GPU rendering for Runtime observation
jobs. Capture consumes SDK `SourcedSnapshot`: QuerySource identifies the exact
published version or Draft identity/base/sequence, while ContentId identifies
frozen content. Animation previews bind that source with `bind_source`.
Undo does not make an old preview current again. Different Drafts and candidate
sequences cannot share a batch merely because they contain the same content node.

- `ObservationInput::capture_from_snapshot` evaluates one parameter pose.
- `capture_motion_from_snapshot` validates source and content, then applies
  Model opacity before mask / Offscreen composition.
- `ResolvedObservation::capture_many_with_control` prepares textures once per
  batch; `capture_many_with_plan` checks the host's metadata plan before acquisition.
- `Observer::new` and `render` reuse a device, renderer and texture cache across
  explicit ROI renders. RenderRequest preserves aspect ratio and model-to-image mapping.
- Results carry QuerySource and ContentId, RGBA8/PNG, effective parameters,
  drawable bounds, texture hashes and backend identity. CPU captures expose
  `source()` and `content_id()` for host completion handling.

Inputs retain ResourceView image versions. Preparation reads verified captured
pixels without reopening files or encoding PNG; no asset root or
ProjectResourceLease is needed. Captures remain usable after Session closure and
source archive/image deletion. ResourceKey is not a content hash: scene digest v4
uses semantic asset descriptions and verified pixel hashes, so equal pixels in
distinct resource versions have equal digests.

The native backend is Metal on macOS and WGPU elsewhere. The `wgpu-backend`
feature selects WGPU on macOS too. Texture filtering uses linear mipmaps and
repeat sampling. Rendering requires a usable device; capture and digest checks
run on CPU. Geometry bounds do not prove unoccluded painted pixels.

Runtime owns job admission, cancellation, budgets and source lifetime. A batch
keeps its resolved textures until rendering finishes, without revisiting live
asset paths. Authoring records remain internal inputs to content digests and
batch validation. There is no scene save/open API or automatic fitted-view path.
Python inspection exports images and metadata, not a reloadable scene.

`render_project_preview` is the automatic fit path used by GPU Runtime saves. It
uses a frozen snapshot at default parameter values, visible drawable bounds and
4% padding, fits textures to 32 MiB, and renders a transparent 1024 × 1024 static
image with no editing overlays. Failures are advisory to project saving. CPU-only
hosts omit previews. This does not alter the live pose, history or document dirty
state. Runtime admits an extra conservative 256 MiB before attempting the render.

`planning::texture_sources` collects the frozen document's mesh texture references,
including hidden meshes and mesh/Offscreen mask sources, deduplicated by asset ID.
Unused assets do not enter the base RGBA budget. `planning::plan_textures` accepts
the shared `ObservationTexturePolicy` contract and an explicit host base-byte
limit. It uses checked integer dimensions, returns a sorted target table and exact
non-base mipmap byte totals, or retains source metadata in an insufficient-budget
diagnostic. Planning reports sizes only; execution verifies captured pixel content.
The planner does not read images, evaluate poses, reserve resources or initialize
a GPU; compile with `--no-default-features` to exclude all renderer dependencies.
The default `gpu` feature retains capture rendering through `zipzi-observe-backend`:
macOS uses Metal without a WGPU dependency; other platforms use WGPU.
`wgpu-backend` explicitly enables WGPU on macOS as well.
Runtime and Python expose `Original` (default) and explicit `FitBudget` policies.
The latter chooses the greatest integer longest-edge cap that satisfies the base
RGBA budget. Preparation executes the admitted table without silently replanning:
it verifies dimensions and hashes on captured straight RGBA8 pixels and filters
those pixels directly. Original dimensions reuse immutable pixel Arcs; fitting
reserves target conversion and source-alpha workspace. File decoding limits apply
when ResourceStore captures images, before they can enter a Snapshot.
Alpha-weighted area filtering and each-axis floor-halved mips have separately
versioned rules.

`texture_report()` shares one sorted report across a resolved batch. Runtime
returns that report once on its observation/Preview batch; individual images do
not repeat its texture table. Captured resource identities use dimension/domain-tagged
RGBA hashes. Scene digest v4 describes source content;
render/input digest v2 includes actual render dimensions/pixels and algorithm
versions, and stays equal for policies producing equivalent rendering inputs.
QuerySource identifies the observation origin; ResourceView retains the matching
immutable pixel versions. Estimates use metadata and execution checks those pixels.

Source alpha samples are extracted before downsampling only for coverage quality,
with original dimensions and the existing at-most-256x256 sample grid. Strain
alone does not retain them. Original/FitBudget share coverage results. The output
PNG writer enforces the same conservative capacity used by Runtime admission.

`RenderRequest.topology` optionally composites diagnostic wireframe, vertices,
persistent vertex IDs, stored boundary/constraint edges and default-pose edge
strain in Rust after GPU readback. `topology_only` clears appearance first.
These layers use the exact `ViewMapping` and ignore masks/occlusion. Both render
and input digests include the overlay options. Optional Mesh/Part `targets` select
the diagnostic layers and quality reports while retaining the full context image.
Parts include descendants; an empty explicit selection draws/reports no meshes.
Labels use a bounded global screen-space layout, nearby leaders, adjustable scale,
spacing and count; crowded labels are omitted. `anomalies_only` restricts all layers
to threshold violations, flips, degeneracy and undefined reference ratios.
Heat uses a separate most-severe-per-pixel layer blended once at `heat_opacity`,
preventing opacity accumulation from shared edges or overlapping mesh layers.
`RenderRequest.quality` includes
per-mesh model-space metrics and source-alpha UV coverage at at most 256x256
regular pixel-center samples (alpha >= 8). Coverage ignores filtering, atlas
ownership and out-of-range repeat; it reports sampling step and denominators.
Capture with `capture_*_from_snapshot_with_quality(..., true)` before requesting
quality or strain. The reference uses default parameters of that frozen source,
including hidden meshes. CPU-only sampling exposes the same geometric metrics
through `GeometryChecks.quality`, without texture coverage. Reports certify only
the requested poses. Coverage and overlay CPU work have explicit bounds.

Focused CPU checks:

```sh
cargo test --locked -p zipzi-observe --lib
cargo test --locked -p zipzi-observe --no-default-features --lib
cargo test --locked -p zipzi-observe --test capture cancelled_capture
cargo test --locked -p zipzi-observe --test capture candidate_animation
cargo test --locked -p zipzi-observe --test capture batch_samples
```

GPU checks:

```sh
cargo test --locked -p zipzi-observe --test capture
cargo test --locked -p zipzi-observe --features wgpu-backend --test capture
```

Runtime, Editor and Python adapters are still undergoing stage-four migration;
their gates are not currently passing. Once wired, Runtime's observation target
and `./tools/test-sdk.sh python --observation` cover these consumers. See the
[architecture status](../../docs/archive/plans/authoring-architecture.md).
`ObservedFrame.adapter_backend` distinguishes native `Metal` from `wgpu (Metal)`.
