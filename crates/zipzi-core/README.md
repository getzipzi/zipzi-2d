# zipzi-core

Core data model for Zipzi 2D, including documents, geometry, deformers, parameter bindings, and scene evaluation.

`image::decode_png_bounded` checks encoded bytes, expected header dimensions and
conservative decoder/conversion workspace before allocating output. Its decoder
bounds chunk storage, skips text/ICC decompression and validates checksums through
the image end. `png_decode_workspace_bytes` is the shared preparation upper bound;
`rgba_png_capacity` is the shared observation output capacity. Focused tests:
`cargo test --locked -p zipzi-core --lib image::`.
