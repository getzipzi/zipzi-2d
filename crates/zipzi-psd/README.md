# zipzi-psd

Imports a layered 8-bit RGB PSD into an in-memory Zipzi `Document` and PNG
assets. The importer does not write files or replace an active session.

```rust
let bundle = zipzi_psd::import_psd(&std::fs::read("art.psd")?)?;
// Write bundle.assets[i].bytes to bundle.assets[i].source relative to the project root.
let document = bundle.document;
```

Raster layers become cropped PNG assets and rectangular meshes. The canvas,
layer names, positions, stacking order, visibility, opacity, supported blend
modes and group hierarchy are retained. Clipping layers use isolated offscreen
groups; exporting them requires MOC3 5.3. The importer does not infer a rig,
parameters or deformers.

Normal, multiply and linear dodge blend modes are supported. Bitmap masks on
raster layers are baked into texture alpha, with a warning that the mask is no
longer independently editable. Grayscale coverage, mask offsets (including
layer-relative coordinates), the default color outside the mask, density and
disabled masks are supported. Layer opacity remains separate from the baked alpha.
Nonzero mask feather, obsolete mask inversion flags, group masks, vector/combined
masks, unsupported blend modes, adjustments, effects, group opacity and
unsupported clipping configurations return `UNSUPPORTED_LAYER`. Text, vector and placed
layers with raster pixels import as images with warnings; empty layers are
rejected. PSD/PSB features outside 8-bit RGB PSD version 1 are unsupported.
Input is limited to 512 MiB; decoded pixels above 512 MiB produce a warning.

`DocumentSession::import_psd_authoring` publishes a new project and its assets;
SDK and Python expose `import_psd(source, destination)`. The destination must be
new, and a failed import leaves the active project untouched. For unsaved Rust
editing, use `AuthoringSession::import_psd_in_memory` and `read_asset`; image
assets stay in memory until the project is saved.

```sh
cargo test --locked -p zipzi-psd
```
