# UI performance reference

Optional guidance for agents and contributors. Consult this reference when the
change involves redraws, large lists, caching, rendering, or an observed slowdown.
Use judgment to select relevant checks; this is not a mandatory workflow for every
UI change.

Choose verification by the change's actual risk. Do not run a full performance
workflow merely because a UI file changed.

- Localized text, color, icon substitutions, and small spacing adjustments normally
  need only relevant functional or visual checks. They do not automatically require
  a new test, benchmark, screenshots, or the full suite.
- Changes to notification subscriptions, cache boundaries, large collections,
  responsive layout, or frequent input handlers need a focused check of the affected
  behavior. Prefer deterministic work-count tests over timing benchmarks. For layout
  changes, include a representative resize or narrow-panel case when relevant.
- Use a release before/after comparison when changing GPU rendering mechanisms or
  frame scheduling, investigating a reported slowdown, or when structural tests
  cannot establish the impact. Measure the affected interaction first; expand to
  other models/workloads only when evidence or unresolved concerns justify it.
  Broad UI refactors should be split into independently verifiable changes.

Design constraints learned from this editor:

- Invalidate views when their displayed state changes. A new canvas frame should
  not redraw an unchanged header or unrelated panels. Keep real state changes,
  including zoom, enabled state, and modal state, working.
- Virtualize potentially large property, parameter, resource, and tree lists.
  Clipping or hiding overflow does not avoid offscreen layout and text measurement.
  Preserve scrolling, variable row heights, folding, and access to the last item.
- Reuse static decoration where practical. Small paths, masks, shadows, and rounded
  corners can add GPU passes; visual size alone does not indicate rendering cost.
- Keep static data and geometry out of repeated drag, playback, and resize work.
  Check cache reuse under those interactions, not only when the UI is idle.

Turn reproduced performance bugs into focused regression tests when practical.
Assert unnecessary draws, row construction, or cache rebuilds rather than machine-
dependent millisecond/FPS thresholds. Pair work limits with correctness checks so
an update that silently stops rendering cannot pass. Test-only counters should
not add production overhead; avoid exact whole-window node counts or source-text
assertions that make unrelated visual changes fail.

The small structural regression group is available when relevant and is also part
of the normal editor test suite:

```sh
cargo test --locked -p kasane-editor --lib headless_tests::ui_performance
```

For benchmarks, keep model, viewport, UI scale, and input workload comparable.
Verify that inputs actually change the model and present new frames; exclude and
document incomplete/no-op runs. Distinguish CPU work, GPU work, and end-to-end
latency/FPS. Claims about responsiveness or FPS need native-window measurements;
headless draw timings alone are insufficient. Retain paired results and report
remaining regressions or uncertainty even when another metric improves.
