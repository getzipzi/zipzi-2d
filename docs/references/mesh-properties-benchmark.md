# Mesh properties native-window benchmark

Run the same Mao source model, Release build, isolated configuration, 30 warmup
inputs and 300 measured inputs at a target 120 Hz as the existing window UI
benchmark:

```sh
./tools/properties-benchmark.sh target/properties-run 300 120
```

`KASANE_BENCH_MODEL` overrides the model. `KASANE_BENCH_PROPERTY_CASE` selects one
case from the checklist below. The runner requires an unlocked Mac, a visible,
active native window, and real Metal. Avoid input and concurrent builds while it
runs. It chooses the visible mesh with the most vertices, restores committed
edits outside each measured phase, never saves the source, and quits at the end.

## Checklist

| Cases | Input path |
| --- | --- |
| `name`, `runtime_id` | GPUI text entry, replacement and Enter commit |
| `part` | Open searchable select, enter query, debounce, select; verify requested ID |
| `masks` | Search and add a different mesh, remove the first reference; verify added ID |
| `opacity_text`, `opacity_drag` | Text commit and uninterrupted Slide Number Input gesture |
| `multiply_hex`, `multiply_reset` | Inline HEX commit and restore-default suffix |
| `color_blend` | Cycle all 18 supported modes; verify each requested mode |
| `enabled` | Repeated checkbox changes |
| `picker_open_close` | Open the production popover and dismiss with Escape |
| `picker_tabs` | Switch RGB/HSV tabs |
| `picker_hex`, `picker_invalid_hex` | Live valid HEX preview and alternating invalid/valid drafts |
| `picker_hue`, `picker_sv` | Continuous color-wheel hue and SV-triangle drags |
| `picker_rgb_{0,1,2}_text`, `picker_hsv_{0,1,2}_text` | Edit every RGB and HSV numeric field |
| `picker_rgb_{0,1,2}_drag`, `picker_hsv_{0,1,2}_drag` | Scrub every RGB and HSV numeric field |
| `picker_done`, `picker_cancel` | Change color, confirm/cancel, reopen |
| `picker_swatches` | Hover original/current swatches, including tooltip dwell |
| `idle`, `picker_idle` | Blurred, static panel and open picker baselines |

Multiply Color is RGB; its picker has no Alpha control. Color-channel gestures
start from a non-grey color. Number scrubs use the existing Timeline number
probe's repeated 7-pixel movement pattern so quantized controls receive meaningful
changes. Continuous inputs never wait for a preview frame. Searchable selects
wait for their actual debounce; popup lifecycle cases wait for their new UI.

Only observation and pointer-isolation hooks require `test-support`. The native
runner reads observed bounds, dispatches actual pointer/key/IME text events, and
does not call headless helpers that force frames after each input. Pointer
isolation follows the existing animation-number benchmark: native AppKit hover
cannot interrupt scripted drags. Normal application builds do not include these
observation hooks.

## Measurements and interpretation

The output contains `environment.txt`, build/application logs, `trace.jsonl`,
`requests.json`, and `summary.json`. Each case reports measured input count,
observed value changes, timer lateness, actual input intervals, UI draw counts,
window presentations, request outcomes, and p50/p95/p99/max durations.

The reporter reuses `frame-trace-report.py` for request, texture lifetime and
presentation correlation. Metrics include scene evaluation, resource preparation,
encoding, model-preview GPU, UI CPU draw, and window-composition GPU. It sums
nested native event-handler scopes per input separately from interaction wall
time, which includes intentional debounce and popup waits. GPU durations are
Metal timestamps, not CPU submission duration.

Tabs, invalid drafts, tooltips and idle can legitimately reuse a model texture.
Their UI and composition work is reported by phase; the reporter does not invent
input-to-presentation latency for those changes. Missing latency counts stay
visible. The phase includes 250 ms of trailing settle time; idle rates should
use the reported phase duration.

As in the existing UI benchmark, missing samples/end records, dropped trace
events, failed native presentations, GPU errors, edit errors and incomplete runs
are failures. Debug measurements cannot pass Release validation. Cases that edit
values must demonstrate a changed property or live color/number value. Expected
no-change operations retain their zero change count.

Compare CPU and GPU stages with the existing 8.33 ms reference budget and retain
over-budget counts. End-to-end latency can span multiple refresh intervals;
exceeding 8.33 ms does not itself establish a dropped frame. Input cadence is
measured rather than inferred from the requested 120 Hz. Review request outcomes
alongside successful samples. Report recurring spikes or missing coverage even
when integrity checks pass; the script's zero exit status establishes valid
measurement, not universal performance health.

```sh
python3 tools/properties-trace-report.py target/properties-run/trace.jsonl --output target/properties-run
python3 -m unittest discover -s scripts -p 'test*trace_report.py'
```
