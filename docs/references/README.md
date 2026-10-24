# Current references

These guides describe behavior and verification practices that still apply to
the current Editor. Consult the relevant guide when changing that area.

| Reference | Purpose | Implementation |
| --- | --- | --- |
| [Viewport tools and gestures](viewport-interactions.md) | Gesture ownership, capture lifecycle, cancellation and input routing | `zipzi-editor-core` tools/viewport modules and `zipzi-editor` input routing |
| [UI performance](ui-performance.md) | Selecting focused checks for redraws, caching, lists and rendering | Headless UI work-count tests and native benchmark harnesses |

The [benchmark guide](../../tools/benchmarks/README.md) owns runnable workloads,
timing boundaries and report interpretation. The [Editor README](../../crates/zipzi-editor/README.md)
owns setup and native acceptance; the [tool guide](../../tools/README.md#test-coverage)
owns test modes and CI coverage.

Keep current contracts and reusable guidance here. Put operational instructions
next to their owning crate or tool. Completed plans, one-off investigations and
measured results belong in [`docs/archive/`](../archive/), with their historical
scope and limitations preserved. A guide is current when it still describes the
implementation and has usable verification instructions; its age alone is not
an archival criterion.
