# Kasane Editor 设计与阶段规划

本目录记录 Editor 的阶段规划，以及后续按阶段编写的详细实施文档。

- [阶段路线图](ROADMAP.md)：目标、依赖、范围、交付物和退出条件。
- [实施文档模板](IMPLEMENTATION_TEMPLATE.md)：进入某个阶段前复制并填写。
- [实际架构与内存接口](ARCHITECTURE.md)：Stage 00–01 的模块和 API。
- [Document specification](DOCUMENT_SPEC.md)：已实现的内存数据规格。
- [项目格式 v1](PROJECT_FORMAT.md)：Stage 03 的 JSON 格式、资源引用与安全保存约定。
- [Stage 00–01 验收记录](stages/00-01-delivery.md)：交付物、环境、运行命令与证据。
- [Stage 02–03 验收记录](stages/02-03-delivery.md)：事务、Undo/Redo、保存与重开证据。
- [Stage 04 验收记录](stages/04-delivery.md)：本地连接、Python SDK、外部进程编辑证据。

核心路线先打通固定样例与脚本的“修改 → 预览 → 保存 → 播放”流程。画布先只读；人工网格编辑移至可选后续阶段 H，不作为任何核心阶段的验收前提。

Stage 00–04 已在 macOS arm64 完成并验收，Stage 05 起尚未开始。已有 runtime 能力不等同于已完成后续 Editor 阶段。

Stage 00 已交付可复用的内存网格创建、更新与释放接口，Stage 01 的 Document 已复用该接口。Stage 02–03 在同一个修改入口上增加短事务、内存历史和项目持久化；Stage 04 允许外部 Python 直接编辑正在运行的 Document。下一步是 Stage 05 的变形器。

后续实施文档建议命名为 `stages/00-memory-preview.md`、`stages/01-document.md` 等；完成每个阶段后，在路线图中更新状态并链接实际验收证据。
