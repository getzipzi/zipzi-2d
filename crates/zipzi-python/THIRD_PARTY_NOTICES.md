# Third-party notices

The repository-level MIT License applies only where a file, directory,
dependency, or submodule does not carry a different notice. The following
upstream works retain their original copyrights and license terms.


## PurismCore

`third_party/purism-core` is a forked Git submodule of
[PurismCore](https://github.com/SakuraMotion/PurismCore).

Copyright (c) 2026 Sakura Motion Project.

PurismCore is distributed under the MIT License included in that submodule.

## Imported Editor and patched dependencies

Editor-owned source and assets retain GPL-3.0-only; see [LICENSING](LICENSING.md)
and the full [GPL text](LICENSE-GPL-3.0). Patched GPUI sources now live in `vendor/`;
their original notices and [dependency/patch record](vendor/README.md#dependencies)
remain authoritative. Directory flattening does not relicense third-party code.

## PSD2Live adaptive mesh generator

`crates/zipzi-editor-core/src/adaptive_mesh/` is a Rust adaptation of the automatic
mesh-generation code in [PSD2Live](https://github.com/tsunehimatoi/psd2live),
revision `692986728e2b7b9254a41a22bdce1656bc0c12f9`, by the PSD2Live contributors.
It retains **GPL-3.0-only**, including its derived synthetic reference outputs in
`fixtures/mesh-generation/psd2live.json`. The original and adapted file mapping and
numerical substitutions are documented in that module's README. These sources
are part of the GPL Editor, not the MIT SDK. The full license is included in
`LICENSE-GPL-3.0`.
