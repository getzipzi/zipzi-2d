# Local distribution artifacts

The Editor source distribution is a **workspace archive**. Its Rust macros embed
`assets/editor/icons` and `assets/editor/locales` from the root; GPUI patches also
live outside the Editor crate. `cargo package --list -p kasane-editor` confirms
that these files are absent from a standalone crate package. The three Editor
crates therefore declare `publish = false`; use the complete source layout below.
This does not change Cargo package identities, APIs or licenses.

```sh
python3.14 tools/package_source.py target/releases/zipzi-2d-source.tar.gz
python3.14 tools/verify_artifacts.py --source target/releases/zipzi-2d-source.tar.gz
uv build --wheel --python 3.14 --out-dir target/python-wheels crates/kasane-python
python3.14 tools/verify_artifacts.py --wheel /absolute/path/to/kasane.whl
./tools/build-editor-app.sh editor debug
./tools/build-editor-app.sh editor release
python3.14 tools/verify_artifacts.py --app 'target/Kasane Editor.app'
```

The source tool includes tracked and non-ignored new source files plus initialized
Purism sources, records hashes and checkout status, and refuses missing gitlinks,
escaping symlinks and tracked private/generated content. Initialize Purism before
packaging. Local models, target, Python caches and machine Cargo config are excluded.
It retains all 18 crate paths, root assets, patches, docs and both license texts.
Extract the `zipzi-2d/` directory and build from that workspace root.

The wheel carries MIT and third-party notices under `.dist-info/licenses`, the
native extension, `py.typed` and `_native.pyi`. Distribution copies of the two
notice files live in `crates/kasane-python/`; `verify_artifacts.py` compares them
byte-for-byte to the canonical root files before CI builds a wheel. Update both
copies when the canonical text changes. An isolated installed-wheel gate is:

```sh
uv run --locked python tools/validate_sdk.py --wheel /absolute/path/to/kasane.whl
```

The CPU wheel report is `partial` when CPU checks pass but GPU/dual-Core checks
were not requested. Read the individual checks; do not relabel it a full pass.

Both app profiles place the binary reported by Cargo in `Contents/MacOS`, the
application icon and MIT/GPL/licensing/notices in `Contents/Resources`, and retain
vendored license files under `vendor-notices/`. SVG icons and locales are embedded
in the binary rather than copied to a second runtime asset tree. The verifier
checks the actual embedded bytes and language text. The output path is reused
between profiles; retain a separate copy under `target/` when comparing both.

These commands create local unsigned artifacts. They do not upload releases,
notarize apps, change remote repositories or verify GitHub license presentation.
