# SDK 局部编辑的增量发布方案

日期：2026-10-23。状态：**设计草案，尚未实施本文的变化记录机制**。

目标是在保留现有 authoring 契约的前提下，让常用局部编辑减少提交阶段的全模型扫描。
首期只记录被修改的对象，用于缩小变化摘要和身份判断的范围；保留当前结构验证、
Document 存储和 checkpoint 历史。后续局部验证单独验收，不以建设通用依赖图为前置条件。

## 1. 依据和当前状态

源码核对基于 editor `61371c0`、kasane-2d `c7f1651` 及两个工作区当时的未提交修改。
未提交文件仍在变化，实施前应重新核对相应函数；以下路径和符号比行号更可靠。

关联文档：

- [发布优化与复测](../benchmarks/2026-10-23-2d-publication-fix.md)：当前保守优化的实现与数字。
- [回归定位](../benchmarks/2026-10-23-ui-authoring-diagnosis.md)：提交、ObjectIndex 和原生栈的证据。
- [原始 UI 对照](../benchmarks/2026-10-23-ui-comparison.md)：最初指定的版本基线。
- [SDK authoring 契约](../../../kasane-2d/modules/kasane-sdk/AUTHORING.md)：身份、锁、事务、历史及通知边界。
- [C6 验收](unified-object-authoring-c6-acceptance.md)：既有跨领域回归入口。

### 已有实现及其边界

| 项目 | 写本文时的代码状态 | 本方案处理方式 |
| --- | --- | --- |
| 未变 Binding/Motion 验证复用 | `validate_structure_since`、snapshot 验证凭据及 checkpoint 携带凭据已在工作区 | 保留，作为首期验证路径 |
| 身份集合共享 | SDK `object_key_cache`、`Arc<HashSet<ObjectKey>>` 已在工作区 | 保留，在记录完整时减少证明身份未变所需的扫描 |
| 按领域裁剪摘要 | `Document::same_domain` 与 SDK `summarize_impl` 已在工作区 | 保留为回退和差分参照 |
| Opacity 预览局部索引更新 | 最新 `preview_opacity` 已调用 `refresh_property_index`；finish/cancel 也已有相应调整 | 作为独立 UI 修复，不重复安排实现；本文未复跑其测试或原生基准 |
| ScenePlan 身份保持判断 | 最新 `ScenePlan::preserves_identities` 已被 `edit_scene`、`apply_scene_plan` 使用 | 作为独立优化保留；不能把该保证扩大解释为完整的内容变化记录 |

旧诊断报告中 Opacity 每帧重建 ObjectIndex、SceneEdit 固定执行身份扫描的描述对应修复前。
不能用它们描述最新工作区仍然必然存在的问题，也不能仅凭源码改动宣称原生性能已恢复。

### 为什么仍考虑下一步

当前修复已使原生 Name／Opacity 文本／Enabled 提交 CPU P95 接近原始基线，
独立 SDK 的 Name／Opacity `commit_to` 中位数约为 3.55／3.57 ms。
修复报告 JSON 中 Opacity commit 的两轮插桩显示：

| 阶段 | 每次提交 inclusive 均值 | 解释 |
| --- | --- | --- |
| `core.validate_structure` | 1.468–1.490 ms | 已跳过部分未变绑定和动画，仍做其他结构检查 |
| `sdk.summarize` | 0.382–0.398 ms | 含领域比较和变化汇总 |
| `core.checkpoint_clone` | 0.366–0.387 ms | 旧内容快照克隆 |
| `core.publish_candidate` | 0.524–0.539 ms | 含内部交换与索引工作 |

这些是插桩归因，不能与原生线程 CPU P95 相减，父子阶段也不能重复相加。
首期主要针对约 0.4 ms 摘要阶段及部分身份／内容比较，不能据此承诺原生 10 ms 提交降至 1 ms。
方案的价值必须由后续成对测量确认；若只有复杂度增加而收益不稳定，应停在保守实现。

## 2. 当前真实调用链

### SDK 发布

[`EditSession::commit_to`](../../../kasane-2d/modules/kasane-sdk/src/edit.rs) 当前依次完成：

1. 检查 session version、预算及取消状态。
2. `candidate.validate_structure_since(session.document())`。
3. 根据 `erased_keys` 找到删除后重建的身份；比较完整内容判断 changed。
4. 取 before keys；通过 `same_authoring_identities` 决定是否复用 after keys。
5. `changes::summarize` 比较实际领域数据，生成通知。
6. 克隆 checkpoint、估算历史预算、再次检查取消。
7. 发布 candidate，刷新 incarnation／缓存，安装历史并发布 receipt。

`same_authoring_identities` 在验证和 commit 的身份判断中都会使用；它仍遍历多个集合。
`same_domain` 的部分领域也包含完整记录或跨域比较。这些借用比较减少分配，但不是常数成本。

### 三条必须分别测量的属性入口

| 用户操作 | Editor／SDK 路径 | 首期所需记录 |
| --- | --- | --- |
| Name 文本 | `document/properties.rs::apply_properties` → `edit_scene(SceneEdit::Patch(Label))` → Core plan apply | 场景对象记录；先覆盖 Mesh Label |
| 直接 Opacity 拖动 | `document/edits.rs::preview_opacity` → `set_mesh_opacity` → 手势结束时提交 | Mesh 基础外观；逐帧 ObjectIndex 更新单独验收 |
| Opacity 文本／绑定形态编辑 | 属性路径可能替换 MeshBinding keyform | MeshBinding 记录及目标 Mesh 的派生摘要影响 |

独立探针 `replace_mesh → commit_to` 只能覆盖其中一部分；不能代替 editor 的 Label plan
或 keyform 路径。Enabled 也要沿实际属性分派测量，不能用基础 Mesh 属性探针替代所有情况。

## 3. 首期范围与明确不做的内容

首期引入一个内部、短生命周期的“被修改记录集合”，只对已审计的路径启用。

- 首批支持 Mesh 的名称、基础外观、Enabled；覆盖 typed setter、允许的 `replace_mesh`
  和 `SceneEdit::Patch(Label)` 入口。
- 整个 replacement 的差异若超出该范围，回退现有发布流程。
- MeshBinding keyform payload 的身份保持替换作为下一批接入，不能让首批误声称
  已完成 Opacity 文本路径优化。
- 同一事务包含未接入的操作时，整笔发布回退；仍使用现有原子性、锁与错误行为。

本轮不引入 Salsa、不建设通用依赖图、不替换 HashMap／SharedVec、不改 checkpoint 历史，
不增加新项目格式或迁移，不重写 Python API，也不将验证移到提交成功之后。
不削弱几何、参数范围、绑定坐标、锁或身份检查来取得性能数字。

## 4. 最小数据设计

以下名称是拟议内部类型，不是现有公共 API。Core 不依赖 SDK 的 `ObjectKey`／`ChangeSet`。

```rust
enum TrackedRecord {
    Mesh(String),
    MeshBinding(String), // 第二批启用
}

enum TrackingState {
    Complete {
        baseline: SnapshotIdentity,
        current: SnapshotIdentity,
        touched: Set<TrackedRecord>,
    },
    Unknown,
}
```

首批不记录每次输入的 old/new 完整对象，也不建立逐字段版本矩阵。原始 session document
已经持有基线，candidate 已经持有最终值；提交时按 touched IDs 借用两边记录比较即可。
同一手势修改同一对象 100 次，集合仍只有一个对象。存储成本随不同 touched 对象数增长。

`baseline` 必须对应 session 的准确快照身份，`current` 必须对应候选的最终快照。
数字 revision 相同不能证明两条分支内容相同。SDK 现有 version 检查继续保留。
Proof 的构造和更新留在 Core 内部，对 SDK 暴露只读查询；缺少 proof 只会降低性能。

### 什么才算 Complete

Complete 只表示“自基线以来的全部持久化写入都已覆盖”，不直接表示内容有效、身份不变
或无需重算求值。以下证据仍分别建立：

- 记录是否发生净变化：比较 touched 记录的基线值与最终值。
- 是否超出首期字段范围：比较目标记录，发现关系、拓扑或拥有子项变化就回退。
- 是否影响身份：首期仅允许现有对象的身份保持更新；创建、删除、替换子身份继续回退。
- 是否影响验证／求值／展示：分别沿现有规则判断，不能合并成一个 Metadata 标志。

记录对象 ID 的收集与最终 `receipt.object_ids` 保持分离。后者有既有通知语义，不能直接
用 touched 集合覆盖；现有摘要对声明类别、被触及对象、派生目标的处理必须保持一致。

## 5. 如何接入现有写入代码

### 5.1 默认路径自动回退

[`Document::changed`](../../../kasane-2d/modules/kasane-core/src/document.rs) 是很多修改的
共同出口，但目前是在内容写入后调用，而且还会清理 editor-state 子项。因此仅在这里
按 `ChangeKind` 收集 IDs 不足以建立完整记录。

建议保留 `changed` 作为未追踪路径：它推进 snapshot 时将跟踪状态降为 Unknown。
抽取共享的 revision／缓存失效逻辑，增加仅供已审计安装入口调用的 tracked 变更出口。
这个出口接收实际写入记录，并覆盖其附带持久化变化；无法覆盖时同样降为 Unknown。
已有编辑器状态清理若产生实际删除，首期直接回退，不隐式丢掉这部分变化。

tracked 写入必须在进入变更前取得与当前 snapshot 匹配的状态，完成成功安装后再推进
跟踪终点。未知操作一旦降级，后续 rename 等操作不能把整笔事务重新升级为 Complete。

所有绕开 `changed` 的持久化安装／恢复路径也需审计。实施检查应搜索 `content` 写入、
`restore_from`、checkpoint exchange、batch build 和各类 Delta install；搜索结果只是
审计入口，不能将“调用了 changed”当作完整性证明。

### 5.2 先接入窄而完整的入口

| 文件／符号 | 拟议变更 | 必须注意 |
| --- | --- | --- |
| Core `document/meshes.rs`：`rename_mesh`、`set_mesh_opacity`、`replace_mesh` | 为允许的旧→新记录变化生成 touched Mesh | replacement 比较规范化后的实际记录；其他字段变化降级 |
| Core `document/property.rs`：`patch_property`、`preview_property` | 接入 Mesh 标量分支；失败恢复时同步恢复跟踪状态 | `preview_property` 目前会恢复内容、snapshot、revision；必须增加 tracker 的同一回滚边界 |
| Core `document/scene.rs`、`scene/fields.rs`：plan apply／field install | Label patch 安装时记录目标 | 只读 plan 不记录写入；实际应用时记录；保留 stale、锁、取消检查 |
| Core `document/bindings.rs`：`replace_binding` | 第二批记录 MeshBinding payload 修改 | Axis/Sample ID、目标 Mesh、轴定义变化继续回退；保留 canonicalize |
| Core `document/organization/scene.rs`：`Delta::install` | 首期默认降级；后续按安装的记录逐步接入 | 会修改 map、order、关系、editor-state、索引，不能只记 `affected_ids` |

通用计划的 `preserves_identities()` 只证明其定义范围内的身份保证，不证明它没有改动
未记录的关系／顺序。不能直接拿它为整个计划生成 Complete proof。

### 5.3 分支和历史

- 跟踪候选从准确 session 基线建立；建议提供 SDK 使用的专用 tracked fork，普通
  `Document::clone` 和内部 planner clone 默认 Unknown。保留当前 clone 丢弃验证凭据的行为。
- 未接入的 clone／replace／restore 进入候选后降级，不借用其旧 tracker。
- checkpoint 保存内容和现有验证凭据，不保存上一笔事务的 touched 集合。
- Undo/Redo 继续现有 exchange 与 receipt 反转；下一次编辑从恢复后的快照重新开始记录。
- 资源重定位修改 checkpoint 时继续使验证凭据失效；不修改现有历史预算定义。

## 6. SDK 如何消费记录

在 [`edit.rs::commit_to`](../../../kasane-2d/modules/kasane-sdk/src/edit.rs) 中增加一个
Core 只读查询，取得“针对本 session 基线的完整 touched 记录”，失败则执行现有代码。
不要在 SDK 根据操作名称补写 Core 没有证明的影响范围。

首期流程：

1. 继续 version／预算／取消检查和当前 `validate_structure_since`。
2. 核对跟踪基线、终点、记录种类和允许字段范围；超出范围回退。
3. 只比较 touched 记录，生成内部净差异；在证明整笔事务身份不变时复用 base keys。
4. 从 touched 记录及明确派生目标生成局部摘要；保留 categories、object_ids、modified
   的现有规则和最终排序／去重。
5. 历史预算、取消检查、candidate 发布、incarnation 更新、事件队列均沿用现有流程。

初期可继续保留 SDK／Core 的 `same_content` 防线，暂不消除所有整文档比较；它们在现有
插桩中的成本较小。只有影子对照证明净差异与整文档判断一致后，才单独替换这一步。

### 摘要不能只过滤最终输出

`changes/organization.rs::collections` 会构造全模型集合；在它完成后删掉无关输出，
不会消除工作。应在调度前选定要运行的领域，并让目标领域只处理必要记录／集合。

首批 Mesh 标签／外观／Enabled 的组织、资源、Motion、Physics、Pose 数据没有变化，
可依据完整记录跳过这些摘要。Scene 继续处理现有分类归一化，不能遗漏 Topology 的移除规则。
第二批 MeshBinding payload 需要把其目标 Mesh 加入 content 摘要候选，即使 Mesh 存储本身
没有改动，也要保留 `content_changes`、相关 collection／epoch 的既有通知。

不直接把 `updated MeshBinding` 映射为唯一的 `modified MeshBinding`；必须沿现有
`changes/binding.rs`、`changes/content.rs` 的依赖传播生成相同结果。

### no-op 和身份生命周期

`1 → 0.5 → 1` 的最终净差异为空时，沿用现有 no-op 行为：不增加历史、不清 redo、不发通知。
事务中若另一个对象实际改变，仍按现有完整摘要的语义处理先前被触及对象，不能擅自删除通知。

删除已有 X 后用相同 ID 和相同内容重建 X 必须退休旧 handle。首期任何删除／创建都回退，
继续使用 `erased_keys`／`identity_keys`。不能用最终内容相同取消身份更新。
以后接入生命周期记录时，必须分别测试“新建后删除”和“基线对象删除后重建”。

## 7. 局部验证何时推进

它是后续独立阶段，首期不会因为记录了 touched IDs 就跳过结构检查。

只有摘要／身份阶段通过差分及性能验收，且结构验证仍是主要可优化成本时，才做以下工作：

1. 从 Mesh Label／基础 Appearance／Enabled 的允许字段差异开始，逐项说明哪些现有
   不变量可能变化；要求基线已验证有效。
2. 对 changed MeshBinding 继续完整 canonicalize，复用其他绑定与 Motion 的已有证明。
3. Parameter 范围、顶点身份、父关系、共享 Blend 表等继续现有广泛验证，不在首轮建立
   通用反向依赖图。
4. 某类高频结构操作后续确有瓶颈，再增加对应的反向索引和依赖闭包测试。

当前 `references_to()` 存在跨集合扫描，不能直接反复调用它并声称验证成本已局部化。
无有效基线、未知写入或未覆盖的依赖始终回退；无效导入内容的修复能力要保留。
公开 `validate_structure()` 仍执行完整验证，不能更改成使用 fast path 的接口。

## 8. 实施拆分与退出条件

| 阶段 | 可独立审阅的交付 | 退出条件 |
| --- | --- | --- |
| A：记录与影子对照 | Core tracker、窄入口接入、clone／恢复／失败降级；发布仍走现有流程 | covered 事务的真实差异全部被记录；未知操作必然回退；锁与原子性不变 |
| B：Mesh 局部摘要／身份判断 | SDK 消费 proof、局部 summary、工作次数计数 | 快慢结果一致；hot path 不为摘要／身份重新枚举全模型；plain 无稳定回归 |
| C：Binding payload 接入 | keyform 实际写入记录、目标 Mesh 派生摘要 | 原生 Opacity 文本路径命中；同级绑定不被重复处理；旧地址／历史语义一致 |
| D：有限局部验证 | Label／Appearance 等明确规则的验证选择 | 与完整验证接受／拒绝及诊断一致；工作次数随实际影响增长 |

A–C 不要求 D 完成。每阶段都保存回退实现；普通 Release 不运行双份摘要或双份验证。
若记录维护成本抵消收益，先停在 B 的已验证范围，不扩展到所有 authoring 域。
持久化 map、分块存储、逐对象版本图和增量历史大小估算不属于这四阶段的交付。

## 9. 正确性验收矩阵

新增测试尽量放入现有模块，不增加新的 SDK 集成二进制。下列“新增”是待实施要求。

| 场景 | 应有结果 | 建议位置 |
| --- | --- | --- |
| `rename_mesh`、`replace_mesh`、Scene Label plan 改同一对象 | 最终文档和摘要一致，covered 入口命中局部路径 | SDK `changes.rs` 单元测试、`tests/scene_lifecycle.rs` |
| Mesh 名称与透明度在同一事务修改 | 两类变化均报告，只有一条历史与事件 | SDK `tests/authoring_acceptance.rs` |
| covered 操作与 Motion／结构操作混用，两种顺序 | 整笔回退且不漏变化 | SDK `tests/authoring_acceptance.rs` |
| 多次覆盖同一字段、改回基线、再修改另一对象 | no-op／redo／通知行为与原路径相同 | SDK `tests/handles.rs`、摘要差分测试 |
| 删除重建同 ID、轴／sample 删除后重建 | 旧 handle／address 永不复活，首期命中回退 | SDK 既有 `handles`、`binding_authoring::stable_domains` |
| 数字 revision 相同的不同分支 | proof 不可跨分支使用 | Core `document::structure::tests` 或新增 tracker 单元测试 |
| clone 后直接写内容，checkpoint 交换、资源重定位 | 不携带错误的完整覆盖／验证保证 | Core tracker／checkpoint 测试 |
| preview 求值失败后恢复，再合法更新 | tracker、内容、revision、preview 同步恢复 | Core `preview_property` 相关测试 |
| 无效 keyform、参数范围缩小、目标几何变化 | 拒绝结果与完整检查一致，不跳过依赖检查 | Core structure、SDK binding tests |
| 单次编辑末尾失败、取消、stale plan、历史预算不足 | 无发布、无新历史、保留 redo | SDK 既有 authoring／history_budget 测试 |
| Undo/Redo 后继续编辑，持有旧读取快照 | 新 tracker 基线正确，旧快照内容不变 | SDK binding／scene lifecycle 测试 |
| keyform opacity 改动，另有同级 binding 和 Motion | summary 包含目标 Mesh 内容变化；未改对象不被扫描式汇总 | SDK `tests/binding_authoring.rs` |

扩展现有 `pruned_summaries_match_full_summaries_across_domains_and_history`：保留不使用
tracker 的完整摘要参照，逐字段比较整个序列化 ChangeSet，而不只比较 `changed` 或数量。
差分调用应使用真实事务收集的 categories／object_ids，不能只从最终合并的 ChangeKind
重建输入，否则会漏掉混合事务差异。完整结构验证必须在独立快照上运行，避免测试自身
填充的缓存掩盖 fast path 缺陷。

使用固定种子的合法编辑序列组合 covered／fallback、no-op、历史往返和失败，不追求
另建庞大的随机框架。测试目标是漏记和错误复用，而不是重复 setter 实现。

## 10. 性能验收与复现

### 工作次数先于耗时

保留现有 `sdk.keys_in`、`core.validate_motion`、canonicalize、`sdk.summarize` span，
新增少量计数：touched 记录数、局部摘要读取记录数、fallback 原因及次数。
这些仅在 `bench-instrumentation`／测试配置开启；不能在 UI 热路径输出逐对象日志。

对 1／10／100 倍无关 MotionKey、BindingSample 的合法合成模型，修改同一个固定大小 Mesh：

- B/C 阶段：局部摘要读取数和身份枚举次数应保持不变；当前验证／checkpoint 仍可能增长。
- D 阶段：允许字段修改的验证读取数也应不随无关对象增长。
- 不要求高扇出参数范围变更保持常数成本；它应回退或处理真实受影响对象。
- 保留旧 snapshot 的读者情形必须测，不能只测唯一持有者。

### 三个性能边界分开报告

1. 独立 SDK：同一探针、Mao ArtMesh215、Release、5 次预热＋40 个样本，保留
   `replace_mesh` 路径并增加 Label SceneEdit／MeshBinding payload 路径；Undo 在计时外。
2. Editor 属性链：复用 `scripts/property-chain-benchmark.py` 与
   `crates/editor-core/examples/property_chain_benchmark.rs`，分别报告 begin/update/finish、
   commit、preview；测试 retained index，不能仅统计 SDK 调用。
3. 原生 UI：Name、Opacity 文本、Enabled，5 次预热＋96 次输入、120 Hz 请求；
   直接 Opacity 拖动的 draw P95、呈现 Hz 和输入延迟另表，不与提交 CPU 混算。

每个变体先构建并保存二进制，测量时不并发编译或运行测试。记录两个仓库 HEAD／补丁哈希、
模型及二进制哈希、Cargo.lock、硬件和输入参数。至少三轮 A/B、B/A 交错，按进程报告样本
及分布；耗时以 plain 版本为准，probe 只用于归因。保留逐请求 CSV／JSONL 和失败记录。

比较对象必须包括“当前保守发布补丁”与“本阶段新增变化”，不能只拿早期未优化版本作对照。
需要判定相对最初 `279f569 / 01db56a` 的恢复程度时，重新交错运行原始基线；跨批次的
0–7% 差异不足以单独判为回归。耗时必须超过进程间波动且正确性门禁通过，才宣称收益。

### 测试入口

以下是实施后的验证入口，本文仅新增文档，没有执行这些 Rust／原生测试。

```sh
# cwd: ../kasane-2d，迭代时按本阶段选择，避免重复跑完整套件
cargo test --locked -p kasane-core --lib document::structure::tests
cargo test --locked -p kasane-sdk --features bench-instrumentation --lib changes::tests
cargo test --locked -p kasane-sdk --features bench-instrumentation --test integration scene_lifecycle::
cargo test --locked -p kasane-sdk --features bench-instrumentation --test integration binding_authoring::
cargo test --locked -p kasane-sdk --test integration authoring_acceptance::
cargo test --locked -p kasane-sdk --test integration handles::
cargo test --locked -p kasane-sdk --test integration history_budget::

# cwd: kasane-editor
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --lib document::properties::tests
```

新增 tracker 单元测试使用实际模块 filter。B/C 改动公共提交链，合并前再跑一次 SDK 普通
integration 目标；不默认扩大到双仓库全部测试、GPU、wheel 或 alpha-mesh 压力套件。
文档涉及的真实模型探针和原生测试需在模型／Metal 可用的环境执行，缺失时明确标为未测。

## 11. 实施前需要固定的决定

- Core 与 SDK 的分层：tracker 的类型与 proof 在 Core；对外 receipt、incarnation、事件在 SDK。
- 第一批完整覆盖仅承诺允许字段的现有 Mesh 更新；其他操作保守回退，不以“看起来是 scalar”判断。
- 先采用记录级 touched 集合，基线／最终值借用比较；不存无限增长的输入日志。
- 变化摘要与现有完整算法保持一致；任何有意调整通知语义需单独评审，不能混入性能补丁。
- 默认 Unknown，只有经过审计的安装入口可维持 Complete；失败恢复、clone、导入和历史边界有明确规则。
- 当前存储和 checkpoint 不动，性能目标限定到本阶段实际减少的工作。

## 12. 可参考的实践

[tldraw RecordsDiff](https://tldraw.dev/reference/store/RecordsDiff) 采用 added／updated
（before、after）／removed 描述记录变化；[Store API](https://tldraw.dev/reference/store/Store)
提供提取修改记录和派生缓存的接口。本方案借鉴“写入时确定变化范围”，在 kasane 中先只存
touched IDs，因为基线与 candidate 已经同时存在。tldraw 的生命周期合并规则不作为
kasane handle／incarnation 语义的依据。

[Blender dependency graph](https://developer.blender.org/docs/features/core/depsgraph/)
主要用于场景求值更新；[rust-analyzer 架构](https://rust-analyzer.github.io/book/contributing/architecture.html)
使用增量计算复用分析结果。它们可供后续依赖设计参考，不构成本轮引入通用依赖框架的理由。
