# Licensing by component and origin

Moving files does not relicense them. The original SDK is [MIT](LICENSE-MIT).
The imported Editor is [GPL-3.0-only](LICENSE-GPL-3.0). Third-party notices and
individual file headers take precedence over these general source boundaries.

- SDK crates, including `cubism-matrix-kasane`, retain MIT unless their own notices say otherwise.
- `crates/kasane-editor`, `crates/editor-core`, `crates/editor-render` and `assets/editor` retain GPL-3.0-only.
- Editor-derived tools, documentation and patches retain their original licensing after moving into shared directories.
  The [complete source map](docs/repository-merge/p3f-file-map.json) identifies each imported file with `origin: editor`, its original blob and old path.
- Root `AGENTS.md`, `.gitignore`, `.gitattributes` and `models/README.md` combine SDK and Editor material; original portions retain their respective licenses. New integration glue does not grant a blanket MIT license to Editor content.
- `vendor/` contains third-party sources and Editor modifications: retain their embedded license notices, patch notices and [dependency record](docs/development/dependencies.md).
- `third_party/purism-core` is a separate submodule under its own license. Live2D benchmark materials and local models have separate terms described in [third-party notices](THIRD_PARTY_NOTICES.md).

The file map records provenance, not a license override. Binary/wheel/app distribution
must include the applicable notices; complete artifact validation is tracked in P5.
