# Dependencies

| Dependency | Source |
| --- | --- |
| GPUI Kit 0.7.3 | [longbridge/gpui-kit](https://github.com/longbridge/gpui-kit), pinned to `507e82b8115ac73af92b7d3856501f9d7b73b2c9` |
| GPUI Fast 0.1.7 | Local patches for `gpui-fast`, `gpui-fast-macos`, and `gpui-fast-apple` under `vendor/`. See [patch details](vendor/README.md). |
| Kasane | [kasane-2d](https://github.com/pakholeung37/kasane-2d), revision `94e3697` or a compatible descendant, checked out at `../kasane-2d`. |
| Metal 0.33 | crates.io |

Sources and overrides are defined in [Cargo.toml](Cargo.toml). All workspace crates
share [Cargo.lock](Cargo.lock); build and test with `--locked`.

Kasane is a local path dependency, so its revision is not pinned by Cargo.lock.
Use the same checkout for all Kasane crates, including `kasane-sdk-observe` for
tests. No sibling GPUI checkout is needed.

The workspace enables `gpui-fast` on both GPUI Kit and the optional FPS monitor.
The three local Fast patches preserve native Metal surfaces, shared queues,
headless rendering, and frame tracing. Kit also retains upstream `gpui-pre`
0.3.9 dependencies in its graph; its Fast feature selects the Fast re-exports
used by the editor and playground. Test renderers use `gpui-fast-apple` too.

The Fast backend and local cache-boundary fix are the accepted default.
See the [old Kit comparison and adoption decision](docs/gpui-kit-old-vs-fast.md)
for measured CPU improvements and retained presentation-latency tradeoffs.
