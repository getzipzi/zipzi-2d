# 对象创作核心架构改造实施记录

对应方案：[对象创作核心架构改造方案](unified-object-authoring-core.md)。本文件记录已交付的接口、实现边界与验收结果；完整目标范围和后续阶段仍由方案维护。

截至 2026 年 10 月 22 日，**C1–C4 已完成，C5 已启动并交付 Pose 首批**；其他 C5 领域与 C6 仍待实施。以下按交付顺序保留
历史阶段状态；C4 的支持矩阵与最终验收以对应的 C4 完成及 review 修正章节为准。

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

## C2：Motion 库、注册与 Attachment 引用

2026 年 10 月 21 日：C2 已贯通相邻 `kasane-2d` 与本仓库，新的菜单与表单交互仍由后续 UI 阶段接入。

- **原生库与领域规划**：`MotionGroup.id`、`MotionRegistration.id` 独立于名称、路径和数组位置；组与显式根注册各保存一份顺序。`MotionEdit` / `MotionPlan` 提供空组、默认 Clip 与注册的联合创建、组改名/排序/解散/复制、注册添加/字段补丁/移动/复制/删除，以及 Clip 内容复制和资源删除。组复制共享 Clip；内容复制生成新的 Track、Key、Event ID。删除最后注册或其组时，迁移原注册到根列表，计划列出 `migrated_to_root`，保留 ID 与覆盖。根列表中最后注册的删除请求可以是 no-op。直接库替换也保留失去入口的 Clip 与原注册，显式删除 Clip 才移除其全部注册与拥有内容。
- **轨道和时间内容**：`MotionTrackTarget::Unspecified` 可持久化；`motion_projection` 提供禁用原因及双向运行槽位映射。`track_order` 单独记录展示顺序，空值表示执行顺序；非空时必须是完整 ID 排列。同 Clip 的轨道移动和行重排不改变执行数组，重复写入目标的结果保持。跨 Clip 的轨道移动/复制、事件创建/移动/复制/删除及跨轨道关键帧剪切/复制由领域规划联合验证；事件按时间稳定排序。同时间 Key 冲突、目标不兼容、越界时间及不能直接保持的 Bezier 模式转换原子拒绝。关键帧粘贴保留连续选中区间的内部插值，新增连接使用 Linear，不承诺保持目标曲线。`MotionCurveEdit` / `MotionCurvePlan` 复用现有 SDK 的插点、删除、时间移动、插值、Bezier 柄和时长缩放规则。
- **SDK 与有效期**：`plan_motion_edit` / `execute_motion_plan` 和 `EditSession::edit_motion` 共用 core 规则与唯一事务历史；计划绑定 SDK Version 与 core 快照身份。组、注册、Track、Key、Event、Attachment 全部进入既有 ObjectHandle/incarnation 集合，删除恢复不复活旧句柄。`MotionRegistrationAddress` 校验字段基线、拥有者句柄和不回退的上下文 epoch。能力与集合查询区分不支持、锁定及待配置；core 的直接替换、属性修改和删除入口也校验持久化锁定。
- **附件内容与引用**：`PackageAttachment` 增加 ID，`bytes: Option<Arc<Vec<u8>>>` 区分无内容与零字节内容；路径独立可改。`AttachmentRef` 区分稳定 ID 与导入未解析路径，Motion sound 与 UserData 均使用此引用。导入成功加载的路径绑定为 ID；未加载项保留未解析状态。`AttachmentEdit` / `AttachmentPlan` 提供创建、替换、共享内容的记录复制、受引用保护的删除和引用修复。`AttachmentLoadToken` 同时校验句柄与内容 epoch，内容替换再 Undo、删除再 Undo 或重载后，旧异步结果都不可写回。声音播放仍维持既有未支持提示，不阻止视觉预览。
- **发布与 Editor**：ChangeSet 增加 Motion 集合、类型化引用变化与资源内容变化；直接 SDK 写入、候选提交和 Undo/Redo 使用同源摘要，包含新子对象。Editor 的 `MotionSelection.entry` 改存注册 ID，组名和序号仅作为展示位置。`plan_motion_authoring` / `execute_motion_authoring` 附加选择建议，不激活动作；正在编辑的 Timeline 候选会阻止结构提交。对象索引按独立 ID 投影组、注册、Track、Key、Event 和附件。库改名、移组与行整理刷新投影而不重建预览运行；删除当前注册清理预览目标，不自动选择另一注册。
- **保存、外部格式与 Python**：原生保存保留空组、显式根注册、覆盖、展示顺序、待配置轨道与附件 ID/内容状态。model3 按当前附件 ID 解析路径；根注册无覆盖时可导出到 Default 组，有注册覆盖时返回 `ROOT_REGISTRATION_NOT_REPRESENTABLE`，不写回共享 Clip。空 MotionGroup 也导出。第三方格式不保存组、注册或关键帧的内部 ID；重导入生成新身份，往返测试比较外部语义。`plan_available_motion_export` / `export_motion_plan` 显式列出轨道省略并校验接受时的版本，保留不透明内容保护。Python 的 `Edit.motion_authoring`、`Edit.attachment_authoring`、`Session.plan_motion_authoring` / `execute_motion_plan` 解析类型化 enum 后调用同一 SDK，计划可查询受影响 ID 与根迁移；新增对象类型可通过 `handle` 查询。
- **规划成本**：库规划使用只包含 Motion、参数/Part 描述与附件引用的工作区，共享曲线和内容 blob；纯库整理不复制场景几何、不验证所有未变化曲线。拥有内容的 ID 查询使用共享的惰性索引。Release 合成库（65 Clips、2048 Tracks、8192 Keys）100 次移组规划总计 79.61 ms，单次提交 123.77 ms；验证未修改曲线 Arc 保持共享。这是单次本机测量，不是属性链或 Timeline 的前后性能对照；全领域性能验收仍归 C6。

验收入口：

```sh
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-sdk --test integration motion_library::
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked \
  -p kasane-core -p kasane-sdk -p kasane-animation -p kasane-project -p kasane-cmo3
cargo test --locked -p editor-core --test integration
./scripts/test.sh
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked --release \
  -p kasane-sdk --test integration motion_library::library_planning_shares_curves_in_a_large_library -- --ignored --nocapture
```

本轮验证：Editor 工作区 729 项 Rust 测试通过、5 项默认忽略；原生动画验收通过三个本地模型，Metal 截图通过。相邻五个领域包 413 项通过、2 项默认忽略，其中 Motion 库专项 15 项通过、1 项性能测量默认忽略；Editor 新增两项播放连续性/删除选择回归，已包含在 333 项 editor-core 集成测试中。Python wheel 在仓库外隔离环境安装后通过 54 项 CPU 测试，脚本测试隔离重跑通过 44 项。格式与 diff 检查通过；专项与全量数字有重叠，不相加。

`./scripts/test.sh` 首次在既有开发启动超时用例失败，脚本隔离重跑全部通过。单独运行完整 Rust/原生工作区时，Timeline 首帧等待曾超时；随后以同样的验收逻辑单独重跑，完整通过导入边界、真实/密集曲线编辑、取消、播放、画布导航、dock 调整、SDK 结果对照与保存往返。没有修改测试的超时策略，也不把第一次完整脚本执行记为成功。

原生模型按项目未发布的约定采用破坏性修改，不新增旧原生文件迁移。C2 交付时 C3–C6 与完整 UI 统一尚未实施；C3 的后续交付见下文。

### C2 review 修复

2026 年 10 月 21 日：补充检查直接 SDK 写入、同事务删除重建、锁定、有效内容导出与 Editor 播放/索引刷新。

- 注册重排按容器位置检查锁定，整库替换遵守相同规则；轨道显示顺序的锁定检查统一进入 core，直接替换 Clip 不能绕过。规划的受影响注册包含位置变化。
- 跨 Clip 移动或复制含 Bezier 的整轨道时，运行模式不同则原子拒绝，避免仅通过数据合法性校验却改变曲线行为。关键帧剪切只重建实际受影响的轨道，保持其他轨道的曲线 Arc 共享。
- SDK 抽出统一的实际删除身份记录，供 Motion、Attachment、整库替换和通用删除复用。附件同事务删除再重建会使原句柄及加载 token 失效；删除最后根注册的 no-op 不产生历史、不清 redo，也不改变 incarnation。
- 附件规划只校验本操作能修改的 UserData 引用，不再用缺少场景几何的工作区重验完整 Model3Settings；已有合法 HitArea 不妨碍附件创建和修复。
- 可用 Motion 导出直接编码过滤后的 Clip，不通过文档写入入口生成临时状态；锁定不阻止只读导出，原有不透明内容基线检查继续生效。
- 新增或移除共享注册不再被当作已有播放输入变化。Editor 独立检查当前注册是否仍存活，删除当前注册仍清理预览；选择新注册时使用最新快照。根注册也保留声音不支持提示。
- Motion/Group/Registration 的普通字段变化增量更新对象载荷，Clip 改名同步注册标签；Undo/Redo 走同一路径，保持无关对象索引行共享。

新增 7 项 SDK 回归与 3 项 Editor 回归，并扩展关键帧剪切测试检查曲线共享。本轮未测量新的 Release 延迟或内存基准，也未重建 Python wheel。

最终验证：Editor 工作区 732 项 Rust 测试通过、5 项默认忽略，三个本地模型动画验收、
Metal 截图和完整 Timeline 原生验收通过；相邻五个领域包 420 项通过、2 项默认忽略。
Motion 库专项 22 项通过、1 项性能测量默认忽略，Editor Motion authoring 专项 5 项通过；
这些专项包含在全量结果内，不重复相加。两仓库格式与 diff 检查通过。

脚本检查最初在 `test_late_pid_after_timeout_is_recovered_on_next_reload` 失败，
重跑 `test_dev.py` 仍有同一失败；编译结束后单项及完整 44 项脚本套件均通过。
未修改开发脚本或放宽超时断言。中间一轮领域测试曾出现 Mao 项目锁占用，
最终完整领域套件的同一用例通过；上述数量来自最终完成的测试轮。

## C3：Physics 条目、节点与运行投影

2026 年 10 月 21 日：实现覆盖相邻 `kasane-2d` 与本仓库；验收结果在本节末尾记录。

- **原生模型与外部身份**：`document::PhysicsDefinition` / `PhysicsSetting` /
  `PhysicsInput` / `PhysicsOutput` / `PhysicsNode` 与 physics3 DTO 分开。
  Group、Input、Output、Node 都有独立 UUID；Group 的 `external_id` 保留外部组标识，
  名称继续只由原生 dictionary 提供。输出保存 `node_id`，转换时才生成
  `VertexIndex`。`PhysicsTarget` 区分未指定、稳定参数 UUID 与导入未解析引用。
  原生计数由集合长度派生；不再保存 runtime ID 到 UUID 的第二份绑定表。
- **领域操作与原子校验**：`PhysicsEdit` / `PhysicsPlan` 提供根资源与默认组的
  联合创建、空通道创建、字段补丁、输入/输出跨组移动、组执行顺序、组和通道复制、
  节点插入/专用重排/复制，以及带显式节点替换的联合删除。组复制重建全部拥有内容
  的 ID 并重映射输出节点，外部参数引用保持共享。移动保留文档来源顺序；输出在
  新组没有合法节点时拒绝，专用转移可按输出 ID 指定新节点。删除被输出使用的节点
  默认拒绝并返回使用者；根节点不能单独移走或删除，删除节点不能把已有双节点链
  缩为单节点。删除组或根资源包含其拥有内容。失败和 no-op 不改变历史。
- **SDK、字段地址与锁定**：`plan_physics_edit` / `execute_physics_plan` 和
  `EditSession::edit_physics` 共用 core 规则与唯一历史。计划绑定 SDK Version 与
  core 快照身份；新增实体全部进入既有 ObjectHandle/incarnation 体系。
  `PhysicsFieldAddress` 捕获条目/节点、拥有者句柄、字段基线与不回退的拥有者上下文
  epoch；项目载入/替换时初始化拥有者上下文，同组重排后仍定位原实体，跨组移动再 Undo 也不能复活旧地址。
  `physics_capabilities` / `physics_collection` 提供类型化查询。
  持久化锁定覆盖根、组及子条目，字段补丁、整资源替换、规划及通用删除不能绕过。
  同事务删除再以原 ID 创建也会使旧句柄失效。
- **运行投影与观察**：`physics_projection` 单独过滤未指定/未解析通道，不重归一化
  其他权重；结构非法和不存在的采样节点仍拒绝。每组保留双向通道槽位映射和节点
  顺序。`PhysicsRuntime` 在构建时持有共享投影，seek checkpoint 共享它，求解循环
  不逐帧重新过滤原生资源。`PhysicsRigSnapshot` 带原生 Group、Input、Output、Node
  ID；Editor 的输入观察值及输出详情通过 ID 查槽位，避免中间通道省略后误定位。
  既有求解器数值行为和测试预览继续由原有轨迹回归验证。
- **发布与 Editor 接入**：ChangeSet 增加 Physics 集合和参数/节点引用变化，报告
  实际实体增删改、标签、顺序、拓扑与求值输入；Undo/Redo 使用实际恢复方向。
  editor-core 的 `plan_physics_authoring` / `execute_physics_authoring` 附加选择
  建议，不激活 Motion/Expression。现有组/通道/节点结构按钮接入领域意图，数值
  候选继续使用局部字段补丁和已有取消/提交生命周期。重复字段的 `PropertyKey`
  改为携带实体 UUID；旧下标字段写入被拒绝。Physics 属性提交、结构操作和历史
  共用回执消费；字段和标签可增量刷新载荷，保留无关索引行及 Canvas 拓扑缓存，
  标签变化不重建动画预览。全量新菜单和交互仍归后续 UI 阶段。
- **保存、格式与 Python**：原生保存保留全部 ID、空通道、语义顺序、未解析引用与
  扩展内容，不提供旧原生格式迁移。physics3 与 CMO3 显式转换内部身份和节点引用，
  不把内部 UUID 写入外部格式；严格导出区分 `NEEDS_CONFIGURATION` 与
  `UNRESOLVED_PARAMETER`。`plan_available_physics_export` / `export_physics_plan`
  列出省略通道并拒绝旧版本的接受结果，使用与预览同源的过滤规则；不透明内容
  保留已有内容与命名空间保护。Python 的 `physics_authoring`、只读规划/执行及
  新句柄类型调用同一 SDK；`create_physics(id, name)` 联合创建根与合法默认组并
  返回组 ID，`replace_physics` 接收新的原生快照。
- **规划和存储成本**：结构规划浅复制 Physics 根与组 Arc，不复制场景几何；字段
  规划保留类型化局部补丁，执行时复用局部校验与写入。控件绑定稳定字段键时借用当前资产，避免逐控件
  复制根元数据。字段写入保留未变组共享，历史内存估算改为类型化字段统计，不再序列化
  整份 Physics 数据。Release 专项沿用同一写入微基准，结果见下表。

验收入口：

```sh
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked \
  -p kasane-sdk --test integration physics_
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked \
  -p kasane-core -p kasane-sdk -p kasane-animation -p kasane-project -p kasane-cmo3
env -u KASANE_DEV_SESSION_DIR cargo test --locked --workspace \
  --features kasane-editor/test-support
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked --release \
  -p kasane-sdk --test integration physics_editing::physics_field_patch_benchmark -- --ignored --nocapture
```

最终验证：相邻五个领域包 **434 项通过、2 项默认忽略**；Editor 工作区 **734 项通过、
5 项默认忽略**，另通过三个本地模型的原生动画、Metal 截图及完整 Timeline 验收。
Physics 专项为 14 项 authoring 加 6 项字段编辑回归；Editor Physics 集成专项 9 项。
这些专项均包含在全量结果内，不重复相加。最终 Python wheel 在仓库外隔离环境通过
55 项 CPU 测试；44 项 Editor 脚本测试通过。格式与 diff 检查通过。
控件字段绑定的借用优化后，相关 11 项 UI/表单测试再次通过。Mao 属性链基准在
`unique` 与 `retained` 两种旧索引读者策略下完成最小冒烟校验（2 samples、1 warmup、
2 updates），均返回 `validated=true`；该冒烟不作为新的延迟分布或前后 A/B 结论。

无其他构建/验收进程运行时，直接执行已构建的 Release Physics 微基准三轮，中位数：

| 配置 | 整资源 setter | 字段 patch（包含保留旧读者的 fork） |
| --- | ---: | ---: |
| 16 组 × 4 节点，2000 次交替写入 | 54.51 μs/次 | 4.37 μs/次 |
| 192 组 × 24 节点，600 次交替写入 | 2084.68 μs/次 | 20.37 μs/次 |

该测量沿用[字段补丁基准](../benchmarks/2026-10-20-physics-patches.md)的工作负载与计时边界，
采用新的稳定身份模型，未做保存前后二进制的严格 A/B；不能据此推断整体界面帧率。
测量前修正了锁定检查对整个物理资源的重复拥有者扫描；普通字段补丁只校验当前组与
直接条目标记，已验证的成员关系不会再触发全资源扫描。

中间验收修正了新增观测用例的零时刻初始化、旧组合 ID 断言，以及删除当前物理组后的
相邻组检查目标恢复。补充了保存重开后首次同组重排仍可使用原字段地址的回归，重载后
旧地址仍被拒绝。早期重叠的领域测试曾遇到 Mao 项目锁占用，最终完成的领域套件中
同一用例通过；没有放宽断言或超时。上述通过数来自最终完成的测试轮。

C3 交付时，C4–C6 与完整 UI 统一仍待实施；C4 的后续进展见下文。

### C3 review 修复

2026 年 10 月 21 日：复核两个工作区的未提交实现，补充以下修复与回归。

- Physics 根资源的 FPS 补丁没有 modified group，原增量刷新因此漏掉所有组的
  Properties 缓存。现在独立同步资产级 FPS，并覆盖 Undo/Redo；仅在 FPS 改变时
  查询物理组，不为普通字段编辑扫描场景对象。
- Input/Output 的轴类型变化会更新字段，但原对象索引名称仍显示旧轴。增量刷新
  同步通道名称，同时保留未变化对象行和旧快照。
- 合法的单节点导入 rig 删除输入/输出时，原逻辑无条件要求至少两个节点。
  最短链校验现在只针对实际删节点；通道删除保持节点链不变并可一次撤销。
- 节点删除因输出引用而被拒绝时，原 Properties 面板仍会切换到相邻节点。
  结构操作返回成功状态，只有成功后才调整通道/节点选择；新增 UI 回归检查
  拒绝后的选中节点和历史均保持。
- Physics 根 ID 与已有 Parameter 等对象冲突时，原只读规划会成功、提交才拒绝。
  ID 唯一性检查归入共享验证，规划与执行使用相同规则。
- `PhysicsEdit::Patch` 原先构建整资源替换计划，执行进入 `set_physics` 并扫描
  全模型身份集合。计划现在保留类型化局部 patch，执行复用 `patch_physics`；
  SDK 的直接统一 patch 同样复用该入口，字段计划不扫描删除身份。
  保留版本/快照检查和提交校验，回归对照三个入口的 ChangeSet、共享组和 Undo。
- 默认空 Physics 资源直接构造类型化数据，移除通过 JSON 中间值创建资源的逻辑；
  字段锁定检查复用 `entity_id()`，移除重复字段枚举分派。

本轮新增 6 项回归。验证范围聚焦 Physics SDK、Editor 集成及 UI/表单，未重跑
全工作区、原生截图/Timeline 验收、Python wheel 或 Release 性能基准。

最终验证：SDK Physics **23 项通过、1 项性能测试默认忽略**；editor-core Physics
**19 项通过**（1 项单元、18 项集成）；Editor Physics UI/表单 **12 项通过**。
共 54 项通过，新增回归包含在这些数字内。两个仓库的格式与 diff 检查通过。

```sh
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-sdk \
  --test integration physics_
cargo test --locked -p editor-core --lib --test integration physics
cargo test --locked -p kasane-editor --lib physics
```

## C4：首批转换原语（实施中，未完成 C4）

2026 年 10 月 21 日：首批代码同时落在相邻 `kasane-2d` 与本仓库。当前交付
Mesh 网格转换、参数范围联合修改及静态仿射 Mesh 改父级；不将本节视为完整 C4 验收。

- **Mesh 零轴与网格转换**：core 的 `MeshGridEdit` / `MeshGridPlan` 支持从目标
  创建零轴 Binding、增加轴、重排轴、插入/移动/删除采样值，以及显式切片或重采样
  后删轴。零轴必须没有 Keyform，求值、Properties 和既有顶点工具读取 Mesh 的基础字段。
  激活首轴从最新基础字段生成网格；扩轴复制全部旧网格，不使用当前预览形态。
  删末轴同时写回选定形态的坐标、颜色、透明度与 draw order，清空网格。
  插值使用 Mesh 各通道的数值规则，缺省 draw order 按目标的有效值处理。
  插点只声明保留底层分段多线性插值；运行时在新点附近的精度吸附可能改变结果，
  `GridGuarantee::InterpolantPreserved` 明确区分此项，未声称完整参数域数值恒等。
- **参数范围联合修改**：`ParameterRangeEdit` / `ParameterRangePlan` 提供保留关键值
  或按旧范围到新范围的递增仿射映射。规划覆盖所有 Mesh/Scene/Glue 轴、共享
  BlendShape KeyTable 和 Constraint；执行联合更新关键值与 Mesh/Scene Keyform 坐标。
  形态、权重、基准下标和 Offscreen 映射顺序保持；共享表/约束全部使用者进入影响与
  锁定检查。通过“先改参数”和“先改网格”均失败、合法最终组合一次成功的回归。
  映射后浮点值合并、越界或任一使用者锁定时整体拒绝，不放宽普通 setter 的检查。
- **改父级首批矩阵**：`ReparentEdit` / `ReparentPlan` 为 Mesh 与 Rotation/Warp
  提供 `KeepLocalData`，检查循环、源/目标/后代锁定，多选按文档投影顺序规范化并排除
  已选祖先包含的后代。无绑定或带普通 MeshBinding 的 Mesh，在根与无驱动、可逆的
  静态 Rotation 链之间支持 `PreserveEditState` / `PreserveParameterDomain`。
  使用与求值相同的旋转、反射与父级角度/缩放继承规则，转换基础坐标和每个形态。
  计划报告模型单位下的容差、采样最大误差与包含多线性累加舍入的保守误差预算；
  保证仅覆盖 Glue 之前的几何。颜色/透明度继承、启用/参数域可见性及 Glue/有状态
  动画明确列入未保证效果。奇异父级、带驱动父级、非线性 Warp、BlendShape Mesh
  或变形器子树的保持策略返回具体的未证明原因；其 KeepLocalData 仍可用。
- **预算、事务与发布**：SDK `BindingAuthoringEdit` / `BindingEditPlan` 和
  `plan_binding_authoring` / `execute_binding_plan`、`EditSession::binding_authoring`
  使用同一 core 规划、候选与历史。计划绑定 SDK Version 与 core 快照身份；在已被
  其他写入改变的候选内复用旧计划也拒绝。完整笛卡尔积分配前检查溢出、组合数、
  载荷/临时字节及历史预算；执行再次检查当前 SDK 会话预算。放弃计划、不变提交和
  拒绝均不发布；no-op 保留 redo。范围规划只保存关键值写片段，不复制形态载荷。
  ChangeSet 增加 `BindingGrid` / `DeformationChildren` 集合和 `BindingReferenceChange`，记录 Mesh/Transform
  父级与绑定目标引用，Undo/Redo 交换正确方向。现有普通 setter、字段 patch、顶点
  写入与事务补齐对应的持久化锁定检查；无标记文档直接走空状态快路径。
- **Editor 与格式**：editor-core 的 `plan_binding_authoring` /
  `execute_binding_authoring` 合成候选/Timeline 会话检查并消费同一回执；创建建议选择
  Binding，不自动激活 Motion/Expression。原生格式沿用既有轴/形态字段，保存重开保留
  零轴和全部基础字段；本次未变更 wire schema。MOC3 严格返回
  `ZERO_AXIS_BINDING_NOT_REPRESENTABLE`；CMO3 默认拒绝并报告 `ZERO_AXIS_BINDING`，
  显式允许损失后省略零轴 Binding、导出目标基础形态，保留诊断。

当前 SDK 统一预算默认值为 **65,536 采样组合、64 MiB 转换临时字节、256 MiB 历史
估算字节**，由会话 `authoring_limits` / `set_authoring_limits` 配置；不是面板配置。
这些是本批有限的初始边界，已有精确边界/溢出回归，尚未按真实模型 Release 基准定型。
字节值是保守载荷估算（含 SDK 候选及旧 checkpoint），不是 RSS/峰值内存实测。

首批交付时尚缺稳定轴/采样身份、待配置轴、Scene/Glue 与共享 BlendShape 转换，
以及 Python 入口。上述能力已由下一节继续交付；改父级扩展、完整转换影响地址、
可中断长转换及真实模型容量/性能验收仍未完成。

本批验收（均从 `kasane-editor` 目录执行）：

```sh
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-core
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-sdk \
  --test binding_authoring --test scene_authoring --test effects_authoring --test mesh_editing
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-moc3 --lib \
  zero_axis_binding_is_rejected_before_runtime_encoding
cargo test --locked -p editor-core --test integration binding_authoring
cargo test --locked -p editor-core --test integration mesh
cargo check --locked -p kasane-editor
cargo fmt --all --check --manifest-path ../kasane-2d/Cargo.toml
cargo fmt --all --check
```

结果：core **69 项**、SDK 相关专项 **31 项**（Binding 18、Scene 5、Effects 2、
Mesh 6）、MOC3 零轴边界 **1 项**、Editor 新专项 **2 项**与既有 Mesh 筛选
**32 项**通过，合计 **135 项**且上述分组互不重复。两仓库格式与 diff 检查通过；
Editor 编译通过。新增回归共 18 项，已包含在上述数量内。

按本批影响范围使用相关包和测试筛选，未执行完整工作区脚本、Python wheel、
Metal 截图或真实模型 Release 基准；这些结果不构成完整 C4/C6 验收。


## C4：稳定轴/采样身份、Scene/Glue 与共享 BlendShape 转换

2026 年 10 月 22 日续：本节完成这三个领域切片，C4 整体仍在实施中。

- **身份与持久化**：`BindingAxis.id` 与带 ID 的 `BindingSample` 是创作源数据；
  Mesh/Scene/Glue Keyform 引用采样 ID 组合，DeltaKeyform 引用共享表采样 ID，
  `base_sample_id` 取代原生基准下标，Constraint 将 ID、值和权重保存在同一条目。
  数值 keys、基准下标与平行权重数组仅为编译缓存，不再写入原生文件。
  数据接口 schema 由 6 升为 **7**，directory-project 由 14 升为 **15**，旧版明确拒绝。
  原生加载严格检查 ID 与采样引用，不能把空 ID 当作外部导入请求而补发身份。
  数值导入/创建适配器可以生成初始 ID；重放未改变的数值替换复用原 ID，
  正式移动采样使用稳定 ID 转换。全局唯一性覆盖新子身份，保存重开保留原身份。
- **SDK 生命周期与字段**：`BindingAxis` / `BindingSample` 纳入 SDK 对象键、
  句柄、incarnation、反向引用与 ChangeSet；删除后 Undo 恢复数据也不能复活旧句柄。
  `BindingFieldAddress` 捕获子项、拥有者和单调上下文 epoch，字段补丁复用领域规划。
  纯轴重排保持稳定地址；驱动、采样域或参数域变化使旧地址失效，Undo 不回退 epoch。
  整表替换、共享转换和重基准化也检查轴/采样与普通网格的持久化锁，删除拥有内容同步
  清理其 editor flags。独立候选和借用候选使用同一有限预算，提交独立候选时再次检查
  最新会话预算；失败、不变和放弃候选都不产生历史。
- **普通网格**：`BindingGridEdit` / `BindingGridPlan` 与 `GridOperation` 覆盖
  Mesh、Warp、Rotation、Part 和 Glue 的零轴、待配置轴、首轴激活、扩轴、重排、
  按 ID 插/移/删点、显式切片/重采样降维，以及保留数值或映射数值的驱动更换。
  待配置轴保存自身身份与采样，但不参与有效笛卡尔积；清空参数需要显式降维策略。
  首轴激活读取最新基础数据，删末轴把选择结果写回目标，不复活旧形态副本。
  Warp 联合处理兼容的 Bezier 控制器；Rotation 插值遇混合反射明确拒绝。
  Part 联合重排/重采样 Offscreen 映射，保留重用槽、neutral sentinel 与静态外观，
  新建插值槽按领域默认颜色生成，不按旧数组位置猜测对应关系。
  旧 `MeshGridEdit` 入口在规划时翻译到同一转换器。
- **共享 BlendShape**：`BlendAuthoringEdit` / `BlendAuthoringPlan` 支持待配置表、
  约束和目标绑定，表可从空状态逐点配置；插值、移动、删除、显式替代基准、重基准化、
  驱动更换/范围映射及绑定换表同步验证全部使用者。移动保留采样与 Delta 的对应；
  约束值/权重按条目 ID 配对。换表在等价采样域上重映射，其他情况需显式接受 reset，
  不允许隐式丢弃增量。插点、移动和兼容换表保留已有 Delta 的完整载荷，包括
  可选通道及不参与执行的基准条目；新建和重基准化才重构形态。重基准化覆盖 Mesh、Warp、Rotation、Part、Glue、Offscreen，
  同时补偿基础字段和普通网格，多个共享使用者的补偿累计到同一目标。
  Offscreen 会物化需要补偿的中性槽。非单位约束、不可保持的 optional color 选择、
  非整数/跨零离散绘制顺序、不可表达的 Bezier 位移等返回具体拒绝原因；
  不通过只改表或只改目标的中间状态发布。计划报告受影响拥有者、预算与删除/reset 损失。
- **运行与格式**：有效轴按紧凑数值缓存求值；零轴绑定不贡献独立形态。
  未配置共享表或约束禁用整条 BlendBinding，保留源数据，不能将缺约束当作权重 1。
  运行热路径复用 PreparedEvaluation 的有效选择，不逐帧复制/过滤文档。
  SDK `plan_available_binding_export` 列出零轴、待配置轴/表/约束及间接禁用的使用者，
  接受时检查版本；CMO3 默认拒绝，显式接受后导出相同可用投影并保留诊断。
  MOC3 在编码前明确拒绝不可表达状态。外部重导入生成新的内部身份。
- **Editor 与 Python**：editor-core 投影稳定轴/采样对象与关系，字段规划复用 SDK；
  Properties 的 MeshKeyform 地址改为采样 ID 组合，现有预览与控制点工具只读取有效轴，
  待配置 BlendBinding 不阻止基础控制点编辑。Python 的 `Edit.binding_authoring`、
  `Session.plan_binding_authoring` / `execute_binding_plan`、`binding_source`、
  `binding_field_address` / `plan_binding_field_patch` 走相同类型化 Rust 规划与候选；
  source 查询返回原生轴/采样身份，计划可查询 affected IDs、预算与共享转换损失，
  普通 tuple 查询继续提供数值视图。

本节不把数值采样回归视为完整数学误差证明。**C4 剩余工作**：第 8.1 节完整改父级
能力/支持矩阵及其场景前置能力；每类转换统一的保留/生成/删除/重映射稳定地址与
参数域保证报告；可协作中断的长转换；真实模型容量、临时峰值与 Release 性能验收。
当前默认预算仍为首批有限边界，尚未用真实模型基准定型。C5 的完整对象生命周期与
后续 UI 操作统一不包含在本节交付中。

本节验收命令（从 `kasane-editor` 执行）：

```sh
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked --no-fail-fast \
  -p kasane-core -p kasane-sdk -p kasane-project -p kasane-cmo3
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-sdk \
  --test binding_authoring --test mesh_editing
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-moc3 \
  --lib --test import_tests -- blendshape_glue zero_axis_binding_is_rejected_before_runtime_encoding
cargo test --locked -p editor-core --test integration -- \
  binding_authoring properties canvas_objects panels
cargo check --locked -p kasane-editor
cargo fmt --all --check --manifest-path ../kasane-2d/Cargo.toml
cargo fmt --all --check
```

Python 在相邻 `kasane-2d` 执行 `uv run --locked maturin build --manifest-path
modules/kasane-python/Cargo.toml --out target/c4-wheels`，安装 CPU wheel 到独立
`target/c4-python` 环境，再运行 `modules/kasane-python/tests/test_cpu.py`。
新增两项 Python 回归覆盖稳定 source、字段地址、待配置轴激活、共享表与过期计划。
Pyright 与 Git HEAD 源文件逐条对比均为既有 **31 errors / 3 warnings**，本轮无新增诊断；
这不是 Python 全量静态类型检查通过的声明。

结果：四包命令共 **433 项通过、2 项既有忽略**（core 69、SDK 234、project 89、
CMO3 41）。最终身份检查又补充了拓扑替换路径的同事务删除/重建回归，Binding 专项
最终 **39 项**与 Mesh 专项 **6 项**通过；这些专项与四包结果重叠，不累加。
MOC3 零轴边界 **1 项**及 BlendShape/Glue/Offscreen 往返 **6 项**、Editor 相关筛选
**85 项**、最终 CPU wheel **57 项**通过。两仓库格式/diff 检查及 Editor 编译通过。
未运行完整 workspace 脚本、Metal 截图、真实模型 Release 延迟或内存峰值基准；
本节结果不代表完整 C4/C6 验收。

## C4：未提交实现 review 修正

2026 年 10 月 22 日：对两个工作区的 C4 实现做针对性 review，修复以下问题：

- **共享 Offscreen 颜色转换**：运行时 Offscreen multiply 以 1 为中性值，
  转换器现在先转为以 0 为中性的增量，再执行插值／重基准化，写回时恢复其表示。
  避免插点和改基准后颜色跳变；回归比较完整参数区间上的 opacity、multiply、screen，
  并检查 Undo/Redo 和原生保存往返。
- **锁定与句柄生命周期**：Offscreen 绑定沿所属 Part 检查锁，直接替换 Offscreen、
  BlendBinding 改目标及 Mesh/Warp 拓扑替换也验证被修改的拥有内容。
  Warp 联合替换纳入 SDK 的删除身份跟踪，同事务删除／恢复采样不会复活旧句柄。
- **待配置状态与改父级**：静态 Rotation 的待配置普通轴／BlendBinding 不再误判为
  有效驱动；仿射转换的插值误差预算仅计算有效轴，待配置轴不会虚增维度。
- **属性地址**：多轴 MeshKeyform 地址必须覆盖完整采样组合，重复同一个 SampleId
  不能代替缺少的维度；合法的无序采样组合仍可解析。
- **Editor 入口与 Warp 工具**：Binding 创作改用 Modeling 上下文检查，并在执行时
  再次校验。控制点编辑保留零轴／待配置 Warp 绑定及其身份，通过所选 Keyform 槽位
  决定写入基础形态还是有效网格；不再因遗漏绑定而提交失败。
- **网格转换实现**：Mesh/Warp/Rotation 插值读取借用的源 Keyform，位置和 Bezier
  辅助函数读取切片／引用，避免为读取标量字段反复复制整份形态。保持原有领域验证。

以上修正不改变上一节列出的 C4 剩余交付范围。

本轮新增 **8 项回归**（SDK 6 项、Editor 2 项）。针对性验证命令：

```sh
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-sdk \
  --test binding_authoring --test mesh_editing --test effects_authoring --test scene_authoring
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-core \
  --test offscreen_tests --test typed_scene_tests --test keyform_tests --test blendshape_glue_tests
cargo test --locked -p editor-core --test integration -- binding_authoring properties canvas_objects panels
cargo check --locked -p kasane-editor
```

最终结果：SDK **58 项**、core **28 项**、Editor **87 项**均通过；Editor 编译及
两仓库格式／diff 检查通过。编译仍报告 vendor GPUI/Cocoa 的既有警告及 `block`
未来兼容性提示。本轮未运行全 workspace、原生截图、Python wheel 或真实模型
Release 基准；不将本次 review 视为完整 C4/C6 验收。

## C4：改父级保证、统一转换报告与协作取消（已完成）

2026 年 10 月 22 日续：本节完成 C4 剩余接口和回归。最终代码的原生 UI、Python
重建与两个真实模型 Release 测量均已通过，**C4 标记完成**。改父级支持边界见下表；
它遵守方案 8.1.2 允许的明确拒绝，未把任意非线性逆变换或任意变形器子树的自动保持
列为已支持。C5 的全对象生命周期和 C6 的全领域验收仍待实施。

- **显式状态和模型单位容差**：`ReparentEdit.context` 捕获原生参数值、编辑时间与
  有限参数域；每个相关驱动必须落在显式区间或给出固定值。快照使用原生参数求值、
  位于 Glue 之前；Motion/Physics 先由调用方解析成冻结参数，不读取提交时的预览。
  Rotation 普通网格选择、重复参数归一化、Blend 约束与运行时 clamp 采用相同规则。
  位置容差为 `absolute + relative × S`，默认 `1e-5`、`1e-6`，
  `S = max(1, 原受影响几何 AABB 对角线)`。尺度从几何本身求得，不依赖纹理渲染。
  计划记录实际样本误差、保守插值／仿射误差界、参考尺度和完整捕获上下文；不放宽容差。
- **Mesh BlendShape 转换**：静态或已冻结 Rotation 坐标系之间，基础位置与普通
  Keyform 按仿射映射转换，所有 Mesh Delta 的位置只应用线性部分，避免重复加平移。
  未参与执行的基准 Delta 和待配置绑定也保留原 ID 与完整其他载荷。多个使用者共用
  KeyTable 的引用保持；颜色、透明度和离散属性不混入位置变换。
- **轻量能力和完整影响**：core/SDK 的 `reparent_support` 与执行共用关系、锁定和
  驱动检查。`RequiresConversionPlan` 明确表示仍须数值／容量验证，查询不构造几何、
  笛卡尔积或运行帧；动态奇异采样留给重型计划判断。移动 Transform 时，影响集合
  包含完整后代及其普通／Blend 绑定，实际父级写入仍只修改规范化后的选中根。
  Opacity、multiply、screen、enabled、域内可见性、Glue 和有状态输入逐项报告未保证范围。
- **统一报告**：每个 core 转换计划与 SDK `BindingEditPlan.report()` 都提供同一
  `ConversionReport`：保留身份、写入／重验地址、生成、删除、位置重映射、源／结果域、
  网格形态重建来源与权重、损失以及类型化网格／参数范围／改父级保证。
  `preserved` 指身份保留，不表示字段数值未变；`rewritten` 描述写入／重验集合。
  普通 Keyform 使用规范化采样 ID 组合，Offscreen 使用绑定计划基线的专用拓扑槽位。
  空 Binding／共享表也有资源 Object 地址；兼容换表明确报告旧／新 SampleId 的形态移交。
  报告包含共享表全部使用者、重基准补偿及 Offscreen 映射；不复制形态载荷来生成地址。
  参数范围保留数值时也列出所有依赖及锁定检查，不只列实际重写关键值的使用者。
- **取消和原子提交**：`ConversionControl` 克隆共享永久取消信号、工作检查点和可选
  进度回调。网格、共享 Delta 转换、降维补偿、地址报告及改父级按有界工作单元检查。
  core 计划、SDK 计划与借用／独立候选共享此有效期；
  `EditSession::binding_authoring_with_control` 可基于已包含前序写入的候选规划，
  将取消信号保持到最终发布；预先取消时 SDK 不创建候选，
  提交在发布前最后检查。失败、取消和 no-op 均不发布事件、不清 redo；取消不能被 Undo
  撤销。回调进度是工作检查点，不作为完成百分比。
- **预算和性能**：完整笛卡尔积及 Delta 分配前，预算同时覆盖旧／新地址报告、
  共享使用者、插值选择与候选／历史成本。预算公式修正为
  `temporary = 6 × retained + 2 × payload`，保留
  `history = 2 × retained + payload`。默认边界固定为 65,536 组合、64 MiB 临时估算、
  256 MiB 历史估算，SDK 会话统一配置。真实模型 Release 探针比较计划估算与实际堆分配
  增量，并验证部分转换取消后的版本和历史；结果见[容量与性能记录](../benchmarks/2026-10-22-binding-authoring.md)。
- **Editor / Python**：Editor 提供可取消规划和轻量改父级查询，继续合成 Modeling
  与候选占用条件；取消的计划不重建现有对象投影。Python 增加独立取消 token、
  `reparent_support` 和 `report_json`，规划释放 GIL，其他线程取消不争用 session 锁。
  保存和外部格式继续复用之前的 C4 映射；本轮仅改操作契约，原生 wire schema 不变。

| 改父级关系 | KeepLocalData | PreserveEditState | PreserveParameterDomain |
| --- | --- | --- | --- |
| 无绑定／普通 MeshBinding 的 Mesh，根与静态可逆 Rotation 链之间 | 支持 | 支持，显式快照 | 支持，显式参数域与仿射误差界 |
| 同上，另有 Mesh BlendShape | 支持 | 支持，基础形态和全部位置增量联合转换 | 支持，固定坐标转换及增量线性等价 |
| 新旧 Rotation 祖先有驱动，全部祖先驱动在本次保证中固定 | 支持 | 支持，使用捕获状态 | 支持其他声明维度；固定驱动值写入保证 |
| 祖先驱动在保证域中变化，未证明同一个仿射转换 | 支持 | 可捕获一个状态 | `PARAMETER_DOMAIN_UNPROVEN` |
| Rotation / Warp 子树，包括普通／Blend 绑定及其后代 | 支持，统一验证循环和全部锁定 | `SUBTREE_CONVERSION_UNPROVEN` | `SUBTREE_CONVERSION_UNPROVEN` |
| Warp 祖先无已证明的唯一非线性逆变换 | 支持 | `NONLINEAR_PARENT_UNPROVEN` | 同左 |
| 奇异父级、不可满足的误差界、缺失／非法快照输入 | 结构合法时支持局部保留 | 具体原因拒绝，整体不变 | 具体原因拒绝，整体不变 |

最后两行的拒绝不改变所请求策略，不自动退回 KeepLocalData。所有几何保证都止于
Glue 之前；域抽样仅为回归，证明依据为固定仿射坐标变换与多线性／增量运算等价，
并加上记录在计划中的浮点误差预算。用户不能将这些结果呈现成“全部外观不变”。

本节验收使用受影响测试筛选，未执行整个 workspace、Metal 截图、Release Python wheel
或 C6 的属性链／Timeline 配对测量。新增 SDK 11 项、Editor 1 项、Python 2 项回归。

```sh
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-sdk \
  --test integration -- binding_authoring:: scene_authoring:: effects_authoring:: \
  mesh_editing:: handles:: history_budget::
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-core \
  --lib --test keyform_tests --test typed_scene_tests --test blendshape_glue_tests --test offscreen_tests
cargo test --locked -p editor-core --test integration -- binding_authoring properties canvas_objects panels
./scripts/test.sh ui -- headless_tests::mesh_properties headless_tests::parameters \
  headless_tests::hierarchy headless_tests::ui_performance
# 在相邻 kasane-2d 执行，构建当前 Debug extension，运行 CPU 与 inspection：
./tools/test.sh python
cargo check --locked -p kasane-editor
```

最终验证：SDK 筛选 **74 项通过**，包括事务内受控规划、16 轴中的单采样
维度不虚增插值误差界、空资源身份报告、共享表跨采样身份移交及分配前报告预算拒绝。
原生 UI 筛选 **44 项通过**；当前源码重建的 Python Debug extension 通过
**59 项 CPU + 4 项 inspection**。此前已通过的 core 相关 **38 项**、Editor 核心相关
**88 项**和 Editor 编译检查亦已通过。各轮专项覆盖重叠，
不累计数字。

Release 探针重新构建并对 Mao、Shirousagi 各测三个普通绑定，每个八轮规划／提交／Undo。
两模型在默认预算内完成，实测规划和提交峰值均低于计划临时估算；部分规划取消后版本和
历史保持。Mao 提交峰值约 **8.53 MB**，临时估算 **16.00–16.02 MB**；Shirousagi
提交峰值约 **0.66 MB**，临时估算 **1.48–1.50 MB**。完整载荷、组合数、历史成本及
耗时见[最终容量与性能记录](../benchmarks/2026-10-22-binding-authoring.md)。

此前被 SIGTERM 中断的重建不计为通过，本轮使用最终源码完成以上验收。验收启动时，
本地 `sccache` 旧服务接收请求却不启动编译器，独立 Rust 探针 15 秒超时；重启服务后，
编译、缓存写入和命中均恢复。随后 SDK、Python 和 UI 均按原 `sccache` 配置通过，
未禁用项目缓存、清空缓存或修改测试断言。两仓库最终格式与 diff 检查通过。
编译日志仍有既有 GPUI/Cocoa 警告与 `block` 未来兼容提示；未修改第三方代码。


## C4：剩余实现 review 修正

2026 年 10 月 22 日：复核上一节新增的改父级保证、统一报告、预算与取消实现，
修正以下问题；改动保留在两个工作区，未提交。

- **父级采样预算**：结构／驱动预检不再提前求值带驱动的 Rotation。新旧祖先普通
  网格的采样组合与选择数组成本纳入计划预算，先完成容量和报告预算检查，再求值父级。
  即使移动的是无绑定 Mesh，父级采样也不能绕过组合数限制；执行旧计划时仍重验
  当前会话预算。场景遍历增加取消检查点。
- **参数范围影响与锁**：保留采样值时，也报告普通绑定的 Mesh／Transform／Part
  目标。Part 普通网格关联的 Offscreen 纳入影响、地址报告和锁定检查，防止范围修改
  绕过锁改变其采样行为。合并重复锁检查与目标收集逻辑。
- **共享转换参数域**：BlendBinding 的 Keyform 地址附带所属表的驱动、采样值与
  基准信息，换表报告可查询该绑定的源／结果参数域；参数范围转换和重基准化复用
  同一地址元数据。
- **报告构建**：按 BTreeMap 的拥有者有序区间清理旧地址，避免每加入一个共享
  使用者都扫描此前全部地址。保留原有身份、重映射和取消语义。
- **Python 类型接口**：同步取消 token、可取消规划、轻量改父级查询和 `report_json`
  的原生 stub，补充 facade 的 token 与计划返回类型。

新增一个 SDK 回归，验证无绑定 Mesh 的驱动父级预算和旧计划执行检查；扩充现有
参数范围测试以覆盖两种策略的完整影响与 Offscreen 锁，以及换表参数域报告。
验证结果：SDK 首轮相关筛选 **75 项通过**，后续报告修正后 Binding 专项 **57 项通过**
（与前者重叠）；core **38 项**、Editor 核心 **88 项**、当前源码重建的 Python Debug
扩展 **59 项 CPU + 4 项 inspection** 均通过。

Python 首次 `./tools/test.sh python -- -k binding` 的 CPU 筛选通过 8 项，但 inspection
筛选没有匹配测试，脚本因此返回非零；随后直接用该脚本刚重建的
`target/python-tests/develop-venv/bin/python -m unittest discover` 分别运行 `test_cpu.py`
和 `test_inspection.py`，得到上述完整通过结果。

本轮未重跑整个 workspace、原生截图或真实模型 Release 基准；上一节的性能数字
仍是此前测量结果，不作为本轮修正后的重新测量。

最终 `cargo check --locked -p kasane-editor`、两仓库格式与 diff 检查通过；
Python facade／stub 语法检查通过。编译仍有既有第三方警告及 `block` 未来兼容提示。


## C5 首批：Pose 根资源、Group、Entry 与 Link

2026 年 10 月 22 日：开始 C5，先交付 Pose 纵向切片。**本节不代表 C5 全领域完成**；
参数与组织、场景生命周期、专用拓扑内容、行为引用集合及资源依赖继续按下表推进。

- **原生身份与待配置**：`PoseGroup { id, name, entries }`、`PoseEntry.id`、
  `PoseLink { id, target }` 均持久化；Part 引用区分 `Unspecified`、`Resolved` 和
  导入 `Unresolved`。空组、未配置成员与 Link 可保存。UUID、跨类型全局身份冲突、
  无效 fade、组内重复主 Part、成员内重复 Link 和已解析的悬空 Part 引用仍拒绝。
  根资源使用 Arc，组、成员和 Link 数组使用 SharedVec；普通字段写入按不可变查找
  得到目标槽位后仅分离其所在分支，不因遍历先分离无关成员数组。
- **领域操作**：core `PoseEdit` / `PosePlan` 提供创建单例、创建组／成员／Link、
  类型化字段补丁、集合移动／重排、复制、删除与指定接收组的解散。
  创建首组可同时创建根资源，一条历史恢复全部；多选按原生集合顺序执行并排除已选祖先
  包含的后代。复制为全部拥有内容生成新 ID，Part 引用保持共享。
  跨组移动和解散验证最终重复约束，失败不发布中间结果。单例不复制或移入集合；
  组不嵌套；没有根成员容器，解散必须给出接收组。
- **SDK 生命周期**：`plan_pose_edit` / `execute_pose_plan` 和
  `EditSession::edit_pose` 使用同一规则；集合查询与集合／实体能力查询区分不支持和
  当前不可用。`PoseFieldAddress` 捕获实体、拥有者句柄、字段基线与拥有者 epoch。
  重排后仍定位同一条目；跨拥有者往返及 Undo 不复活旧地址；删除／Undo 或同候选
  删除再恢复不复活旧句柄。整资源替换、pose3 导入和普通 `erase_object` 同样检查
  锁定及身份退休，不能绕过领域入口。失败、取消候选和 no-op 沿用 SDK 历史规则。
- **运行投影与导出**：`pose_projection` 同源过滤未配置／未解析主 Part 的整项及
  不可用 Link，保留空组、原生顺序、稳定来源 ID、紧凑槽位及反向来源。
  主成员失效导致的有效 Link 省略有 `OwnerUnavailable` 原因链。
  MotionPreview 与 MotionEditingPreview 共用此投影，重置时构建并在 seek checkpoint
  间共享；逐帧不重新过滤或复制。过滤前的第一项不可用时，第一项有效成员仍正确初始化
  为可见状态。严格 pose3 导出拒绝待配置；`plan_available_pose_export` /
  `export_pose_plan` 列出直接和间接省略项，并在版本变化后要求重新接受。
  不透明内容保持既有 namespace／content hash 保护，不允许在部分导出时静默过滤。
- **发布与 Editor**：`ChangeSet` 增加 Pose 集合与方向明确的 `pose_references`。
  摘要借用源记录，报告直接替换／导入／规划／历史的实际变化，不反复按 ID 扫描层级。
  组名变化只刷新对应 Properties 载荷，复用关系记录和无关数组，不推进 SDK 求值版本或
  Editor 动画输入版本；Undo/Redo 同样处理。Editor 投影与原生 Pose Inspector 改用
  稳定 Group／Entry／Link 身份，未配置项有诊断；Document／Editor 适配仅调用 SDK，
  不自动激活 Motion 或 Expression。新菜单与表单交互继续属于后续 UI 阶段。
- **保存和 Python**：原生目录格式升至 **16**，明确拒绝 15 等旧实验版本，不提供迁移。
  pose3 只写外部 Part runtime ID，导入为内部拥有内容生成新 UUID，不泄露内部身份。
  Python `Edit.pose_authoring`、`Session.plan_pose_authoring` / `execute_pose_plan`
  经过相同 Rust 类型化命令；旧 `create_pose` 便捷方法生成新的原生记录结构。

C5 剩余覆盖核对：

| 范围 | 本轮状态及后续工作 |
| --- | --- |
| Pose 根资源／Group／Entry／Link | 本节首批交付；后续 C6 继续检查公共契约和性能 |
| Parameter／ParameterGroup／Part | 第二批组织与集合操作已交付，见下一节；其余场景生命周期与通用 UI 在后续切片覆盖 |
| Mesh／Warp／Rotation／Offscreen／Glue／DrawOrderGroup | C4 前置能力保留；完整生命周期、无纹理 Mesh、未配置 Glue 与绘制组织待补齐 |
| 顶点／UV／三角形／Warp 控制点／GlueVertexPair／Offscreen 映射 | C4 转换覆盖保留；其余专用增删、依赖重映射与稳定 Pair 身份待补齐 |
| ModelParameterGroup／ModelHitArea／CdiCombinedSet／其他引用列表 | 稳定身份、待配置成员、集合操作、运行过滤及格式诊断待补齐 |
| Texture／ImageAsset／Attachment／sound／UserData | C2 Attachment／sound 保留；空图像内容、异步句柄及跨遮罩／Offscreen 的依赖传播待补齐 |
| Document／Layout／ExportSettings／SourceCanvas／修复／不透明内容与编辑器状态边界 | 本切片遵守既有边界；全入口覆盖核对仍待完成 |

这些待办属于 C5 未完成范围，不能以“类型不支持”缩减目标。C6 的全领域验收、真实模型
性能复测及 UI 统一尚未开始。本轮新增 SDK **11 项**、Editor 核心 **3 项**、Python
**1 项**回归；以下命令采用受影响测试筛选：

```sh
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-sdk --test integration -- \
  pose_authoring:: handles:: editor_state:: document_operations:: history_budget::
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-project \
  --test pose_tests --test project_tests --test motion_tests --test cdi_tests
cargo test --manifest-path ../kasane-2d/Cargo.toml --locked -p kasane-animation \
  --test motion_preview --test motion_editing_preview
cargo test --locked -p editor-core --test integration -- pose_authoring:: panels:: animation::
./scripts/test.sh ui -- headless_tests::animation_panels
# 在相邻 kasane-2d 执行，重建当前 Debug extension：
./tools/test.sh python
```

最终验证：SDK 筛选 **29 项通过**（含 11 项新增 Pose 回归）；project 的 Pose／原生格式／
Motion／CDI 筛选 **61 项通过**；animation **26 项通过**；Editor 核心 **57 项通过**；
最终稳定行 ID 版本的原生动画面板 **10 项通过**；当前源码重建的 Python Debug extension
通过 **60 项 CPU + 4 项 inspection**。各组覆盖可能重叠，不合计数字。

默认 `cargo check --locked -p kasane-editor`、两仓库 `cargo fmt --all --check` 与
`git diff --check` 通过。编译仍报告既有 vendor GPUI/Cocoa 警告及 `block`
未来兼容提示；未修改第三方代码。

未运行完整 workspace、截图／真实模型 Release 或 C6 的综合性能验收。代码保留在
`kasane-editor` 与相邻 `kasane-2d` 工作区，未提交。


## C5 第二批：Parameter、ParameterGroup 与 Part 组织集合

2026 年 10 月 22 日：先提交上一批 Pose（SDK `df89e91`、Editor `5d917c0`），
随后完成本节。本节交付组织与集合操作；其余 C5 领域及 C6 验收继续保留在待办中。
原生格式维持 **16**，没有新增平行的分组存储。

- **单一原生规则**：core `OrganizationEdit` / `OrganizationPlan` 和 SDK
  `plan_organization_edit` / `execute_organization_plan`、
  `EditSession::edit_organization` 共用创建 Parameter／空 Group／Part、Group 字段补丁、
  多选移动／重排、复制、删除内容及解散。集合为 `Parameters { group_id }`、
  `ParameterGroups { parent_id }` 和 `PartMembers { part_id }`，空拥有者表示根集合。
  放置锚点是稳定 sibling ID；多选按源顺序执行，排除已选祖先覆盖的后代，禁止循环。
  Group 可任意合法嵌套；未解析 CDI 行保留原位置、扩展内容和 runtime ID，
  Parameter 组归属继续由 DisplayInfo 唯一表示。旧 membership setter 保留导入表作用域，
  不因更改归属而改写其他参数的手动顺序。
- **复制与依赖闭包**：Parameter／Group 复制为拥有内容生成新 UUID 和不冲突 runtime ID，
  外部 Motion、Expression、Physics 和 Binding 用户继续引用原 Parameter。
  带未解析 CDI 参数行的组复制返回当前不可用
  `UNRESOLVED_PARAMETER_COPY_CONFLICT`，要求先修复引用；不会为导入行虚构运行参数。
  移动、解散仍保留这些行。Part 复制沿组织子树，包括 Mesh、Transform、Offscreen 和
  拥有的普通／Scene／Blend Binding；内部 Part、变形父级、遮罩、Axis、Sample 与
  keyform 坐标重映射，外部 Parameter、纹理、共享 Blend table／constraint 保持共享。
  Offscreen 保留 keyform 索引。绘制成员只能属于一个组，源组内的外部成员不转移到副本，
  通过 `OrganizationEffects.external_drawing_members` 报告。独立场景对象复制仍属于
  后续场景生命周期切片，能力返回当前不可用 `USE_SCENE_AUTHORING`。
- **删除与解散**：内容删除先计算组织拥有闭包和 Binding 子对象，再检查外部引用；
  外部 Glue、变形后代、行为资源及绘制组中的外部成员不随组织删除，返回 `OBJECT_REFERENCED` 和
  referrers。组织内成员的 CDI 行、编辑器状态及 Binding 子对象状态同时清理。
  Group 解散提升直接参数（含未解析行）及子组，保留成员 ID 和同级位置。
  Part 解散提升直接组织成员，只在可证明绘制序列保持时展开绘制组；支持相同静态 rank
  以及静态排序后展开结果一致的情况。Part 禁用、存在有效 Part Binding／Blend 驱动、
  或为 Offscreen owner 时返回 `PART_DISSOLVE_STATE_UNREPRESENTABLE`；
  绘制展开无法证明时返回 `PART_DISSOLVE_DRAW_ORDER_UNPROVEN`。
  无运行贡献的待配置 Binding 可随 Part 删除。失败不发布部分变化。
- **组织与绘制／变形分离**：Part 移动保留独立绘制组与所有变形父级。
  组织带来的继承可见性和合成归属变化通过 effects 显式报告。
  同级 palette 重排不实体化绘制组、不改变求值；原地移动使用集合相对顺序判 no-op，
  不生成历史或清空 redo。旧全资源 replace 仍保留完整字段替换语义；交互组织移动经过
  上述 typed intent。Deformer 锁只保护其本身及显式改父级受影响对象，组织 Part 锁
  保护成员，与既有 Editor 锁语义一致；不沿变形父级把全部后代变为锁定。
- **能力、生命周期与发布**：SDK 提供集合／实体能力和 `OrganizationAddress`，
  地址包含实体／拥有者句柄与单调拥有者 epoch；移动往返及 Undo 不恢复旧归属地址，
  删除／Undo 或同候选删除再创建不恢复旧句柄。计划绑定 SDK version 和 native snapshot。
  源子树、目标容器及拥有的 Binding／Axis／Sample 锁均生效；旧 DisplayInfo、palette
  replacement、Part／Mesh／Transform／Offscreen 改归属入口也检查相关锁。
  `ChangeSet` 增加精确 Organization collection 和方向明确的 ownership reference，
  Undo 反转引用；参数／组名、同级排序与归属元数据不推进 SDK 求值版本。
- **缓存与 Editor**：native 组织索引缓存集合、拥有者、源顺序和子树关系，
  label-only 编辑与 Undo 复用关系缓存；结构／归属／排序变化使其失效。
  hover Move 规划只保留父级、顺序及元数据增量，不复制 Mesh 几何和 keyform 网格；
  复杂复制／删除的最终结构检查使用隔离候选。
  Editor 既有参数管理、Group 表单、拖放排序与 Properties 改归属接到同一 SDK 规则，
  移除重复组移动／解散算法。Document／Editor 提供统一 plan／execute／capabilities
  适配和建议选择／定位结果，不自动改变活动 Motion／Expression 或播放时钟。
  纯标签更新只刷新目标 Properties 和 Document 元数据，复用无关关系；
  复制／删除场景内容及改归属正确使 Canvas 祖先、几何与拾取缓存失效。
- **Python**：`Edit.organization_authoring`、`Session.plan_organization_authoring` /
  `execute_organization_plan` 使用同一 Rust 命令，计划暴露 affected／copied IDs 和 effects。
  命令示例及解散／未解析复制边界已补到相邻 SDK 的 Python API 文档。

本轮新增 SDK **18 项**、Editor 集成 **5 项**、core 缓存 **1 项**、Python **2 项**回归。
验证命令与结果如下（各筛选覆盖可能重叠，不合计）：

```sh
# 相邻 kasane-2d
cargo test --locked -p kasane-core --lib organization::index
cargo test --locked -p kasane-sdk --test integration -- \
  organization_authoring:: cdi_authoring:: hierarchy_order:: scene_authoring:: \
  editor_state:: binding_authoring:: mesh_editing:: effects_authoring::
./tools/test.sh python
# kasane-editor
cargo test --locked -p editor-core --test integration -- \
  organization_authoring:: sorting:: parameter_management:: parameter_creation:: properties:: locks::
./scripts/test.sh ui -- headless_tests::parameter_groups headless_tests::parameter_management \
  headless_tests::parameter_creation headless_tests::parameters headless_tests::hierarchy \
  headless_tests::sorting headless_tests::ui_performance
```

SDK 筛选 **96 项通过**，core 组织缓存 **1 项通过**，Editor 集成筛选 **68 项通过**；
当前源码重建的 Python Debug extension **62 项 CPU + 4 项 inspection 通过**。
原生 UI 受影响筛选 **59 项通过**。该回归发现并修复新建参数的全局列表插入位置，
保持既有创建后定位／Undo 行为；另有集成回归覆盖根级 Part 排序与未归组场景对象共存，
确保 After 锚点保留在对应 palette；修正后原生排序补查 **13 项通过**。

最终 `cargo check --locked -p kasane-editor`、两仓库 `cargo fmt --all --check` 与
`git diff --check` 通过。仍有既有 vendor GPUI/Cocoa 警告及 `block` 未来兼容提示。

本轮未运行完整 workspace、真实模型 Release 或截图验收；本节不提供新的性能测量数字。
本轮代码暂留两仓库工作区，尚未提交。
