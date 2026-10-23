# SDK 局部编辑的增量发布方案

日期：2026-10-23。状态：**A 已完成；B／C／D 已通过差分／工作次数验收，原生 UI 三轮复测已完成；独立 SDK／Editor 链耗时复测待执行**。

目标是在保留现有 authoring 契约的前提下，让常用局部编辑减少提交阶段的全模型扫描。
首期只记录被修改的对象，用于缩小变化摘要和身份判断的范围；保留当前结构验证、
Document 存储和 checkpoint 历史。后续局部验证单独验收，不以建设通用依赖图为前置条件。

### 2026-10-23 实施记录：阶段 A

- Core 新增 `document/tracking.rs`，通过 `fork_tracked_candidate` 建立准确快照基线，
  `tracked_records_since` 只读查询完整记录。默认及普通 clone／fork 为 Unknown；
  checkpoint、restore、组织 Delta 安装和未知写入不携带旧事务的覆盖保证。
- 覆盖现有 Mesh 的名称、Appearance、Enabled：typed setter、允许字段的 `replace_mesh`、
  无结构 Delta 的 Scene field patch，以及不改变 draw order 的基础 Mesh `PropertyPatch`。
  Binding payload／keyform、几何、关系、顺序和混合未覆盖操作整笔回退。
- 记录只保存不同 touched Mesh IDs。重复修改和改回基线仍保留 touched 集合；
  SDK 继续以原有完整内容判断处理 no-op、redo、历史、handle 与通知。
- 预览失败同时恢复 tracker、snapshot、revision 和标量；结构型预览清理的导入遗留
  editor-state 也在失败时恢复。普通 Appearance 预览不为此克隆 editor-state map。
- 影子对照仅存在于测试：恢复 touched records 后比较完整持久化内容，使用真实事务的
  `categories`／`object_ids` 对照完整序列化 ChangeSet，并验证固定种子序列、历史往返、
  retained snapshot、混合回退、同 ID 重建和失败恢复。Release 不运行双份摘要或验证。
- `replace_mesh` 与 typed rename 原有声明 categories 不同；各自保留原通知语义，
  分别与完整算法对照。另补齐了同 ID 的 Part／DrawOrderGroup 在摘要排序中的 kind
  次序，避免独立 HashSet 枚举导致序列化对照随机失败。

已通过：Core `document::` 单元测试 13 项；SDK `changes::` 单元测试 9 项；
SDK 普通 integration 321 项（2 项原有忽略）；Editor 属性链单元测试 3 项。
阶段 A 交付时 SDK 提交仍走原验证、身份比较及摘要路径；未启用局部发布，也未运行真实模型
或原生性能基准。后续阶段 B 的接入和验收见下文。

### 2026-10-23 实施记录：阶段 B

- SDK `changes/mesh.rs::MeshPublication` 消费 Core 的准确快照记录，仅接受既有 Mesh
  名称、Appearance、Enabled 的完整覆盖；保留实际 `object_ids` 和累计 categories。
  未知覆盖、退休身份或无法证明的通知范围分别记录 fallback 原因，继续原摘要算法。
- 局部摘要只比较 touched Mesh 的三个允许字段，复用 Scene 字段分类及摘要排序／去重；
  不调度 Organization、Binding、Motion、Physics、Pose、Resources、References、Content
  全模型汇总。不把 Appearance 变化误报成 MeshTopology 的 content_changes。
- 完整范围保证所有 authoring identities 不变时复用原 `Arc<HashSet<ObjectKey>>`。
  Core `same_authoring_identities` 也消费同一窄范围证明，使现有验证路径中的身份判断
  无需再扫描全模型；结构验证规则、绑定依赖检查和公开完整验证入口均保持原有规则。
- 新增只在 `bench-instrumentation` 编译的工作计数；`Measurement.calls` 记录单位数，
  计数条目的两个时间字段为零，计数本身不读时钟、不进入 span 栈。普通构建不含这些调用，
  也不运行双份摘要／验证；`same_content`、checkpoint 和历史大小估算继续原流程。
- 完整序列化 ChangeSet 与独立完整摘要逐字段对照。覆盖七种入口、绑定中的固定三角形、
  RGB 外观、跨 Mesh 字段、改回基线但仍需通知、no-op／redo、历史往返、保留旧快照、
  固定种子混合序列、删除重建、未知操作双顺序及通知范围回退；独立快照执行完整验证。

工作次数验收的每档还包含恒定的目标 Mesh 两个 BindingSample；无关 Mesh／Part 的数量
按 1／10／100 同步增长。下表每格对七种入口及 retained／unique 两种读者策略均成立：

| 倍率 | 无关 MotionKey | 无关 BindingSample | touched 记录 | Core proof 记录读取 | 通知范围读取 | 局部摘要记录读取 | 全局身份扫描／key 枚举 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 8 | 4 | 1 | 4 | 2 | 2 | 0／0 |
| 10 | 80 | 40 | 1 | 4 | 2 | 2 | 0／0 |
| 100 | 800 | 400 | 1 | 4 | 2 | 2 | 0／0 |

Core proof 在验证中的身份判断和 SDK 发布选择各查询一次，每次读取基线／最终 Mesh 各一条。
两 Mesh 正对照的摘要读取数为 4、proof 读取数为 8。混合创建操作的反对照确实运行旧摘要并
枚举一次 keys，避免仅测试“新计数器没有递增”。同 ID 的 Mesh handle 在局部提交后继续有效，
缓存 key set 的 Arc 地址保持相同；删除重建仍走回退并永久退休旧 handle。

验证：Core `document::` 及新增身份证明测试、benchmark 计数器测试均通过；SDK `changes::`
在普通构建通过 11 项、开启插桩通过 13 项；普通 SDK integration 321 项通过（2 项原有忽略），
Editor 属性链 3 项通过。七入口工作次数矩阵共 42 组，复现命令如下，原始输出保存在
`../kasane-2d/target/sdk-incremental-publication/mesh-work-counts.txt`：

```sh
# cwd: ../kasane-2d
cargo test --locked -p kasane-sdk --features bench-instrumentation --lib changes::mesh_tests::mesh_publication_work_is_independent -- --nocapture
```

本次完成 B 的实现、差分和工作次数验收；尚未执行第 10 节规定的 plain Release 成对耗时、
Mao 探针或原生 UI 复测，因此不宣称耗时收益，也未把 B 的“plain 无稳定回归”退出项判为通过。
B 交付时 Binding payload／keyform 的 Opacity 文本路径仍回退；阶段 C 的接入见下文。

### 2026-10-23 实施记录：阶段 C

- Core 新增 `TrackedRecord::MeshBinding`。`replace_binding`、`set_mesh_keyform` 在原有
  canonicalize 成功后比较实际安装记录；`PropertyPatch`／`preview_property` 的 keyform
  分支在写入前记录 Binding。仅支持 positions、Appearance、draw order 的 payload 变化。
  轴完整定义、Axis／Sample ID、target Mesh、keyform 数量和规范化后的坐标必须保持一致；
  其他变化整笔回退，不把最终身份相同当成生命周期未变化的证明。
- 通用 tracked 安装出口沿用默认 Unknown 和 editor-state 清理检查。普通 clone／恢复／
  checkpoint 仍丢弃记录；预览失败同时恢复内容、snapshot、revision、tracker 和原预览帧。
  原有锁、参数域、坐标、几何长度检查及提交验证保留；没有启用阶段 D 的局部验证。
- SDK 局部摘要同时接受 Mesh 与 MeshBinding 记录，只比较 touched Binding 的最终
  keyforms；净变化向其目标 Mesh 产生 `MeshTopology`、`topology=false` 的 content_changes，
  推进内容 epoch，继承既有 Content／EvaluationInputs 分类。目标 Mesh 本身不必有存储写入。
  净变化为空时不产生派生 content_changes，但若其他记录改变，仍保留事务原有通知对象。
- 不把 updated Binding 当成唯一 modified 对象：保留 MeshBinding／目标 Mesh、原始
  object_ids、累计 categories、Binding 参与时的 Topology 保留规则，以及最终排序／去重。
  轴、sample、BindingGrid、关系及未改同级 Binding 不做扫描式摘要，也不伪造集合变更。
  完整摘要和局部摘要共用字段、内容分类 helper；Release 不运行双份摘要或验证。
- 真实 Editor 属性链新增验收：绑定形态下的 Opacity 文本提交和连续标量预览均命中局部
  发布，SDK 摘要读取 2 条 Binding 记录并派生 1 个 Mesh；保留旧 ObjectIndex，当前索引
  与完整重建一致且未增加构建次数。此为 CPU 属性链测试，未运行原生窗口／Metal 耗时基准。

差分使用真实事务的 categories／object_ids 和独立快照完整验证，逐字段比较序列化 ChangeSet。
覆盖七条入口、RGB／位置／draw order、规范化输入顺序、Mesh 与 Binding 混合、额外通知、
净 no-op 与 redo、64 步固定种子序列、未知 Motion 操作双顺序、失败预览、晚期错误和预算拒绝。
内容地址在 payload 编辑及历史往返后不复活；未变 sample 上下文和 handles 保持有效。
轴／sample 删除重建即使最终全文相同也回退，旧 handles 和地址永久失效。

同级未改 Binding 数、无关 samples 和 MotionKey 按 1／10／100 增长；目标 Binding 始终为
固定三角形的两个 keyforms。七条入口 × retained／unique 读者策略 × 三档规模，共 42 组：

| 倍率 | 同级未改 Binding | 无关 BindingSample | 无关 MotionKey | touched Binding | Core proof 记录读取 | SDK Binding 摘要读取 | 派生 Mesh 数 | 身份扫描／key 枚举 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 1 | 4 | 8 | 1 | 4 | 2 | 1 | 0／0 |
| 10 | 10 | 40 | 80 | 1 | 4 | 2 | 1 | 0／0 |
| 100 | 100 | 400 | 800 | 1 | 4 | 2 | 1 | 0／0 |

Core proof 的 4 次记录读取来自两次基线／最终查询；SDK 摘要只读取 touched Binding 的
基线／最终各一条，单独的通知范围查找共 6 次。每笔提交的 canonicalize 次数保持原入口行为：
replacement／setter 共 2 次（写入、提交各一次），标量 patch／preview 为提交时 1 次；
未改同级绑定不重复 canonicalize，未改 Motion 不重新验证。两 Binding 正对照读取 4 条摘要
记录并派生 2 个排序后的 Mesh。SDK key set 继续共享同一 Arc。

验证通过：Core `document::` 18 项；SDK `changes::` 普通构建 17 项、插桩构建 21 项；
SDK 普通 integration 321 项（2 项原有忽略）；Editor 属性链普通／插桩构建各 4 项。
工作次数原始输出保存在 `../kasane-2d/target/sdk-incremental-publication/binding-work-counts.txt`；
复现命令：

```sh
# cwd: ../kasane-2d
cargo test --locked -p kasane-sdk --features bench-instrumentation --lib changes::binding_payload_tests::binding_payload_work_is_independent -- --nocapture
# cwd: kasane-editor
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --features bench-instrumentation --lib document::properties::tests::bound_mesh_opacity
```

本次完成 C 的实现、差分及工作次数验收；plain Release 成对耗时、真实 Mao 探针和原生
UI 性能复测仍未执行，不据此宣称耗时收益。结构验证、全文内容比较、checkpoint 和历史
大小估算仍可能随模型规模增长；C 交付时 D 及存储／历史优化未启用。D 的后续接入见下文。

### 2026-10-23 实施记录：阶段 D

- Core `validate_structure_since` 仅在基线已有绑定准确 snapshot 的有效结构凭据，且整笔
  事务完整覆盖允许的既有 Mesh 字段时启用局部验证。先筛选 Mesh-only 记录，再复核
  tracking 两端快照与完整 Mesh 字段边界；不把 Complete 等同于结构有效。
- 现有不变量审计：Mesh name 是自由字符串，Enabled 是布尔字段；基础 Appearance 的
  opacity 必须有限且非负（允许大于 1），multiply／screen 六个通道必须有限且在 0..=1。
  这些字段不改变几何、拥有身份、runtime ID、纹理引用、父关系、遮罩、draw order，
  也不是 Binding／Motion／共享 Blend 表或其他结构验证规则的输入。
  局部路径只对 touched Mesh 复用原 `validate_appearance`，共用诊断排序及凭据更新出口。
- 无已验证基线、Unknown、分支不匹配、越界字段、Binding payload 或混合操作继续现有
  广泛验证。首次回退可验证并建立基线凭据，但本次仍走原路径；无效导入仍可修复。
  changed Binding 继续完整 canonicalize，未改 Binding／Motion 沿用现有复用规则；
  阶段 C 的 canonicalize 次数和 proof 读取次数保持原样。
- 公开 `validate_structure()` 始终完整验证。clone／restore 丢弃凭据或覆盖记录；
  checkpoint／Undo／Redo 只携带准确内容的有效凭据，下一笔事务重新记录 touched IDs。
  局部验证失败清除候选凭据，修复后重新验证；校验不改变内容、snapshot 或 revision。
- 差分在独立 clone 上调用公开完整验证，对比完整 `StructureIssue` 列表（object ID、
  code、message 和排序）。覆盖透明度与 RGB 边界、NaN／正负无穷、同一 Mesh 多项非法、
  多 Mesh 诊断排序、自由名称、Enabled、重复写／净 no-op、未验证／无效导入、修复、
  同 revision 分支、clone／checkpoint／restore、几何／参数域／关系／身份／共享表回退。
  同时保留已有固定种子序列、预览失败恢复、retained snapshot、历史往返与混合回退对照。
- 调整三处 Core 测试夹具：构造非法导入 flags 前先 clone 丢弃原有效凭据，避免测试直接
  改写已验证基线的 raw content 却保留旧凭据；生产验证规则没有为夹具放宽。
  Editor 的 Name／Enabled／未绑定 Opacity 属性链均断言命中局部验证；绑定 Opacity
  文本／预览仍命中局部发布，但明确断言保留广泛验证。

工作次数验收沿用固定目标三角形与两个目标 BindingSample；无关 Mesh／Part、Binding
和 MotionKey 按 1／10／100 同步增长。九条入口（原七条，加 Scene Appearance、基础
Appearance preview）× retained／unique 两种读者策略 × 三档规模，共 54 组：

| 倍率 | 无关 MotionKey | 无关 BindingSample | touched Mesh | 局部 Mesh 检查 | 广泛验证 | Binding canonicalize | Motion 验证 | Core proof 记录读取 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 8 | 4 | 1 | 1 | 0 | 0 | 0 | 4 |
| 10 | 80 | 40 | 1 | 1 | 0 | 0 | 0 | 4 |
| 100 | 800 | 400 | 1 | 1 | 0 | 0 | 0 | 4 |

每次提交的 Core proof 查询和 SDK 发布查询仍分别读取基线／最终记录各一条。局部验证
另读取最终 Mesh 一条并检查 Appearance；这些检查不会遍历无关记录。两 touched Mesh
正对照检查数为 2；混合创建反对照调用一次广泛验证。公开完整验证的正对照在三档规模
检查 Mesh 分别为 3／12／102，canonicalize Binding 分别为 2／11／101，Motion 各 1，
证明完整验证计数器确实反映工作量。原阶段 C 的 42 组仍通过且均明确回退广泛验证。

验证：Core `document::` 普通／插桩构建各 24 项；SDK `changes::` 普通 17 项、插桩 22 项；
SDK 普通 integration 321 项（2 项原有忽略）；Editor 属性链普通／插桩各 4 项。
复现命令与原始输出：

```sh
# cwd: ../kasane-2d
cargo test --locked -p kasane-core --features bench-instrumentation --lib document::
cargo test --locked -p kasane-sdk --features bench-instrumentation --lib changes:: -- --nocapture
cargo test --locked -p kasane-sdk --features bench-instrumentation --test integration
# cwd: kasane-editor
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --features bench-instrumentation --lib document::properties::tests
```

`../kasane-2d/target/sdk-incremental-publication/validation-work-counts.txt` 保存 54 组局部和
3 组公开完整验证计数；`stage-d-core.txt`、`stage-d-changes.txt`、`stage-d-integration.txt`
及对应 plain 日志保存测试结果。Editor 日志为 `target/stage-d-properties.txt` 和
`target/stage-d-plain-properties.txt`。完成 D 的有限局部验证、差分和工作次数验收；
上述 D 交付时，plain Release 成对耗时、真实 Mao／原生 UI 性能复测尚待执行；后续原生
复测见下文。全文内容比较、checkpoint、历史大小估算及回退验证仍可能随模型增长，
不据工作次数宣称整笔提交耗时收益。

### 2026-10-23 原生 UI 性能复测

已完成同一 editor `3f24179` 分别链接 SDK `4bb2fee` 保守发布、同一 HEAD 加 A–D 补丁的
plain Release 对照，并同批重跑最初 editor `279f569`／SDK `01db56a` 的保存二进制。
三轮交错顺序为保守→增量→原始、原始→增量→保守、增量→保守→原始；构建全部在测量前
完成，期间不编译或跑测试。Name／Opacity 文本／Enabled 各 5＋96 次、120 Hz；
直接 Opacity（同进程含 pan／zoom）各 30＋300 次、120 Hz。

| 指标 | 保守发布 | 增量发布 | 变化 |
| --- | ---: | ---: | ---: |
| Name 提交 CPU P95（ms） | 8.758 | 6.343 | -27.6% |
| Opacity 文本提交 CPU P95（ms） | 10.588 | 8.759 | -17.3% |
| Enabled 提交 CPU P95（ms） | 8.635 | 6.221 | -28.0% |
| 直接 Opacity UI draw P95（ms） | 3.492 | 3.450 | -1.2% |
| 直接 Opacity 新预览呈现 Hz | 119.202 | 119.999 | +0.7% |

上表是三个进程各自 P95／频率的中位数，没有混池。三个属性提交的每轮配对均下降；
Opacity 文本仍走 Binding 验证路径，其收益包含阶段 C，不能全部归于 D。直接拖动的
索引修复已在保守版本中，当前未见新的明显退化；输入到呈现 P95 轮间波动较大，中位数
40.699→40.754 ms，不能认定为稳定改善或回归。

36 个原生进程、10,692 个正式输入 ticks 全部通过完整性检查；属性均 96 次有效提交，
benchmark_done 为真、唯一 end、lost_events=0、issues 为空。增量版本的三轮直接 Opacity
共 900 个请求全部 presented；保守／原始版本的 accepted_not_composed 也在报告中保留。
报告脚本测试 27 项通过。

[详细报告与完整机器可读数据](../archive/benchmarks/2026-10-23-sdk-incremental-ui-retest.md)
保存原始基线重测、逐轮 CPU／GPU、输入延迟、请求终态、源码／模型／二进制／锁文件哈希。
本地原始数据和复现脚本位于 `target/ui-incremental-publication-20261023/`。
本次完成原生 UI 边界复测；独立 SDK 及 Editor property-chain 的 plain／probe 成对耗时
尚未重跑，不将这两个边界或全部性能退出条件标记为通过。

## 1. 依据和当前状态

源码核对基于 editor `61371c0`、kasane-2d `c7f1651` 及两个工作区当时的未提交修改。
未提交文件仍在变化，实施前应重新核对相应函数；以下路径和符号比行号更可靠。

关联文档：

- [发布优化与复测](../archive/benchmarks/2026-10-23-2d-publication-fix.md)：当前保守优化的实现与数字。
- [回归定位](../archive/benchmarks/2026-10-23-ui-authoring-diagnosis.md)：提交、ObjectIndex 和原生栈的证据。
- [原始 UI 对照](../archive/benchmarks/2026-10-23-ui-comparison.md)：最初指定的版本基线。
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
4. 消费完整 Mesh／MeshBinding 记录证明；命中则复用原 keys，否则通过 `same_authoring_identities`
   决定是否复用 after keys，必要时重新枚举。
5. 命中则运行 Mesh／Binding 局部摘要及目标 Mesh 派生通知；否则 `changes::summarize`
   比较实际领域数据，生成通知。
6. 克隆 checkpoint、估算历史预算、再次检查取消。
7. 发布 candidate，刷新 incarnation／缓存，安装历史并发布 receipt。

`same_authoring_identities` 在验证和 commit 的身份判断中都会使用；阶段 B／C 的完整
记录可避免这部分扫描，其他情况仍遍历多个集合。
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
| A：记录与影子对照（已完成） | Core tracker、窄入口接入、clone／恢复／失败降级；发布仍走现有流程 | 已通过记录完整性、差分、分支／历史、锁与原子性测试；见开头实施记录 |
| B：Mesh 局部摘要／身份判断（已接入） | SDK 消费 proof、局部 summary、工作次数计数 | 快慢结果及工作次数已通过；plain 成对耗时回归验收待执行 |
| C：Binding payload 接入（已接入） | keyform 实际写入记录、目标 Mesh 派生摘要 | Editor 属性链命中；差分／工作次数、旧地址／历史语义已通过；原生窗口及耗时复测待执行 |
| D：有限局部验证（已接入） | Label／Appearance／Enabled 的 Mesh-only 验证选择 | 已通过完整验证诊断差分、54 组恒定工作量及 touched／完整验证正反对照；见开头记录 |

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

以下是分阶段验证入口。A–D 的门禁及原生 UI 三轮复测结果见开头记录；独立 SDK 与
Editor 属性链成对耗时复测仍待执行。

```sh
# cwd: ../kasane-2d，迭代时按本阶段选择，避免重复跑完整套件
cargo test --locked -p kasane-core --lib document::structure::tests
cargo test --locked -p kasane-core --lib document::tracking::tests
cargo test --locked -p kasane-sdk --features bench-instrumentation --lib changes::
cargo test --locked -p kasane-sdk --features bench-instrumentation --lib changes::binding_payload_tests::binding_payload_work_is_independent -- --nocapture
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
