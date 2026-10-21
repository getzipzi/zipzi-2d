# 对象创作核心架构改造实施记录

对应方案：[对象创作核心架构改造方案](unified-object-authoring-core.md)。本文件记录已交付的接口、实现边界与验收结果；完整目标范围和后续阶段仍由方案维护。

## C1 / Stage 1：Expression / ExpressionEntry

2026 年 10 月 21 日：Expression / ExpressionEntry 切片已落地于相邻
`kasane-2d` 和本仓库。后续 C2–C6 与 UI 统一仍按原计划推进。

- **原生模型与领域操作**：`ExpressionEntry.id` 使用独立 UUID；
  `ExpressionTarget::Unspecified` 与已解析、导入未解析目标分开保存。
  `ExpressionEdit` / `ExpressionPlan` 负责空 Expression、空条目、字段补丁、
  跨 Expression 移动、复制、删除和资源顺序；多选按文档来源顺序处理，
  `before` 使用稳定条目 ID。重复参数目标允许保留，执行沿用首条匹配语义，
  条目重排因此具有业务含义。复制生成新的条目和来源场景拥有的轨道/关键帧身份，
  参数等外部引用保持共享。锁定在 core 校验，整资源替换与普通删除也不能绕过。
  core 计划另外绑定内存快照身份：未修改的 clone / candidate 共享基线，任何内容
  修改、恢复或历史交换都会更换身份，批量构建期间也不例外。文档 ID 与 revision
  相同的分叉不能互用计划，避免覆盖另一分支的数据或绕过另一分支的锁定。
- **SDK 规划与生命周期**：`plan_expression_edit` / `execute_expression_plan`
  与 `EditSession::edit_expression` 复用同一领域规则和现有历史；计划绑定完整
  SDK Version，文档变化后需重新规划。新增条目复用 `ObjectHandle` / incarnation，
  移动不改句柄，删除后 Undo/Redo 不复活旧句柄。
  `ExpressionEntryAddress` 捕获条目、拥有者句柄、拥有者上下文 epoch 和字段基线；
  无关条目修改后可以在最新快照重新规划。跨拥有者移动会推进上下文 epoch，Undo
  移回原处不会复活旧地址。已生成的计划与完整候选提交仍校验完整 SDK Version。
  `ExpressionEntryCandidate` 支持可恢复的非法预览、取消、no-op 和一次提交。
  候选在删除恢复、跨会话或重载后不可提交。
- **查询与 Editor 适配**：`expression_capabilities`、`expression_collection`
  提供领域能力和条目顺序；editor-core 的
  `plan_expression_authoring` / `execute_expression_authoring` 返回
  `OperationResult`，仅附加选择与定位建议。
  `expression_authoring_capabilities` 分开保留 SDK 原因和 Editor 候选占用原因。
  Editor 的 `begin_expression_entry_edit` 返回 `ExpressionEntryEditToken`，候选由
  `Document.pending` 持有，通过 `preview_expression_entry_edit` 更新；SDK 的独立
  candidate 仍供 SDK 调用方使用。取消、选择/检查目标变化、工作区上下文变化、
  失焦及关闭会废弃 Editor token；Save / Finish 最多提交一次，迟到的更新或提交
  被拒绝。预览使用候选投影，非法预览保留上一有效值，取消及 no-op 保留 redo。
  对象投影可按独立条目 ID 检查拥有者、参数引用和诊断；结构操作不会自动激活
  Expression。既有单击激活和淡入淡出逻辑保留。
- **运行与格式**：`expression_projection` 提供过滤后的条目、双向槽位映射和
  `NeedsConfiguration` / 未解析参数诊断；独立 Expression 预览、Motion 中的
  Expression 阶段及可用内容导出共用它。坏条目不阻断其余条目；结构非法仍拒绝。
  原生保存重开保留 ID、顺序和未完成状态，不提供旧原生文件迁移。
  严格 exp3/Cubism 导出给出诊断，第三方条目不携带内部 ID，重导入时生成新身份。
  `plan_available_expression_export` 明确列出省略项，
  `export_expression_plan` 在版本变化后拒绝旧的接受结果；不透明内容仍受已有保护规则约束。
- **发布与缓存**：`EditReceipt.changes` 提供可组合类别、实体增删改、集合和引用变化；
  C1 的直接替换、导入、候选与规划执行均计算实际 Expression 差异，Undo/Redo 返回
  实际方向。项目整体替换显式发布全量失效，no-op 不发事件、不清 redo。
  其余领域的精细语义继续在对应切片补齐，现有 `ChangeKind` 保留兼容。
  普通条目字段和 fade 使用原位字段补丁；条目索引按结构失效，运行投影按版本缓存、
  在 seek checkpoint 间共享；变更记录复用提交已有的身份集合。
  Expression 名称和资源列表顺序不推进求值版本。
  锁定等持久化 EditorState 变化以独立 `EditorState` 类别报告，包含直接受影响
  的实体及能力依赖的拥有者/条目；Undo/Redo 使用同一摘要。SDK 的统一发布路径
  同时维护字段地址上下文，只有受影响的条目需要更新。Editor 的 Expression
  命令、属性提交与 Undo/Redo 共用 receipt 消费路径；普通字段只更新相关资源与
  条目载荷、关系说明，保持无关索引行共享。集合或引用端点变化才重建关系索引，
  Expression 操作不使 Canvas 拓扑缓存失效。
- **Python**：新增条目创建、配置、移动、复制、删除、资源复制/排序和只读移动规划；
  使用同一 SDK 事务与领域规则。`handle("expression_entry", id)` 与
  `EditEvent.changes` 暴露相同的身份生命周期和提交变化。

本轮 review 重构验收：Editor 工作区通过 727 项 Rust 测试、原生动画（三个本地
模型）、Metal 截图与 Timeline 验收，5 项按默认忽略；相邻五个相关 Rust 包通过
398 项，1 项性能测量默认忽略。两仓库格式检查与 diff 检查通过。
完整脚本首次在开发启动超时用例失败；44 项脚本测试独立重跑全部通过，Rust 与
原生验收按相同参数单独执行通过，未修改开发脚本。各命令覆盖存在重叠，不将
专项与全量数字相加；默认忽略的性能测量不计入通过数。

本轮 review 重构新增 15 项回归：SDK 的 `expression_entries` 共 24 项，Editor 的
`expression_authoring` 共 8 项。覆盖相同 revision 的分叉计划拒绝、锁定保护、
批量构建期间快照变化、无关字段修改后重新规划、移动再 Undo 的地址失效、
锁定变化及反向事件、取消与上下文变化后的迟到 token、跨文档 token、非法预览
恢复、no-op 保留 redo、保存单次提交，以及预览/提交/Undo/Redo 的无关索引行共享。
引用变化后的取消还验证关系说明和诊断恢复。完整目标范围与 C2–C6 顺序不变。

验收入口：

```sh
# Editor 格式、脚本、核心、无界面 UI 与原生验收
./scripts/test.sh

# 相邻 kasane-2d 的领域、格式、动画与 SDK 回归
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked \
  -p kasane-core -p kasane-sdk -p kasane-animation -p kasane-project -p kasane-cmo3

# C1 专项（含保存、格式、句柄、候选、复制闭包与 no-op）
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked \
  -p kasane-sdk --test expression_entries
```

初次 C1 实施时，Python wheel 已构建并在仓库外的临时隔离环境安装，运行
`modules/kasane-python/tests/test_cpu.py`，包含条目端到端和 ChangeSet 方向校验。
现有 `tools/validate_sdk.py` 在开始测试前引用缺失的
`examples/sdk/python_observe_recipe.py`，因此当时直接执行其 CPU 套件并通过 53 项，
不将完整 recipe 验证器记为通过。本轮重构未重跑 Python wheel CPU 套件，
也不宣称新的 Release 延迟或内存基准数据；
现有 UI 工作量与行为回归由 Editor 套件覆盖。
