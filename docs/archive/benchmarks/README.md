# Historical Kasane UI benchmark measurements

These reports were collected in the original `kasane-editor` / `kasane-2d`
checkouts before the Zipzi monorepo rename. They are archived without changing
recorded revisions, symbols, paths or binary/model hashes. They are **not**
measurements of the current Zipzi checkout. References into the original
`target/` directories require those local measurement artifacts.

The October 24 layout/paint report identifies additional per-frame UI work;
it does not establish that added buttons explain all residual Warp/Rotation
latency. The port includes the validated Properties/Select/menu fixes and
benchmark tools, without the speculative header/tool-bar optimizations or
isolated diagnostic instrumentation.

- [2026-10-23-physics-ui-retest-44accd1](2026-10-23-physics-ui-retest-44accd1.md) ([data](2026-10-23-physics-ui-retest-44accd1.json))
- [2026-10-23-ui-comparison-44accd1](2026-10-23-ui-comparison-44accd1.md) ([data](2026-10-23-ui-comparison-44accd1.json))
- [2026-10-24-basic-modeling-regression](2026-10-24-basic-modeling-regression.md) ([data](2026-10-24-basic-modeling-regression.json))
- [2026-10-24-menu-capability-fix](2026-10-24-menu-capability-fix.md) ([data](2026-10-24-menu-capability-fix.json))
- [2026-10-24-ui-draw-diagnosis](2026-10-24-ui-draw-diagnosis.md) ([data](2026-10-24-ui-draw-diagnosis.json))
- [2026-10-24-ui-draw-fix](2026-10-24-ui-draw-fix.md) ([data](2026-10-24-ui-draw-fix.json))
- [2026-10-24-warp-rotation-layout-paint](2026-10-24-warp-rotation-layout-paint.md) ([data](2026-10-24-warp-rotation-layout-paint.json))
