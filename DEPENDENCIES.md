# Dependency revisions

- GPUI Kit 0.6.5: official `https://github.com/longbridge/gpui-kit`, pinned to `5059de47a6560fdbe24e32cd91b45d6aa6849695`.
- GPUI 0.3.6: three crates patched through root `[patch.crates-io]`: `gpui-pre`, `gpui-pre-macos`, `gpui-pre-apple`. Snapshot: Zed `bcf6582ce3500df93a8a39366640173e6786cea6`.
- Kasane: local `../kasane-2d`, at revision `2d71ba66075abad19de5e15a6552ed7e0ac6b1ea` or a compatible descendant. This includes `kasane-render-metal` and its backend-neutral changes, plus persistent `HierarchyOrder` metadata and `EditSession::replace_hierarchy_order` for palette sorting. Layer locking includes `ObjectLocks`, `EditSession::replace_object_locks`, metadata-preserving preview history and project format v7 support.
- Rendering: native Metal via `metal 0.33`, with GPUI's device and command queue shared by Kasane.
- Authoring: `editor-core` depends on the local `kasane-sdk` for document transactions, history and project IO, and `kasane-project` for decoding validated assets. It has no GPU dependency. `editor-render` owns the direct `kasane-render` / `kasane-render-metal` dependencies.
- Tree visibility also requires `Offscreen.enabled` and its evaluation/persistence support in the local Kasane checkout. Hidden Offscreens require project format v8 or later; projects without hidden Offscreens or hidden Deformer controls continue to save as v7.
- Deformer control visibility requires `DeformerDisplay` metadata and `EditSession::replace_deformer_display`. Projects with hidden Deformer controls save as v9, without changing runtime transform enablement or geometry.

- Animation additionally requires `MotionPreview::advance_to_with_progress` in
  the local `kasane-animation` crate, reusing the canonical absolute 60 Hz seek
  grid for continuous playback. Cancellation preserves completed canonical
  steps; backward replay uses transactional seek. High-refresh playback also
  requires retention of the full canonical step before each fractional sample:
  presentation tails must not accumulate into physics or replay from an old
  checkpoint every display frame. The SDK still owns Motion / Expression /
  Physics / Pose evaluation.
  Fractional-state retention is implemented in `2e2912a2d098bebec0f3d66df5e05489427bd3e2`.
- The local SDK must reexport `apply_model_opacity`, shared by the animation
  renderer and `kasane-sdk-observe`. This host policy multiplies each drawable
  once before masking / Offscreen; Offscreen and Part opacities remain separate.
  It matches the observer's existing host policy and the Framework renderer
  color-alpha reference documented in the sibling
  `docs/archive/EDITOR-ANIMATION-IMPLEMENTATION.md`. Framework Model opacity
  alone does not automatically set renderer alpha.
- The editor's Metal tests use the sibling `kasane-sdk-observe` as a development
  dependency. An overlapping, masked Offscreen fixture at opacity 0 / 0.5 / 1
  compares actual pixels (maximum error 1/255) and distinguishes this policy
  from final flattened-image alpha. These SDK / observer changes must accompany
  this editor checkout; they are committed in the adjacent repository at
  `c90bdc5f2f00b540c87635745d0f3a4f66dd97bb`.
- Animation acceptance reads the existing SDK `SeekCacheStats` through
  `MotionPreview::seek_cache_stats` to check the retained checkpoint budget.
  The native benchmark uses the existing GPUI `frame_trace` hooks and Metal
  command timestamps to distinguish CPU evaluation, GPU completion, texture
  adoption and compositor presentation. Stage five adds no sibling SDK or
  renderer changes beyond the revision above. Its real-model opacity test adds
  an Offscreen to a temporary imported Mao document; licensed source assets are
  unchanged. See [validation results](docs/archive/workspaces-animation-validation.md).

All workspace crates use the root `Cargo.lock`. Build and test with `--locked`.
The editor supports macOS. There are no application backend switches or CPU
readback fallback. No sibling GPUI checkout is required.

GPUI Kit has dependencies for other target platforms, which Cargo may retain in
the cross-platform lockfile. This does not make them active macOS dependencies.
See `vendor/README.md` for the exact local patch scope.
