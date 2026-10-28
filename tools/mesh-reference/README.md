# PSD2Live mesh reference

`Reference.java` invokes the real Kotlin generator on four synthetic masks and
exports all three edge modes × five fill algorithms. No proprietary assets are
required. `generate.py` only needs Python's standard library, JDK 21 and a local
PSD2Live checkout with its Gradle dependencies cached:

```sh
python3 tools/mesh-reference/generate.py ~/web/psd2live
```

The fixture records the checkout revision. Start from a clean source checkout;
compilation updates its build outputs but does not edit sources. Kotlin packages
marked `internal` are callable from this Java harness. Output coordinates follow
the original generator (top-down); the Rust regression explicitly converts them.
The measured `elapsed_ns` values are diagnostic single calls with JVM warm-up
variation; they are not a controlled cross-language performance benchmark.

The checked-in fixture is from revision
`692986728e2b7b9254a41a22bdce1656bc0c12f9`. The original generator and derived
fixture retain GPL-3.0-only; see `THIRD_PARTY_NOTICES.md`.

Generate the Rust report and an offline, filterable comparison gallery:

```sh
env -u ZIPZI_DEV_SESSION_DIR ZIPZI_MESH_REPORT=/tmp/zipzi-mesh-report.json \
  cargo test --locked -p zipzi-editor-core --lib adaptive_mesh::tests::psd2live_reference
python3 tools/mesh-reference/compare.py /tmp/zipzi-mesh-report.json
```

Open `target/mesh-port/comparison.html`. Each pair uses the same coordinate scale;
all 60 cases are included. This is a visual inspection aid, not a screenshot
baseline or a substitute for the geometry assertions.
