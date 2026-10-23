# P4: package-scoped test gates and CI

Implemented after P3F `df9c3d79`. The authoritative package partition is
`tools/package-domains.json`: 15 SDK packages, including Python and the Rust
comparison runner, and 3 Editor packages. `cargo_domain.py --check` compares this
partition with Cargo metadata and rejects missing, stale, duplicate or overlapping
members. The runner generates explicit `-p` selectors for test/check/build/clippy/fmt,
rejects scope overrides and propagates Cargo exit status from the repository root.

SDK and Editor full/no-argument gates retain their original domain coverage,
including examples/doctests. SDK daily gates still skip only `alpha_mesh_stress::`;
full and stress keep the complete matrices. Python/wheel forwarding remains
unchanged. Editor retains test-support, all five acceptance targets and development
session isolation. Formatting selects only the corresponding owned packages.
See [test domains and CI](../development/testing.md) for usage and coverage.

## CI implementation

- SDK's Linux/macOS/Windows matrix is unchanged. Existing named steps were
  compared against P3F: every step remains; non-Cargo gates and their platform
  conditions are unchanged. Format, clippy and test now select the SDK domain.
- SDK/Python wheel, typing, isolated acceptance, validator negatives, decoder
  safety without native Core, experiment controls and macOS WGPU comparison remain.
- New macOS Editor CI checks partition/format/tooling, compiles all targets with
  test-support and runs core synthetic tests. A Swift probe checks actual Metal
  device availability before UI/integration and all five acceptance targets run.
- An absent Metal device produces an explicit warning and job-summary coverage
  gap; a broken probe fails the job. No successful native test result is fabricated.
  Optional licensed models remain absent from hosted CI and their coverage is
  explicitly reserved for local acceptance.
- Both workflows trigger on all pushes and PRs. No path filters omit SDK consumers,
  shared files, assets or vendor changes. Filtering can be optimized after hosted
  coverage has been observed.

## Local verification

- Package metadata coverage: 15 + 3, exhaustive and disjoint.
- Workspace structural verifier: editions/licenses, lock/patch settings and no
  SDK-to-Editor/GPUI dependency unchanged. Cargo.lock has no change.
- Gate controls: 4 SDK entrypoint tests, 8 Editor entrypoint tests and 5 shared
  domain tests passed. They cover actual subprocess argument construction,
  filters/harness arguments, complete default/full scopes, native target retention,
  session isolation, outside cwd and format/Rust/tooling/wheel failure propagation.
- SDK domain formatting and Editor domain formatting passed.
- `cargo_domain.py sdk test --locked --no-run`: complete SDK test target selection
  compiled successfully, without GPUI/Editor compilation.
- `test-editor.sh regression --no-run`: all three libraries and synthetic integration
  target compiled. `test-editor.sh acceptance --no-run`: all five acceptance
  target executables compiled.
- `test-sdk.sh regression physics_editing::`: 7 tests passed across the filtered
  63 test executables, preserving the daily stress exclusion.
- `test-editor.sh regression i18n::`: 2 locale/catalog tests passed.
- Local Swift probe reported `available=true`; the exact native test
  `overlay::overlay_matches_paths_markers_and_retains_old_frames` passed using the
  Editor domain runner, test-support and a cleared development session.
- Workflow YAML parsed; existing SDK matrix/steps compared; shell syntax, active
  documentation links and Git whitespace checks passed.

Compile/test logs are retained locally under `.git/merge-baseline/p4-*`.
No full expensive suite or wheel acceptance was rerun during this routing change.
Hosted CI has not run: the branch is local and was not pushed. In particular,
local Metal availability does not establish GitHub runner availability. Full
three-platform execution, full SDK/Editor regression, wheel/app artifacts,
licensed models and fresh-checkout isolation remain P5–P6 acceptance work.
