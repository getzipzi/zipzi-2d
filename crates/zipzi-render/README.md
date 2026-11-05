# zipzi-render

Backend-independent rendering support for frame validation, scene organization, and render planning.

`texture::area_average_rgba8` directly downsamples straight RGBA8 with alpha-weighted
RGB and area-averaged alpha. The controlled variant checks cancellation before
allocation and during source scanlines. Unchanged dimensions preserve all source
bytes. `straight_rgba_mipmaps` uses the same filter recursively; thumbnails must
filter directly from the source rather than from an intermediate mip level.
Filter and mipmap versions are explicit constants.

Mipmap halving uses exact integer 2×2 filtering for even dimensions and reuses
fractional axis footprints for odd dimensions. Both paths preserve the general
filter's bytes, including alpha-weighted RGB and rounding at very low alpha.
`premultiply_rgba8_in_place` converts owned upload buffers without another copy;
authored pixels and mip generation inputs must remain straight RGBA.

Focused CPU checks:

```sh
cargo test --locked -p zipzi-render --lib texture::
```
