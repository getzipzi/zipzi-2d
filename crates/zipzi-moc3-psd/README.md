# MOC3 to PSD

Exports the model's default parameter pose as an 8-bit RGB PSD with one raster layer per ArtMesh. ArtMeshes invisible in the default pose are exported as hidden PSD layers with usable pixels. Textures are required because `.moc3` files contain UV geometry, not image pixels. The PSD writer uses `ag-psd`: layer channels are ZIP compressed and the flattened preview is RLE compressed. A layered PSD still contains more pixels than the packed source texture, so its size need not match the texture PNG.

```sh
cargo run -p zipzi-moc3-psd -- model.model3.json model.psd
cargo run -p zipzi-moc3-psd -- model.moc3 model.psd --texture 0=texture_00.png --texture 1=texture_01.png
```

The crate also exposes `from_model3_file`, `from_moc3_file`, `write_model3_psd`, and `write_moc3_psd` for programmatic use. Both conversion methods reject missing or corrupt textures.

`from_document` exports frozen authoring content using a captured-texture resolver and the document’s export bounds. The Editor exposes this through **File → Export → Layered PSD…**; it exports the default parameter pose independently of viewport previews.

Texture sampling uses GPU texel-center coordinates and bilinear filtering of premultiplied colors, preserving smooth texture and mask edges without bleeding invisible RGB. At native resolution, untransformed visible layer pixels retain their original values.

Layer names use mesh display names verbatim (including Unicode and duplicates), falling back to runtime IDs and then stable IDs when unnamed. Parts become nested pass-through PSD folders, including empty Parts. Interleaved Parts are split into same-named folders with a notice so the evaluated drawing order remains unchanged.

The PSD contains a flattened preview and editable ArtMesh layers, in evaluated draw order. ArtMesh clipping is exported as editable grayscale pixel masks, including combined and inverted masks. Original layer pixels remain intact; the flattened preview includes the masks. Mask sources use raw texture alpha independently of their visibility, opacity, colors and own clipping, matching the renderer. These masks capture the default pose and do not retain dynamic links between layers. The model's rig, parameter keyforms and offscreen effects cannot be represented as PSD layers. Offscreen effects produce warnings. The output is a raster snapshot of the default pose, not an editable Live2D model.
