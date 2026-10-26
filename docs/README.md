# Documentation

- [Repository layout](../MONOREPO.md) and [licensing](../LICENSING.md)
- [SDK boundaries](../MONOREPO.md#sdk-and-language-boundaries), [Python SDK](../crates/zipzi-python/README.md) and [API](../crates/zipzi-python/API.md)
- [Editor development](../crates/zipzi-editor/README.md) and [dependencies](../vendor/README.md#dependencies)
- [Test domains and CI](../tools/README.md#test-coverage)
- [Source, wheel and app artifacts](../tools/README.md#local-distribution-artifacts)
- [Versioning, Alpha releases and package publication preparation](RELEASING.md)
- [Tool guide](../tools/README.md) and [model inventory](../models/README.md)
- [Runtime evidence](runtime/README.md) and [Runtime plan](plans/sdk-runtime-implementation-plan.md), [R2 Editor migration](plans/sdk-runtime/r2-editor.md)
- [zipzi-python2 construction plan](plans/python-sdk-rewrite-plan.md): independent package, typed API, shared Runtime execution and bounded generation
- [zipzi-python2 P3A refactor](plans/python-sdk-rewrite/p3a-api-agent-contract.md): final API, generation boundaries, agent documentation and acceptance gates
- [Editor application](../crates/zipzi-editor/README.md), [editing core](../crates/zipzi-editor-core/README.md) and [render adapter](../crates/zipzi-editor-render/README.md)
- [Manual Mesh 编辑实施方案](plans/manual-mesh-editing.md): authoring topology, dashed-edge caches, atomic editing and staged mesh generation
- [Current topic references](references/README.md) and [benchmark guides](../tools/benchmarks/README.md)
- [Tooling tests](../tools/tests/README.md) and [shared fixtures](../fixtures/README.md)

`references/` holds current topic guides. `archive/`, completed implementation/acceptance
plans and `plans/sdk-runtime/r0-*` retain historical commands, measurements and paths.
Historical records may describe pre-merge layouts; do not run old commands verbatim.
Current tool paths are listed by function in [tools/README.md](../tools/README.md#directory-ownership).
Current commands run from the repository root unless a guide explicitly says otherwise.
