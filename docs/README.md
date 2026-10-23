# Documentation

- [Repository layout](../MONOREPO.md) and [licensing](../LICENSING.md)
- [SDK architecture](ARCHITECTURE.md), [Python SDK](../crates/kasane-python/README.md) and [API](../crates/kasane-python/API.md)
- [Editor development](development/editor.md) and [dependencies](development/dependencies.md)
- [Test domains and CI](development/testing.md)
- [Tool guide](../tools/README.md) and [model inventory](../models/README.md)
- [Runtime preparation](runtime/README.md) and [Runtime plan](plans/sdk-runtime-implementation-plan.md)
- [Repository merge plan](plans/repository-merge-plan.md) and [execution evidence](repository-merge/README.md)

`references/` holds current topic guides. `archive/`, completed implementation/acceptance
plans and `plans/sdk-runtime/r0-*` retain historical commands, measurements and paths.
Repository merge P0–P3 evidence is immutable. Resolve historical paths using the
[P3F file map](repository-merge/p3f-file-map.json); do not run old commands verbatim.
Current commands run from the repository root unless a guide explicitly says otherwise.
