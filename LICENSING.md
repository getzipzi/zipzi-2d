# Licensing by component and origin

Moving files does not relicense them. The original SDK is [MIT](LICENSE-MIT).
The imported Editor is [GPL-3.0-only](LICENSE-GPL-3.0). Third-party notices and
individual file headers take precedence over these general source boundaries.

- SDK crates, including `cubism-matrix-zipzi`, retain MIT unless their own notices say otherwise.
- `crates/zipzi-editor` (including its assets), `crates/zipzi-editor-core` and `crates/zipzi-editor-render` retain GPL-3.0-only.
- Editor-derived tools, documentation and patches retain their original licensing after moving into shared directories.
  The historical source map (`docs/repository-merge/p3f-file-map.json` in Git history) records imported files with `origin: editor`, their original blobs and old paths. Retain that provenance when reusing or moving these files.
- Root `AGENTS.md`, `.gitignore`, `.gitattributes` and `models/README.md` combine SDK and Editor material; original portions retain their respective licenses. New integration glue does not grant a blanket MIT license to Editor content.
- `vendor/` contains third-party sources and Editor modifications: retain their embedded license notices, patch notices and [dependency record](vendor/README.md#dependencies).
- `third_party/purism-core` is a separate submodule under its own license. Live2D benchmark materials and local models have separate terms described in [third-party notices](THIRD_PARTY_NOTICES.md).

Historical source maps record provenance, not a license override. Binary/wheel/app
distribution must include the applicable notices; see the current
[distribution checks](tools/README.md#local-distribution-artifacts).
