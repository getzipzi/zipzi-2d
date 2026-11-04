# zipzi-project-preview

Read-only static previews for `.zipzi` container v1. This crate does not depend on
the document model, SDK, GPU or Editor. `read_preview` only reads the ZIP index,
`format.json` and `preview.png`; it never extracts files or reads project JSON or
external resources. Preview presence does not certify the authoring document.

The advisory `preview` descriptor declares SHA-256, width and height. The image
is static RGBA8 PNG, at most 1024 × 1024 and 8 MiB encoded. The reader limits
archive length (16 GiB), directory size (8 MiB), entries (16,384), cumulative reads
(32 MiB), format JSON (64 KiB) and PNG workspace (16 MiB). Projects exceeding the
Shell limits can still open in the Editor. Absent or rejected previews let the
system fall back to its normal file icon / unavailable preview.

`include/zipzi_preview.h` exposes a callback-based C ABI for native, stream-based
Shell extensions. No allocations or pointers cross ownership boundaries. The
caller supplies a bounded output buffer and random-access reads; `0` means no
image, negative means rejected input. A PNG result can be decoded by the OS.

```sh
cargo test --locked -p zipzi-project-preview
```

Native frontends live in `native/project-preview/`. Build them through
`tools/packaging/build_project_preview.py`; see that directory's README for
platform validation and registration.
