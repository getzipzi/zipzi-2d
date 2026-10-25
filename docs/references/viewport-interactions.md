# Viewport tools and gestures

Tool state belongs to the document's Modeling context. A viewport provides the
displayed frame, projection and input owner; multiple viewports share object and
component selection but keep navigation and presentation independent.

Each document/viewport fits its content once, when first presented with visible
metrics. Later parameter previews, geometry/visibility changes, animation frames
and resizes preserve its zoom and pan. The explicit Fit command fits the current
content again; an unmeasured or hidden viewport keeps its initial fit pending.

## Where behavior lives

[`zipzi-editor-core/src/tools/`](../../crates/zipzi-editor-core/src/tools/) groups interaction
behavior by operation:

- `select.rs`: shared hit/selection preparation, deferred object clicks,
  rectangle completion and its overlay and capture hints.
- `lasso.rs`: freehand sampling, release rules, overlay and idle/capture hints.
- `controls.rs`: control-point/gizmo candidate preparation, transform math,
  commit/cancel, feedback and intermediate-frame acceptance.
- `hand.rs` and `glue.rs`: tool guidance; Glue shares selection gestures, while
  Hand uses viewport navigation.
- `gesture.rs`: the explicit Select/Lasso/Controls variants, common press
  projection and dispatch. Tool selection-family and idle Escape policy are
  declared here alongside tool dispatch.
- `region.rs`: shared rectangle/polygon geometry and selection application.
- `input.rs`: pointer event dispatch and whether the model changed.
- `overlay.rs`: projection of shared scene controls; delegates gesture feedback
  to the active variant.

The press snapshot holds the original scene, transform, selection and modifiers.
Only a control gesture can own a candidate edit. An idle control press creates no
transaction; crossing the existing drag threshold prepares it. Selection gestures
never own model transactions.

## Lifecycle

[`viewport/interaction.rs`](../../crates/zipzi-editor-core/src/viewport/interaction.rs)
owns interaction acquisition and
release for both UI and synchronous core clients:

1. `begin_canvas_gesture(binding, point, shift, alt)` checks the displayed frame before taking
   ownership. A rejected stale press leaves a running drag intact.
2. The result supplies the current capture token. Captured UI events use
   `update_canvas_gesture(token, input)` to reject old serials and repeated presses.
3. Pointer Up completes the gesture and releases the token in the same call.
   Cancel, lost button and operation errors roll back and release it.
4. Releasing an unfinished Modeling capture with `cancel_viewport_interaction`
   also rolls it back. It cannot leave a candidate without an owner.
5. Visibility, resize, document/context changes and focus loss use the same core
   cancellation path. Delayed cleanup of an old token cannot affect its successor.
6. Core commands release the Modeling token when they end its gesture, including
   commands that fail after cancellation. Preflight rejection preserves a running
   gesture and its token; navigation captures remain independent.

`ViewportInputUpdate` reports only `model_changed` and the surviving capture.
Selection-only clicks do not evaluate the model or open an edit. `hover_canvas`
handles uncaptured move/leave and cannot disturb a gesture in another viewport.
`begin_viewport_navigation` acquires a navigation token; Modeling acquisition
always begins the gesture in the same call. Device-specific pan positions, wheel interpolation and OS
capture stay in `viewports/`; they do not own model transaction completion.

## Status bar hints

Tools and gestures declare static semantic `InteractionHint` slices next to their
operation. Timeline declarations and published state live in
`panels/timeline/hints.rs`. A Timeline-owned `TimelineHints` handle lets the dock
and status bar read a snapshot while the Timeline entity is borrowed. Capture
snapshots check the original core operation before producing hints; cancellation
and entity release clear them. `InputRouter` stores no Timeline hint state. These
declarations carry actions, modifiers and command identities, with no GPUI or
translation dependency in core.

StatusBar chooses active viewport capture, active Timeline capture, then idle tool
guidance. It reads viewport capture directly from core, resolves configured command
bindings, translates labels and formats keys. It has no tool-specific capture
variants or operation rules. The existing message-only visual presentation remains;
this refactor does not enable visible shortcut hints.

## Window input routing

`input_routing.rs` coordinates three steps without panel-specific branches:

1. `input_routing/facts.rs` reads rendered hitboxes, focus and blocking UI facts,
   event flags and surface-provided targets, keeping only the first valid capture
   snapshot in surface order.
2. `input_routing/policy.rs` resolves an immutable snapshot into Ignore,
   CancelCapture, Consume or an Invoke with a command ID and local target. It
   classifies keyboard ownership and applies capture/hover/focus priority,
   Escape policy, command scopes, binding order and repeat policy. Only key
   matching uses a host adapter.
3. The window executes that decision: cancel with the captured serial, consume
   a suppressed repeat, or construct and invoke the command once. A disabled
   local command remains matched and consumed.

Each `InputRegion` declares its target resolver. Viewport input declares its
binding and idle Escape action; Timeline declares its own panel binding;
Properties declares whether it currently targets a Timeline inspector or an
ordinary panel. Dock and panel lifecycle modules enumerate visible surfaces;
lifecycle cancellation also enumerates hidden surfaces. Target resolution does
not affect keyboard focus.

Pointer observation remains separate from keyboard policy. It records the native
menu target from the current rendered regions and deliberately retains that
target when the pointer enters menu chrome. Shortcut routing uses current capture,
hover or focus facts and never falls back to the saved menu target.

Capture cleanup still has two phases for GPUI borrowing: the local entity compares
the captured operation with its current operation and releases local state, then
returns core cleanup for the window to apply. Delayed cleanup cannot release a
replacement capture.

## Verification

Run focused checks from the repository root:

```sh
./tools/test-editor.sh fast viewport_targets::
./tools/test-editor.sh fast viewport_isolation::
# Native/headless UI libraries require macOS and Metal:
./tools/test-editor.sh ui headless_tests::input_routing
./tools/test-editor.sh ui input_routing::policy::
```

The shell entrypoint clears `ZIPZI_DEV_SESSION_DIR`. See
[test coverage](../../tools/README.md#test-coverage) for broader gates.


Core integration cases cover current-frame rejection, delayed frames, original
write targets, one-step history, cancel/rollback and independent viewport owners.
`viewport_targets.rs` also exercises automatic token release, abandoned captures
and selection changes without model evaluation. Headless canvas and input-routing
tests cover native pointer delivery, Escape, focus loss, tool switches and
configured-key hint resolution. Routing policy tests cover focus/IME ownership,
capture/hover/focus priority, scope filtering, Escape and repeat decisions without
a GPUI window. Input-routing headless regressions also cover Properties target
rebinding and stale/window-local Timeline hint snapshots.
