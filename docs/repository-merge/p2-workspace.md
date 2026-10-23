# P2: workspace integration

## Decisions

The root owns the single main Cargo workspace and lockfile, with resolver 3,
15 original SDK/benchmark members and 3 Editor members. `core`, `project`, and
`sdk` are the default members. Explicit `--workspace` still selects all 18;
existing broad SDK scripts and workflows must be adapted in P4 before use.

SDK members retain edition 2021 and MIT. Editor members explicitly declare
edition 2024 and GPL-3.0-only, and inherit Rust 1.95. The root's MIT default does
not relicense Editor. The imported Editor workspace manifest and lockfile are
removed; vendor manifests and the GPUI Kit nested workspace remain intact.
All Editor Cargo path dependencies, including test dependencies, now stay inside
this checkout. Runtime resource and script paths are deferred to P3.

The GPUI crates.io and Git patches are rooted at `apps/editor/vendor/`; those
packages are excluded from main workspace membership. GPUI Kit/FPS retain Git
revision `507e82b8115ac73af92b7d3856501f9d7b73b2c9`.

## Lockfile and features

The SDK lockfile was retained as the primary seed. Editor-only package records
were added from the Editor lockfile; compatible overlapping versions were
normalized to SDK versions before Cargo resolved the combined graph offline.
This avoids an unconstrained update to today's registry versions.

The final lock contains 877 package identities. All 218 SDK baseline identities
are retained, with 659 additions from the Editor baseline. No package identity
is new relative to the union of the two original locks. Existing registry
checksums and Git sources are unchanged.

Fourteen Editor versions converge to the SDK baseline:

| Package | Editor baseline | Unified |
| --- | --- | --- |
| cc | 1.5.1 | 1.4.7 |
| find-msvc-tools | 0.1.14 | 0.1.13 |
| smallvec | 1.16.2 | 1.16.1 |
| thiserror, thiserror-impl | 2.0.21 | 2.0.20 |
| zerocopy, zerocopy-derive | 0.8.59 | 0.8.57 |
| js-sys, web-sys | 0.3.106 | 0.3.105 |
| wasm-bindgen, wasm-bindgen-macro, wasm-bindgen-macro-support, wasm-bindgen-shared | 0.2.129 | 0.2.128 |
| wasm-bindgen-futures | 0.4.79 | 0.4.78 |

These shared compatible versions satisfy the combined manifests; the paired
wasm-bindgen family stays synchronized. Incompatible major/minor series needed
by either side remain separate, as before. No dependency requirements were
weakened to force resolution.

SDK `serde/rc` and `serde_json/float_roundtrip` are retained. The
[dependency audit](p2-dependency-audit.json) records whole-workspace metadata
feature differences: 40 shared identities differ from SDK-only metadata and
2 differ from Editor-only metadata. These reflect the combined graph, including
platform-specific dependencies; they are not a claim that every feature builds
for every selected package or target. SDK graph traversal confirms no dependency
on Editor or GPUI, even with Editor debug/test features enabled in metadata.

## Profiles

The Editor development profile is preserved: line-table debug information,
incremental compilation, and optimization level 3 for external dependencies.
Existing named SDK overrides remain at level 0. All workspace members not named
in those overrides also receive Cargo's default level 0 (the `"*"` override
applies to non-workspace packages). In particular `kasane-moc3-psd`, previously
an Editor dependency without an explicit override, now receives level 0 as a
workspace member. Editor crates remain level 0. Compared with the old SDK root,
external development dependencies now use the Editor optimization policy and
SDK debug info changes to line tables. This can change cold build costs; no
wall-clock performance equivalence is asserted.

The Python release `strip = "none"` workaround remains. Release optimization
settings are otherwise unchanged. No machine `.cargo/config.toml` was copied.

## Minimum Rust version

Rust 1.94.0 fails with E0658 at both existing `gpui-fast/src/profiler.rs`
(vendored) and `gpui-pre-0.3.9/src/profiler.rs` (registry): they use
`std::hint::cold_path`. The [Rust standard library documentation](https://doc.rust-lang.org/std/hint/fn.cold_path.html)
marks this API stable since 1.95.0. This is a pre-existing Editor dependency
requirement, uncovered by the migration; no GPUI source or version was changed.
The unified workspace MSRV and existing CI toolchain are consequently 1.95,
not the local latest compiler. The existing SDK CI package scope still needs P4.

## Validation

Local raw logs
are under `.git/merge-baseline/p2-*.log`; they are not source artifacts.

Structural acceptance is reproducible using Python 3.11+:

```sh
python3.14 docs/repository-merge/verify-workspace.py
python3 docs/repository-merge/verify-import.py
```

`verify-workspace.py` uses `cargo metadata --locked --offline` from both roots
with Editor debug/test features. It checks identical workspace/target resolution,
all 18 member identities and licenses/editions/MSRV, defaults, local source
containment, actual patch provenance, vendor exclusion, Git revision, dependency
direction and profile/feature declarations. Offline success uses this machine's
existing Cargo source cache and does not demonstrate a fresh offline clone.

P3 resource discovery, P4 broad gate selection, P5 artifact licensing and P6
full/fresh-clone acceptance remain pending. No native runtime, wheel packaging,
licensed model, benchmark timing, or cross-platform result is inferred here.

Completed checks (macOS arm64):

| Command / check | Result |
| --- | --- |
| `verify-workspace.py` | Passed from root and Editor directories, including debug/test feature resolution |
| `verify-import.py` | Passed; original P1 import remains exact |
| `cargo check --locked -p kasane-sdk -p kasane-editor --features kasane-editor/test-support` | Passed on Rust 1.99.0 |
| `cargo check --locked -p kasane-sdk` | Passed independently on Rust 1.99.0; log contains no GPUI or Editor compilation |
| `env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --lib document::properties::tests:: -- --nocapture` | 4 passed, 0 failed, 29 filtered out on Rust 1.99.0 |
| `cargo +1.95.0 check --locked -p kasane-sdk -p kasane-editor --features kasane-editor/test-support --target-dir target/p2-msrv` | Passed on the declared minimum Rust 1.95.0 |
| `env RUSTUP_TOOLCHAIN=1.95.0 python3.14 docs/repository-merge/verify-workspace.py` | Passed on Rust 1.95.0 |
| Combined SDK/Editor check on Rust 1.94.0 | Failed on existing GPUI `cold_path`; reason for raising the unified MSRV |

The four property tests exercise physical-field index reuse, bound mesh opacity
preview, presentation commits and numeric candidates. They preserve existing
result and work-count assertions; this is not a timing benchmark. Rust 1.99
reported the existing `block 0.1.6` future-incompatibility warning. The Rust 1.95
check also emitted vendor warnings (including 741 from `gpui-fast-macos`);
success means compilation passed, not that the dependency build was warning-free.
No vendor warnings were suppressed or patched in this stage.

Rust 1.94 installation itself succeeded; rustup's subsequent optional self-update
failed while setting permissions on `rustup-init`. `rustc +1.94.0 --version` and
the compiler run worked. The 1.95 installation uses `--no-self-update`.

P2 acceptance is complete. Original source HEADs were rechecked and remain the
frozen P0 SHAs, with both original worktrees clean. No remote was pushed.
