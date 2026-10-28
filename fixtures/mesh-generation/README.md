# Mesh-generation fixtures

- `psd2live.json`: actual PSD2Live outputs for four synthetic masks and all 15
  edge/fill combinations. GPL-3.0-only. Input definitions and reproducible export:
  `tools/mesh-reference/Reference.java` and `tools/mesh-reference/generate.py`.
- `opaque32.png`: a generated opaque 32×32 RGBA rectangle for testing transparent
  texture padding at image edges. No third-party artwork.

Coordinates in the JSON are top-down source pixels. Tests convert the Rust
bottom-up geometry before comparing raster coverage. Vertex ordering and exact
triangles are deliberately not required to match the Kotlin triangulator.
