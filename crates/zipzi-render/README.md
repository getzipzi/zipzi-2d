# zipzi-render

Backend-independent rendering support for frame validation, scene organization, and render planning.

`texture::area_average_rgba8` directly downsamples straight RGBA8 with alpha-weighted
RGB and area-averaged alpha. The controlled variant checks cancellation before
allocation and during source scanlines. Unchanged dimensions preserve all source
bytes. `straight_rgba_mipmaps` uses the same filter recursively; thumbnails must
filter directly from the source rather than from an intermediate mip level.
Filter and mipmap versions are explicit constants. Focused CPU checks:

```sh
cargo test --locked -p zipzi-render --lib texture::
```
