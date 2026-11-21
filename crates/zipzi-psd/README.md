# zipzi-psd

Imports a layered 8-bit RGB PSD into an in-memory Zipzi `DocumentState` and PNG
assets. The importer does not write files or replace an active session.

```rust
let bundle = zipzi_psd::import_psd(&std::fs::read("art.psd")?)?;
// Capture bundle.assets PNG bytes into a ResourceStore before publishing.
let document = bundle.document;
```

Raster layers become cropped PNG assets and rectangular meshes. The canvas,
layer names, positions, stacking order, visibility, opacity, supported blend
modes and group hierarchy are retained. Clipping layers use isolated offscreen
groups; exporting them requires MOC3 5.3. The importer does not infer a rig,
parameters or deformers.

Normal, multiply and linear dodge blend modes are supported. By default, bitmap
masks on raster layers are baked into texture alpha, with a warning that the mask is no
longer independently editable. Grayscale coverage, mask offsets (including
layer-relative coordinates), the default color outside the mask, density and
disabled masks are supported. Layer opacity remains separate from the baked alpha.
Nonzero mask feather, obsolete mask inversion flags, group masks, vector/combined
masks, unsupported blend modes, adjustments, effects, group opacity and
unsupported clipping configurations return `UNSUPPORTED_LAYER`. Text, vector and placed
layers with raster pixels import as images with warnings; empty layers are
rejected. PSD/PSB features outside 8-bit RGB PSD version 1 are unsupported.
Input is limited to 512 MiB; decoded pixels above 512 MiB produce a warning.

`zipzi_project::ProjectImporter::import_psd` captures the PNGs into owned
ResourceView values and returns an ImportedProject plus the import report.
The caller can create an SDK Snapshot from its document/resources and publish it
through AuthoringSession. Import failure performs no active-session mutation.

```sh
cargo test --locked -p zipzi-psd
```

For editable user masks, use `import_psd_with_masks(bytes, true)` or Python
`session.import_project(path, v.ImportFormat.PSD_EDITABLE_MASKS)`. Each mask
becomes a zero-opacity native mesh named after the layer with a `(Bitmap Mask)`
suffix. Its texture stores grayscale in RGB and clipping coverage in alpha.
The artwork retains its original RGBA. Read or replace either image using its
asset ID; edit the mask mesh positions to move the mask. Disable clipping by
clearing the artwork's `masks`, and restore the source mesh ID to re-enable it.
An initially disabled PSD mask retains its source mesh without an active link.
White outside coverage uses an inverted mask. Fractional outside coverage has
a constant border extending over the imported canvas and layer. These native
meshes can be saved and reopened or rigged independently.
