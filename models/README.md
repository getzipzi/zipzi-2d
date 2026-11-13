# Model Assets

This directory documents the full models used for development, tests, and benchmarks. Place locally obtained models in `models/local/`; that directory is ignored by Git and excluded from source releases. Follow each model's license and do not commit restricted assets. Small shared test fixtures are maintained separately in [fixtures](../fixtures/README.md).

## Local models

Keep each model's directory structure intact, including the textures, motions, and other files referenced by its model3 file. Paths below are relative to the repository root.

| Model | Entry point | Used for |
| --- | --- | --- |
| Nijiiro Mao (`mao_pro`) | `models/local/mao/runtime/mao_pro.model3.json` | Editor import, rendering, and animation tests; SDK and rendering examples; and Cubism comparison benchmarks. The accompanying `models/local/mao/mao_pro_t06.cmo3` and `mao_pro_t06.can3` are used in Cubism authoring tests. |
| Shirousagi | `models/local/Shirousagi/Shirousagi.model3.json` | Editor import, rendering, and multi-model scenarios. The accompanying `Shirousagi.psd` can be used for PSD import. |
| DeepSeek (大肥鱼) | `models/local/DeepSeek/大肥鱼.model3.json` | Full-model round-trip tests and project I/O and performance measurements. |

These assets are not needed for every development task. Some tests skip when an asset is missing; other tests and benchmarks require it explicitly. Check the instructions for the test or benchmark before running it.

## Cubism SDK samples

Some MOC3 tests read samples directly from `third_party/CubismSdkForNative-5-r.5/Samples/Resources/`. Keep them in the SDK's original directory structure.

| Sample | Directory under `Samples/Resources/` | Used for |
| --- | --- | --- |
| Hiyori | `Hiyori/` | MOC3 import tests |
| Rice and Mark | `Rice/`, `Mark/` | Older-version MOC3 import tests |
| Ren | `Ren/` | MOC3 import and export tests |

The SDK's Mao sample is a different model from Nijiiro Mao listed above. Obtain the SDK and local model assets separately.

## Running tests and benchmarks

- The [Cubism comparison benchmark](../benchmarks/cubism-matrix/README.md) describes its SDK, model, and build prerequisites. Rendering cases copy Mao into the benchmark's ignored `assets/live2d/mao/` directory; the source remains in `models/local/mao/`.
- The [benchmark tools guide](../tools/benchmarks/README.md) provides commands for Editor, rendering, and project I/O measurements.
- The [Runtime README](../crates/zipzi-runtime/README.md) describes the optional DeepSeek round-trip test.
