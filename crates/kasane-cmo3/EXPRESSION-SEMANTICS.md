# Editable Cubism transfer

CMO3 carries model authoring data; CAN3 carries editable animation scenes.
The editor enables `Can3Options::auto_frame_rate` to infer a shared time grid
per declared-FPS group, retaining anchors and Bezier curves. Together with
`bake_animation`, it adapts inverse-step boundaries locally; whole-track
sampling at the declared FPS is only a fallback when no time grid is found.
See the module README for timestamp rounding and subframe limitations.

## Expression blending

An Animator expression scene contains absolute parameter poses. Runtime
expressions additionally apply Add, Multiply or Overwrite operations to a
moving baseline, with expression fades and transitions. These are different
representations: CAN3 scene blending does not offer per-parameter Multiply.
The [official expression workflow](https://docs.live2d.com/en/cubism-editor-manual/setting-and-exporting-facial-expressions/)
uses `exp3.json` in Cubism Viewer for these operations.

The exporter preserves both representations:

- Editable scene poses are calculated against model defaults and clamped like
  runtime expression targets.
- Each expression parameter's native `optionParam` string map includes a
  `KASANE_EXPRESSION_ENTRY_V1` record containing its original operation, value,
  scene/parameter identities and the emitted pose/default baseline. Cubism
  preserves this map when saving; it does not interpret the custom operation
  during native scene playback.
- Paired publication writes standard `expression-<UUID>.exp3.json` companions.
  `expressions.json` maps UUIDs and display names to these files. Load the exp3
  files in Viewer/SDK with the matching runtime model to retain dynamic blending.
  The companions are not needed to restore semantics when reimporting CAN3.

Unchanged CAN3 poses restore original operations and values, including values
hidden by clamping or a zero default. Editing a static pose recalculates its
operation relative to the current model default. An edited nonzero Multiply
pose against a zero default is rejected rather than silently converted to Add.
Copied scenes may receive a new GUID while retaining their operation records;
the original scene identity is provenance and does not block restoration.
Parameter identities must still match their channels.
Legacy CAN3 scenes without these records retain the existing Add inference.
Animated scenes remain motions instead of restoring stale static operations.

## Validation

`cargo test --locked -p kasane-cmo3 -p kasane-project` covers expression/motion
layering, fades, transitions, pose edits, archive framing and atomic companion
publication. Editor acceptance additionally uses the optional local v3a9 model.

With a local licensed Cubism installation, the native probe can save and reload
an exported CAN3 and compare its embedded operation records:

```sh
uv run --locked python tools/validation/check_cmo3.py --editor /path/to/Cubism \
  --resave-animation /path/to/new-saved.can3 /path/to/exported.can3
```

The destination must be new. This probe uses the installed Editor's APIs without
copying proprietary binaries into the repository.
