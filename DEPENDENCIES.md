# Dependencies

| Dependency | Source |
| --- | --- |
| GPUI Kit 0.6.5 | [longbridge/gpui-kit](https://github.com/longbridge/gpui-kit), pinned to `5059de47a6560fdbe24e32cd91b45d6aa6849695` |
| GPUI 0.3.6 | Local patches for `gpui-pre`, `gpui-pre-macos`, and `gpui-pre-apple` under `vendor/`. See [patch details](vendor/README.md). |
| Kasane | [kasane-2d](https://github.com/pakholeung37/kasane-2d), revision `8d448d0` or a compatible descendant, checked out at `../kasane-2d`. |
| Metal 0.33 | crates.io |

Sources and overrides are defined in [Cargo.toml](Cargo.toml). All workspace crates
share [Cargo.lock](Cargo.lock); build and test with `--locked`.

Kasane is a local path dependency, so its revision is not pinned by Cargo.lock.
Use the same checkout for all Kasane crates, including `kasane-sdk-observe` for
tests. No sibling GPUI checkout is needed.
