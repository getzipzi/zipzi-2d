# kasane-cmo3

Native Rust export from `kasane_core::Document` to a self-contained, editable
Cubism `.cmo3` model and linked `.can3` animation projects. The output uses the
Cubism Editor 5.3 document schema and SDK target. The encoder does not invoke Java, Cubism Editor or a native Cubism
SDK. PSD2Live/Umamo's local implementation was consulted for CAFF and XML
wire-format conventions; it is not a build or runtime dependency.

## API

```rust,no_run
use kasane_cmo3::{encode_cmo3, AssetResolver, Cmo3Artifact, Cmo3Error, Cmo3Options};
use kasane_core::Document;

fn convert(document: &Document, assets: &impl AssetResolver)
    -> Result<Cmo3Artifact, Cmo3Error>
{
    encode_cmo3(document, assets, &Cmo3Options::default())
}
```

`AssetResolver` reads encoded PNG bytes for an `ImageAsset`; closures implement
it too. PNG dimensions are checked, and project adapters additionally verify
saved asset hashes. Output bytes and serializable diagnostics are returned
together. Encoding reads the committed document without changing its revision,
preview, history or resources. Fixed inputs/options produce identical bytes.

For a linked model/animation pair, use:

```rust,no_run
use kasane_cmo3::{encode_cubism, CubismArtifact, CubismOptions, AssetResolver, Cmo3Error};
use kasane_core::Document;

fn convert_pair(document: &Document, assets: &impl AssetResolver)
    -> Result<CubismArtifact, Cmo3Error>
{
    encode_cubism(document, assets, &CubismOptions {
        base_name: "avatar".into(),
        ..Default::default()
    })
}
```

The artifact contains `avatar.cmo3`, `avatar.can3` and a combined report. CAN3
links the sibling model by filename and uses its original part GUIDs and
parameter runtime IDs. `encode_can3(document, &Can3Options)` captures animation
alone without asset I/O; `model_file_name` specifies the sibling `.cmo3`.
`inspect_can3` and `inspect_cubism` expose preflight diagnostics for a future CLI.

`inspect_export` reports conversion losses without resource I/O. The default
`LossPolicy::Reject` returns `Cmo3Error::Unsupported(report)` before reading
images. Explicit `LossPolicy::Allow` produces a file with those same diagnostics;
callers should surface the report instead of silently discarding it.

## Project and SDK adapters

`kasane-project` provides:

- `encode_project_cmo3(source, options)` for a saved project directory or manifest.
- `encode_document_cmo3(document, asset_root, options)` for a captured document.
- `DocumentSession::encode_cmo3(options)` including unsaved/in-memory PSD artwork.
- `publish_cmo3(artifact, destination)` for staged, atomic file publication.
- `DocumentSession::export_cmo3(destination, options)` combining encoding and publication.

For paired export, `encode_project_cubism`, `encode_document_cubism` and
`DocumentSession::encode_cubism` return the linked artifact. `publish_cubism`
publishes both files and `export-report.json` into a managed output directory.
It stages all files before replacing an existing export and restores the old
pair if the commit rename fails. Replacement refuses directories containing
files other than the same model/animation filenames and report, protecting
source projects and unrelated files. The parent directory must already exist.
Directory replacement uses a backup/rename sequence; it is rollback-safe on
reported failures, but does not promise a crash-atomic two-file transaction.
`DocumentSession::export_cubism` combines those operations. Animation-only
adapters are `DocumentSession::{encode_can3, export_can3}` and `publish_can3`.

`kasane_sdk::AuthoringSession::{encode_cmo3, encode_can3, encode_cubism}` expose
the same in-memory artifacts.
Encoding and publication are separate so hosts can inspect diagnostics, choose
a destination and publish the exact bytes they reviewed. A failed encoding or
pre-rename publication preserves an existing output. A post-rename directory
sync failure is reported as a `Publication` warning.

Example (absolute paths):

```sh
cargo run --locked -p kasane-project --example project_cmo3 -- \
  /path/to/project /path/to/model.cmo3
# Linked model + animation in /path/to/editable/avatar.{cmo3,can3}:
cargo run --locked -p kasane-project --example project_cubism -- \
  /path/to/project /path/to/editable avatar
```

## Preserved model data

| Data | Representation in CMO3 |
| --- | --- |
| Canvas, origin, pixels per unit, runtime Y reversal | Canvas/model information, reflected root forms, rotation orientation, UVs and winding |
| Parameters, ranges, defaults, repeat, names, folders | Parameter sources and group tree |
| Parts and representable drawing groups | Part sources and child GUIDs |
| Ordinary parameter grids | Grid bindings, access keys and absolute forms |
| Mesh vertices, UVs and triangles | ArtMesh source/forms, editable points, edges and UIDs |
| Rotation/warp deformers and their keyed appearance | Deformer sources/forms, with coordinate and scale conversion |
| Mesh/warp/rotation/part/Glue/Offscreen blend shapes | Morph forms relative to the ordinary default pose, with weight constraints |
| Clipping and inverted masks | Drawable/part clipping GUIDs |
| Legacy and extended color/alpha blending | ColorComposition and AlphaComposition |
| Glue pairs, weights and keyed intensity | Editable Glue sources/forms |
| Offscreen composite appearance | Owner part's offscreen state and forms |
| X/angle physics, vertices, normalization and integral FPS | Editable physics settings |
| Artwork, including unused assets | Embedded PNG atlases and texture-input extensions |

Texture-atlas mode keeps meshes and texture pages editable. Original PSD layer
names, source-layer layout, painting history and undo history cannot be recovered
from a Kasane texture asset. Kasane Bezier handles and palette preferences are
not serialized; the sampled warp lattice remains editable. These are reported as
informational diagnostics. This exporter does not reconstruct an original PSD.

Known losses are reported explicitly: nonzero blend-neutral keys, multiple
blend curves for the same target/driver, fractional draw orders/canvas values,
drawing-group trees or tie orders that cannot fit Cubism's part hierarchy, Y
physics channels, custom physics forces or opaque JSON extensions, unresolved
CDI metadata, combined-parameter metadata, and separate motion/expression/pose
or package attachments, and model3 effect groups/layout/hit areas/UserData.
Runtime sidecars can still be exported through the
existing `kasane-project` APIs. Editable meshes above 32,767 vertices and invalid
resources/documents are rejected.

## Preserved animation data

Every Kasane motion clip becomes a named, editable CAN3 scene in document
order, including clips containing only events. A document without motion clips
gets one empty scene linked to the model so animation can be authored later.

| Data | Representation in CAN3 |
| --- | --- |
| Duration, FPS, loop and restricted Bezier flag | Scene movie information |
| Parameter tracks | Live2D parameter attributes linked by runtime ID |
| Part opacity tracks | Part visibility attributes linked by runtime ID and CMO3 GUID |
| Model opacity | Visual effect opacity curve |
| Linear, Bezier, stepped and inverse-stepped segments | Mutable sequences, editable anchors, handles and curve types |
| Clip/track fade durations | Scene fades and attribute options in milliseconds |
| Motion events | Model-track user data at frame indices |

CAN3 anchors, duration and events use integer frames; Bezier handles retain
fractional frame positions. Times are rounded to the nearest frame, and fades
to the nearest millisecond. Changes beyond float noise produce loss diagnostics.
Scenes shorter than one frame also report their extension to the minimum duration.
Colliding anchors/events retain the last item only with `LossPolicy::Allow`.
Duplicate tracks (including normalized part-ID collisions) retain the first.
Inverse-stepped segments retain their authored keys/type, but Animator displays
the anchor value at the first frame of the segment while Kasane samples its
endpoint. When those values differ this boundary mismatch is an explicit loss.
Unresolved targets, aggregate EyeBlink/LipSync or unknown model-effect curves,
opaque motion extensions, and registration-specific fades/sound references
are reported as losses. Plain model3 motion groups are encoded as native CAN3
playlists, preserving names, order, repeated entries and empty groups alongside
the normal all-scenes playlist. A reserved playlist GUID namespace records
which playlists are model3 registrations; ordinary native Editor playlists are
not automatically treated as runtime registrations. Known registered motion
scenes retain their identity even if they contain only initial values.
Use explicit parameter curves for editable eye/mouth
animation. Add expressions become scenes containing absolute parameter initial
values: the current model default plus the expression offset. Missing expression
Blend is also Add. Native CAN3 scenes with parameter values only at frame zero and no
later keys or events import as expressions, regardless of name or duration;
even an equal-valued second key keeps a scene classified as a motion. Empty
scenes remain motions. Static part visibility is scene setup, retained with
duration/FPS and track fades in the expression's `KasaneSourceScene` authoring
metadata. Parameter entries import as Add offsets (scene initial value minus
model default), including zero offsets. Runtime exp3 output omits this metadata
and emits these Add offsets.
Engineering re-export uses current expression entries while retaining scene
setup. Multiply/Overwrite expressions have unavailable CAN3 blend semantics and are
explicit export losses. Pose and other runtime sidecars still require the
existing runtime package exporter. Paired export carries motion clips in CAN3;
standalone CMO3 export continues to report their omission.

Native CMO3 imports populate CDI parameter, group and Part display entries.
Runtime package export can therefore preserve Part names in `cdi3.json` even
when the imported display metadata is marked as originating from an Editor
project. Mesh/deformer display labels have no equivalent standard CDI fields.

## Future CLI boundary

Use `Document` as the authoring interchange representation, and keep resources
behind a resolver/session:

| Format | Existing input/output boundary |
| --- | --- |
| Kasane | `kasane_project::{decode_project, encode_project, DocumentSession}` |
| MOC3 | `kasane_moc3::{decode_moc3, encode_moc3, import_from_model3_file}` |
| PSD | `kasane_psd` import; `DocumentSession::import_psd_in_memory_authoring`; `kasane-moc3-psd` for reconstruction |
| CMO3 | `kasane_cmo3::{inspect_export, encode_cmo3}` plus the project adapters above |
| CAN3 | `kasane_cmo3::{inspect_can3, encode_can3}`; linked pair via `inspect_cubism` / `encode_cubism` |

A CLI can compose these APIs without depending on editor or Python code.

## Native editable project import

`import_model` reads CAFF resources and the CMO3 XML object graph into a typed
Kasane document plus in-memory PNGs. `animation_model_paths` reads CAN3 model
references, and `import_animation` imports its scenes against the model's stable
parameter and Part identities. No Java or Cubism installation is required by
these readers. Project/SDK adapters resolve relocated sibling files and retain
artwork in memory; `AuthoringSession::import_cubism` accepts either entry file.

The local Mao Editor-authored CMO3 5.2 and CAN3 5.4 files are tested, together with
Kasane's 5.3 output. Coverage includes ordinary grids, meshes, warp/rotation
hierarchies, morphs and constraints, Glue, parameter folders, effect groups,
physics, scene curves and fades. Unknown necessary structures, missing model
associations, multiple-model scenes, model track offsets/repetition, Form
Animation, ArtPath, Offscreen and non-default morph edit bases are rejected.
This is not a promise of all-version/all-feature Editor compatibility.

Source keys before zero or past the scene duration remain in native projects;
complete runtime packages split those curves at the playback boundaries without
altering the document. Editable exports retain the source keys and duration.

`DocumentSession::transfer_snapshot` captures fixed document/resources for
runtime or editable grouped publication. Unsaved artwork is held through shared
immutable pixels and encoded on the export worker, independently of the live
session. Model-only editable groups omit an empty CAN3. Transfer adapters always
use strict loss handling.

## Validation

```sh
cargo test --locked -p kasane-cmo3 -p kasane-project
cargo run --locked -p kasane-cmo3 --example fixture -- /tmp/model.cmo3
# Optional: local Cubism Editor archive/XML reader and model setup.
uv run --locked python tools/check_cmo3.py \
  --editor '/Applications/Live2D Cubism 5.4 alpha' /tmp/model.cmo3
```

The Rust tests independently unpack CAFF/ZIP, verify shared/cyclic references,
PNG embedding, keyform grids, morph constraints, masks, Glue UIDs, deterministic
output and loss handling. Animation tests check scene linkage, all four curve
types, handles, IDs, fades, events, empty scenes, frame collisions and overflow.
Project tests cover real MOC3-origin models, saved
project equivalence and publication rollback. The optional Java probe uses the
user's installed editor only for validation, checks its internal error list,
and constructs a model instance. For CAN3, it supplies the local editor's
resource relocation context, resolves the sibling CMO3, sets up every scene and
samples floating-point attribute curves. It also rejects errors logged by the
editor's reader, including errors omitted from its internal error list. The
CAN3 probe currently uses internal class names from Cubism 5.4 alpha; another
editor version may require adapting the optional probe, independent of the
Rust codec. It does not establish GUI rendering or editing
acceptance; those require opening the resulting file in Cubism Editor.

### Local Mao integration

With the local Nijiiro Mao package installed as described in
[`models/README.md`](../../models/README.md):

```sh
cargo test --locked -p kasane-project --test mao_cubism -- --nocapture
uv run --locked python tools/check_cmo3.py \
  --editor '/Applications/Live2D Cubism 5.4 alpha' \
  target/mao-cubism-integration/editable/mao.cmo3 \
  target/mao-cubism-integration/editable/mao.can3
# Compare Cubism's evaluated geometry and draw order against Kasane's poses:
uv run --locked python tools/check_cmo3.py \
  --editor '/Applications/Live2D Cubism 5.4 alpha' \
  --expected-geometry target/mao-cubism-integration/expected-geometry.json \
  target/mao-cubism-integration/editable/mao.cmo3
```

The integration imports the full model3 package, checks 260 meshes, seven Glues,
seven motion clips and eight expressions, verifies strict loss rejection,
exports with an explicit `LossPolicy::Allow`, and checks byte-identical export
from the saved Kasane project. It writes the linked files, full loss report and
`summary.json` under `target/mao-cubism-integration/`; third-party assets are
never checked in. Reference poses include parameter extrema and representative
mesh/warp/rotation morph drivers. The optional geometry probe compares every
vertex in canvas pixels with a default tolerance of 0.05 px, and compares the
complete back-to-front mesh sequence at every pose. This catches canvas
Y-reversal and parts-panel tie-order errors that a successful archive/XML read
alone cannot detect. If the
local sample is absent the optional Rust test skips.
