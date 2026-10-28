# zipzi-cmo3

Rust import and export for editable Cubism CMO3 models and CAN3 animations.
Export targets the Cubism Editor 5.3 schema and does not require Java, Cubism
Editor or the native Cubism SDK. PSD2Live/Umamo was consulted for CAFF and XML
wire-format conventions; it is not a dependency.

## API

```rust,no_run
use zipzi_cmo3::{encode_cmo3, AssetResolver, Cmo3Artifact, Cmo3Error, Cmo3Options};
use zipzi_core::Document;

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
use zipzi_cmo3::{encode_cubism, CubismArtifact, CubismOptions, AssetResolver, Cmo3Error};
use zipzi_core::Document;

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
`inspect_can3` and `inspect_cubism` expose preflight diagnostics.

`inspect_export` reports conversion losses without resource I/O. The default
`LossPolicy::Reject` returns `Cmo3Error::Unsupported(report)` before reading
images. Explicit `LossPolicy::Allow` produces a file with those same diagnostics;
callers should surface the report instead of silently discarding it.

## Project and SDK adapters

`zipzi-project` provides `encode_project_cmo3`, `encode_document_cmo3` and
`DocumentSession::encode_cmo3`. Paired exports use `encode_project_cubism`,
`encode_document_cubism` or `DocumentSession::encode_cubism`. The corresponding
`publish_*` and `export_*` methods write artifacts. `zipzi-sdk::AuthoringSession`
also exposes in-memory encoding.

Encoding and publication are separate so callers can review diagnostics before
writing. Paired publication stages the output, protects unrelated files and
rolls back reported replacement failures; it is not a crash-atomic transaction.
The destination parent must already exist. A post-rename directory sync failure
is reported as a publication warning.

```sh
cargo run --locked -p zipzi-project --example project_cmo3 -- \
  /path/to/project /path/to/model.cmo3
cargo run --locked -p zipzi-project --example project_cubism -- \
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

Texture assets do not contain original PSD layers, painting history or undo
history. Unsupported features and conversion losses are reported. Examples
include some blend curves, fractional draw orders, Y physics, custom forces,
opaque extensions and runtime package attachments. Invalid resources or
documents and meshes over 32,767 vertices are rejected.

## Preserved animation data

Every Zipzi motion clip becomes a named, editable CAN3 scene in document
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

CAN3 uses integer frames for anchors, duration and events. Rounding and
incompatible features produce diagnostics. `auto_frame_rate` searches for a
suitable grid; `bake_animation` can adapt or sample incompatible tracks when
explicitly enabled. Inverse-step boundaries and subframe interpolation can
differ. Expressions are represented as scenes and standard exp3 companions;
see [expression semantics](EXPRESSION-SEMANTICS.md) for preservation limits.
Pose and other runtime sidecars use the runtime package exporter.

## Native editable project import

`import_model` reads CMO3 into a typed document and in-memory PNGs.
`animation_model_paths` and `import_animation` resolve and import CAN3 scenes.
Project and SDK adapters resolve sibling files; `AuthoringSession::import_cubism`
accepts either entry file. Session imports extract independent mesh images from
atlas pages; see [project import artwork](../zipzi-project/README.md#import-artwork).

Unknown required structures and unsupported features are rejected, including
multiple-model scenes, Form Animation, ArtPath and Offscreen. Compatibility is
not guaranteed for every Cubism version or feature. Source keys outside scene
duration remain in editable exports; runtime package export splits curves at
playback boundaries.

## Validation

```sh
cargo test --locked -p zipzi-cmo3 -p zipzi-project
cargo run --locked -p zipzi-cmo3 --example fixture -- /tmp/model.cmo3
# Optional: validate with a local Cubism Editor installation.
uv run --locked python tools/validation/check_cmo3.py \
  --editor '/Applications/Live2D Cubism 5.4 alpha' /tmp/model.cmo3
```

The optional probe uses Cubism internals and may need adaptation for other
versions. It does not test GUI rendering or editing. Local model integration
requires separately obtained assets; see [models](../../models/README.md).
