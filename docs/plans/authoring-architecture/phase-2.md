# Authoring 阶段 2 SDK 原生工程工作流

2026-11-06 完成，阶段提交为 `b6bf2ba3`。[架构方案与阶段索引](../authoring-architecture.md#实施记录)

SDK 在私有 `authoring` 模块中实现工作流，并从 crate 根导出相关接口。

- Snapshot 将冻结模型与精确 ResourceView 配对；正式与候选查询携带不同来源。
- Draft 独立持有 Builder 和资源增量，提供类型化改名、顶点编辑、批量创建和图片版本替换。
  成功 batch 推进 sequence；命令错误、被忽略的错误、无效最终图或 panic 均中止候选。
- PreparedChange 在发布前完成冻结和资源检查。新的 AuthoringSession 直接拥有 Snapshot，
  通过唯一安装点发布提交、Undo/Redo 与文档替换；来源、取消和计数溢出在安装前检查。
- 线性快照历史按 WriteSet 固定估计裁剪，使用 `target_bytes` 软目标；大步骤保留最近一步，
  Undo/Redo 不按字节拒绝或临时裁剪。发布时使用最新策略，失败或无净变化保留 redo。
- Core Builder 增加只读候选预览，不消费工作区；SDK 不通过旧 Session 或 Document 执行。
- Project `ProjectIo` 直接将 owned 解码值冻结为 DocumentState，接管并配对资源；没有旧
  Document 转换执行器。编解码共用严格 wire admission 与只读投影，归档格式保持原生 v22。
- 保存只将 ResourceKey 投影为归档内容路径。新旧格式消费者共用 ZIP 验证、预算、目标指纹、
  staging 和原子发布算法；保存不修改内容身份、资源键或历史。缺失/损坏图片保留明确未解析来源。
- SDK ProjectContext / SavedMarker 安装实际发布目标与 ContentId。保存持有独占 `&mut Session`
  借用；失败不改保存点，发布后的取消/目录同步警告按已发布处理。异步宿主排他任务接线属于阶段四。
- SDK 根入口已经直接切换，旧 EditSession / AuthoringSnapshot / ObjectHandle 等不再导出，
  旧 SDK 事务和 checkpoint 历史不进入编译路径。未使用新旧 facade 或运行时回退。

阶段边界：D6 的 Project 活文档所有权已退出正式 SDK 路径，D7/D8/D11 的旧 SDK 执行入口已切断；
旧领域/格式转换源文件中的 DocumentSession、checkpoint、计费和路径改写代码仍按阶段三至五删除。
旧 SDK 集成/mesh 测试与示例源保留为迁移输入，尚不能按旧接口编译。当前 fast gate 明确改为
`--lib --test authoring`，不将它宣称为原有全部领域覆盖。阶段三继续迁入领域算法和行为测试。

本阶段执行边界检查：`cargo check --locked -p zipzi-runtime --lib` 在 Observe 引用已删除的
AuthoringSnapshot / ConversionControl 等 SDK 接口处失败；这是阶段三/四尚未迁移的调用方，
不是已通过的 workspace 验证。Editor、Python native 和旧 SDK 的 regression/full gates 同样不在
本阶段通过范围；不添加兼容接口使它们暂时编译。[方案第 10 节](../authoring-architecture.md#10-实施顺序)允许这样的中间切换提交。

当前可运行入口：`cargo run --locked -p zipzi-sdk --example native_authoring -- INPUT.zipzi NEW_OUTPUT.zipzi`。

## 本地验证

| 检查 | 结果 |
| --- | --- |
| `./tools/test-sdk.sh fast` | 新 SDK 单元 14 项、原生工作流集成 6 项通过；包含三个已入库工程样本 |
| `cargo test --locked -p zipzi-sdk --doc` | 保存排他借用的 compile-fail 检查通过 |
| `cargo test --locked -p zipzi-project --tests` | 单元和集成共 159 项通过；包含新状态完整领域往返、旧文件格式互读和资源失败边界 |
| `cargo test --locked -p zipzi-core --lib state::` | 24 项通过 |
| `uv run --locked python tools/test_tools.py -p test_dev_entrypoint.py` | 10 项通过 |
| `native_authoring` 示例 | 对 pixel-binding.zipzi 实际执行加载、改名、Undo/Redo、另存并检查 ZIP 输出通过 |
| workspace package-domain、所改 Rust 文件格式、`git diff --check` | 通过 |

未运行 GPU/Windows 验证。上面的 Runtime 边界检查仍失败；该项及旧 SDK/宿主的未迁移 gates
不包含在本阶段通过结果中。
