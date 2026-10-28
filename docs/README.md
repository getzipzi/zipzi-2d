# Documentation

- [Repository layout](../MONOREPO.md) and [licensing](../LICENSING.md)
- [SDK boundaries](../MONOREPO.md#sdk-and-language-boundaries), [Python SDK](../crates/zipzi-python2/README.md)
- [Editor development](../crates/zipzi-editor/README.md) and [dependencies](../vendor/README.md#dependencies)
- [Test domains and CI](../tools/README.md#test-coverage)
- [Source, wheel and app artifacts](../tools/README.md#local-distribution-artifacts)
- [Versioning, Alpha releases and package publication preparation](RELEASING.md)
- [Tool guide](../tools/README.md) and [model inventory](../models/README.md)
- [Runtime evidence](runtime/README.md) and [Runtime plan](archive/plans/sdk-runtime-implementation-plan.md), [R2 Editor migration](archive/plans/sdk-runtime/r2-editor.md)
- [zipzi-python2 construction plan](archive/plans/python-sdk-rewrite-plan.md): independent package, typed API, shared Runtime execution and bounded generation
- [Python P7 closeout](archive/plans/python-sdk-rewrite/p7.md): current macOS/CPython 3.14 scope complete; capability/contract evidence, generated/manual responsibilities, costs, agent results and final artifact identity
- [zipzi-python2 P3A refactor](archive/plans/python-sdk-rewrite/p3a-api-agent-contract.md): final API, generation boundaries, agent documentation and acceptance gates
- [Runtime R5 / Python P5 animation authoring](archive/plans/sdk-runtime/python2-r5-authoring.md): typed edits, records, SDK plans and addresses; Preview, spatial/GPU observation, fair scheduling and Editor playback isolation are delivered; [R7 acceptance](archive/plans/sdk-runtime/python2-r7-closeout.md) records final workflows, distribution and performance
- [Editor application](../crates/zipzi-editor/README.md), [editing core](../crates/zipzi-editor-core/README.md) and [render adapter](../crates/zipzi-editor-render/README.md)
- [Manual Mesh 编辑实施方案](archive/plans/manual-mesh-editing.md): authoring topology, dashed-edge caches, atomic editing and staged mesh generation
- [Windows 原生开发支持实施方案](plans/windows-development.md): tooling, platform services, Runtime IPC and Python validation first; GPUI/WGPU desktop integration last
- [Current topic references](references/README.md) and [benchmark guides](../tools/benchmarks/README.md)
- [Tooling tests](../tools/tests/README.md) and [shared fixtures](../fixtures/README.md)

`references/` holds current topic guides. `archive/` retains completed plans and
historical commands, measurements and paths.
Historical records may describe pre-merge layouts; do not run old commands verbatim.
Current tool paths are listed by function in [tools/README.md](../tools/README.md#directory-ownership).
Current commands run from the repository root unless a guide explicitly says otherwise.
