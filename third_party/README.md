# Third-party dependencies

This directory is the repository-wide home for local third-party source and
binary dependencies. Proprietary Cubism SDK contents are ignored by Git and
must not be committed.

Install the Native SDK at:

```text
third_party/CubismSdkForNative-5-r.5/
```

`benchmarks/cubism-matrix` uses this shared copy.
Set `CUBISM_SDK_ROOT` to use a different SDK location. A locally modified Cubism
Framework can be placed at `third_party/CubismNativeFramework/` or selected with
`CUBISM_FRAMEWORK_ROOT`.

The current local checkout has the Native SDK copied from the original SDK
checkout, with matching hashes; this is a machine-local dependency, not part
of the tracked repository or workspace source archive. The local
`.cargo/config.toml` was copied from the original Editor checkout and is also
ignored. A new clone needs its own authorized SDK installation and optional
machine Cargo configuration.

The Live2D models used by the repository are inventoried in
[`models/README.md`](../models/README.md). Keep model files outside the SDK
under `models/local/`; SDK sample models remain in their vendor package.
