# R2：Editor 托管与窗口生命周期

日期：2026-10-24。R2 已交付；R3 尚未开始。最终性能验收结果见下文。

## 所有权与发布

应用级 `DesktopHost` 强持有 `RuntimeHost`，窗口持轻量视图。Editor Core 的
`Document.session` 改为 `HostedSession`（RuntimeHost、SessionId、事件游标），
`PendingEdit` 仅持 `HostedEdit`。正式 SDK 与历史只在 Runtime 中存在一份。

模型/属性/排序/对象/资源/Timeline/Undo/Redo 都经类型化 Runtime 发布；领域规则、
计划的完整 Version 检查、句柄校验和历史仍由 SDK 执行。只读 guard 直接借正式模型；
大量 Motion/Physics/Pose 数据用既有模型内的 Arc 共享，避免为 guard 生命周期复制
整条曲线。Editor 的选区、工具、投影、纹理和播放适配仍由视图持有。

SDK candidate 由 Runtime 表持有。少量需要先验证再替换旧手势的入口使用有界
`Prepared` 工作区：它不可发布，计入候选数量/字节；晋升时检查每客户端/会话只
允许一个正式可提交候选。`CandidateWork` 临时借出候选以便 Editor 辅助函数读取
正式模型；即使 closure 返回错误或 panic，适配器也先归还并标记候选失败。
所有权租约期间仍计费。普通错误沿用 SDK 全事务失败规则，可恢复的 preview
校验沿用其原有行为。

交互占用属于 Runtime 元数据。人的候选手势期间，其他客户端的 Commit/Undo/Redo/
独占作业返回 Busy，不消费外部候选。结束后旧候选由 SDK 返回完整版本冲突。

Runtime 独立消费 SDK receipts。Editor 本地发布保留既有增量投影路径并推进游标；
外部发布走同一事件流，更新投影/纹理/动画，断档则重新同步。订阅通知在 Runtime
写 guard 释放后合并投递，不在 SDK 可变借用中回调。工作量测试约束无重复提交、
历史、索引重建及无关面板通知。

为适应只读正式模型，视图的默认 `PreviewState` 已在 R2 提前从 SDK 实例中分离；
它仍使用 SDK/Core 的求值规则。远端 Preview/Plan/Proposal/token 生命周期留到 R4–R6。

本地 receipt 快速路径若越过外部发布或遇到事件断档，会保留明确 resync 标记，
下次同步重新建立完整投影。外部保存即使在两次 UI 轮询之间完成，Saved/状态事件
也会触发路径/modified 的展示刷新；不依赖观察到中间 Busy 才刷新。

候选表的 lease/return/释放不再唤醒正式投影任务；宿主用独立事件唤醒标记检查
Session/注册表流的变化，释放 Runtime guard 后才通知。该标记仅用于短调用的本机
合并唤醒，不是内容 Version 或协议事件游标。普通提交、Busy/Ready/Faulted 与关闭
仍有测试验证通知。共享身份集合的容量估算随其不变 Arc 复用，每个候选仍按原规则
保守计费，避免鼠标每次移动重新遍历完整对象身份表。

## IO、路径和窗口

Open/Import/Save 的正式所有权、路径身份及 Save As 预留都归 Runtime。UI 不再维护
另一份路径占用表。`Work` 携 SDK 在后台执行，`Completion` 必须先归还应用宿主并
记录 Job 终态，再尝试弱视图回调。无窗口期间完成不会重新打开窗口。

加载在具有 8 MiB 栈的解码线程运行，随后在后台准备纹理/只读对象投影。正式完成
先入 Runtime，再交付有版本的视图准备结果；准备失败不撤销成功的正式导入，也
不能吞掉 Completion。保存 worker 只携 SDK，视图侧冻结展示继续使用既有投影。
普通错误归还同一个 SDK；SDK worker panic 保留 Runtime 冻结来源并进入 Faulted，
视图不会用空白文档伪装 Ready，也不残留 Busy。

`DesktopHost` 在无窗口时保存 Editor 的视图状态，并缓存最近 32 个终态结果与
待显示错误。重开窗口先对账终态 Job，再挂接驻留会话；即使在旧窗口的完成回调
之前重开，也能恢复正确状态。任务结果的协议去重/重连窗口属于 R3。

`--no-window` 启动同一应用宿主，不创建窗口。关闭窗口或标签只 detach，保留正式
会话、未保存修改与历史；重新打开窗口列出驻留会话。明确 Quit 仍经过保存/丢弃/
Busy 协调。最后窗口关闭不自动退出。dev-session 保存驻留 Runtime 路径，继续使用
同一个 PID 发现与清理协议；进程退出后的历史持久化不在内存驻留保证内。

## 验证

- Runtime：29 项（含 Prepared 晋升、交互 Busy 保留候选、租约失败及 100 轮释放）。
- Editor Core：35 单元测试 + 468 集成测试。
- 原生 UI 全量：528 项通过、1 项原有 ignored；另外 Core 35 项与渲染 2 项通过、
  渲染 1 项原有 ignored。涵盖 Timeline、属性/IME/焦点、Esc、失焦、工作量及
  5 项应用宿主生命周期。
- 普通 SDK 全量：29 单元测试 + 337 集成测试通过，2 项原有 ignored；包含历史预算、
  工程 IO、候选快照及 2 项估算缓存独立重算测试。工具 Runtime 系列 12 项通过。
- 静态守卫 `tools/runtime/check_editor_boundary.py` 与破坏性 fixture 检测测试：
  禁止 Editor 持有/可变借用正式 SDK、直接 drain 或把 IO 正式完成置于弱视图回调后。
  Runtime 源引用索引用于逐项审阅，`reviewed-r2` 不单独代表行为证明。

原生生命周期用真实 SaveTask，先确认 Busy，再关闭最后窗口并释放最后强视图；
分别验证无窗口保存成功/失败、完成前重开、同一会话/历史/路径/modified 与错误。
Core 另验证无视图 Open 完成、外部 Busy/恢复、外部发布和 Faulted 冻结来源。

既有 Physics 工作量用例引用已移除的 `physics-copy-group`。在原样 R1 副本上重现
相同失败后，baseline/candidate 测试均改走当前资源列表的 Duplicate 上下文菜单；
保留全部原有内容与工作量断言，修正后的 baseline 和 candidate 均通过。

空白文档的 UI pristine 状态按完整 Version 记录于有界宿主视图元数据，标签 detach/
重挂仍保留；外部提交即使尚未被视图消费 receipt 也立即打破该标记。显式关闭
未经修改的初始空白允许释放，实际脏文档仍要求保存决定。

全量 UI 测试另发现 Properties 字段覆盖断言把 Mesh texture 的领域描述符当作普通
表单字段；该测试与表单定义在 R1 副本中逐字相同。断言现在明确排除由资源流程
负责的 texture 分配，其余字段继续逐项相等。标签关闭测试按 R2 契约改为 detach
后检查驻留脏状态，再明确 Quit 验证原保存确认/Cancel 行为。

## 复查修复（2026-10-24）

两轮复查的七个问题已修复：退出检查包含脱离标签的任务和其他客户端的临时加载，
无窗口 Quit 统一进入退出协调；Timeline 候选持有自动释放的交互占用，异步
Undo/Redo 同样遵守它；即使视图未观察到 Busy，也会冻结 Faulted 会话并显示诊断。
窗口恢复先同步已有文档投影，订阅后再次同步以补齐间隙；标签关闭后的保存失败
仍显示错误。同步保存调度失败统一取消、回收任务和路径预留。

图片导入、图片/附件内容替换、矩形网格重建改为 SDK 生成 owned candidate，
Runtime 执行候选数量、字节和发布历史预算检查后统一提交。准备好的图片负载随
候选保留，仅在成功提交后进入工程资源表；预算拒绝不消费加载令牌、不改变版本、
历史、redo 或事件。图片底层分配的完整计费与回收仍属于 R4。

本轮定向验证：Runtime 30 项；Editor Core Runtime 18 项、Timeline 48 项、
退出/保存协调 16 项；原生宿主生命周期 7 项；SDK 资源 19 项、Motion library
22 项（1 项原有 ignored）、矩形几何 3 项；Editor 所有权静态守卫及工具测试通过。
以下性能数据是此前 R2 验收记录，本轮正确性修复未重跑配对性能基准或全量套件。

## 性能

对照来源为开始 R2 时的 R1 工作区快照（HEAD 加原有未提交补丁及新增文件），保存于
`target/runtime-r2/r1-baseline.patch`、`r1-untracked.tar`、`baseline-source/`。
正式比较使用分别复制的 Release 二进制，禁止测量期间并发编译，至少五对交替进程，最终为十对。
收敛过程中两次不完整采样保留在 `setup-sampling/` 与 `before-event-edge-fix/`，
不与最终 `paired/` 数据合并；最终比较只使用最终事件边界修复后构建的二进制。
阈值沿用计划 §6：P50 +10% / P95 +15%，CPU 0.2 ms、输入到显示 1 ms 取较大裕量。

初测发现 Runtime 预算刷新会重复扫描历史检查点。SDK 为不变历史条目缓存估算值，
Undo/Redo checkpoint 交换与 Save As 资源重定位使缓存失效。预算范围与阈值不变，
独立重算测试检查容量估算一致。首次五组比较还定位到候选开始与发布边界的重复
内容扫描：候选缓存初始 SDK 估算，所有可变 Document 访问与整候选导入使其失效；
正式内容按完整 Version 缓存，保存显式失效。发布复用已验证的候选大小与检查点大小，
元数据仍按实际持有容器容量计费。预算值、拒绝边界和候选增长后检查均保留。

最终使用同一组 Release 二进制，完成 **10 对交替进程、145 项指标，全部通过**。
下表保留配对结果与统计范围；历史 JSON 产物已清理。没有合并不同操作或模型。
最终前五对单独汇总仍有少量尾延迟/呈现超限；在不改变代码、场景、每进程样本数或
阈值的前提下，事先决定对所有场景统一补足十对，并汇总全部十对，未挑选五对。
这符合计划“至少五对”的要求；原始前五对与早期失败记录均保留。

机器为 Apple M4、16 GiB、macOS 27.0.1；Rust 1.99.0，Mao 同一本地模型，窗口
1100×740。以下是每进程分位数的跨进程中位数，单位 ms，格式 baseline → candidate。
完整验收结论为 145 项全部通过。

| 操作/边界 | P50 | P95 |
| --- | --- | --- |
| CPU Mesh opacity 开始候选 | 1.851 → 1.899 | 2.398 → 2.341 |
| CPU Physics mobility 更新+求值 | 4.396 → 4.431 | 5.238 → 5.190 |
| 原生数值拖动 → presentation | 44.053 → 44.123 | 45.956 → 45.802 |
| 原生 RGB 拖动 → presentation | 43.051 → 42.647 | 45.267 → 45.532 |
| 多顶点拖动 → presentation | 38.924 → 39.389 | 40.770 → 40.766 |
| Warp 拖动 → presentation | 39.752 → 40.482 | 45.732 → 40.742 |
| 播放 enqueue → accept | 3.526 → 3.507 | 5.085 → 4.907 |
| 自然 Timeline 键拖动 → 当前 Metal draw | 21.298 → 21.580 | 23.265 → 24.578 |
| 密集 Timeline 键拖动 → 当前 Metal draw | 76.361 → 76.456 | 86.719 → 85.874 |
| 双视口交替 → 当前 Metal draw | 0.837 → 0.820 | 6.035 → 6.469 |
| 另一会话批次期间双视口 → 当前 Metal draw | 1.538 → 1.368 | 3.317 → 4.062 |

CPU 进程 RSS 中位数为 230.45 → 228.09 MiB；范围为 228.78–250.05 与
227.77–228.52 MiB。它是进程测量，不替代 Runtime 的存活计数与容量预算，也不计作
R4 的实际图片/PNG/blob 回收验收。100 轮候选释放测试返回相同已计量字节基线。

性能快照保存于 `performance-source.patch` / `performance-untracked.tar`。
采样后只补齐了显式 Quit 被其他客户端拒绝时的错误分支：取消退出并显示 Busy，
避免忽略 close 错误后递归重试。该函数不在任何计时路径中；新增回归后重新运行
32 项聚焦原生测试及原生全量测试，均通过。源码差异单独保留于
`post-performance-quit-guard.patch`，JSON 明示此差异。

真实进程检查 `tools/runtime/editor_no_window.py` 另确认：`--no-window` 启动后持续
存活、PID 发现一致、WindowServer 窗口数为 0；只终止并清理测试自身创建的进程。

双视口用例的候选准备/作业准入在每次输入计时之前；计时覆盖另一文档 SDK worker
实际发布期间的人工输入到当前 Metal draw。它验证 R2 的任务隔离，远端请求队列与
更大批次公平调度仍按 R3/R4 单独验收。测量不覆盖 OS 输入投递或硬件扫描输出。


最终原生配对按计划最低场景选择既有 `opacity_drag` 与 `picker_rgb_0_drag` 驱动，
另运行四种顶点/变形器、播放、Timeline、双视口与另一会话提交。首轮完整 Mesh/color
套件的 33 个场景均通过 trace 完整性和功能校验，原始一组数据保留；全量 UI/工作量
回归另行覆盖其余表单，配对性能结论只对明确列出的场景成立。

默认 Warp59 (`36d63e33-4a9e-4613-8ab4-644f7ed85041`) 在 R1 仍触发父变形奇异，
失败 trace 保存在 `deformation-default-rejected/`，未纳入比较。按 [R0 已记录问题](r0-results.md)
和既有 UI 基线，最终两版均固定 Warp `5101be9b-3d35-45a7-8764-b8d62616ada4`；
Mesh/Rotation 保留既有相同选择规则与输入轨迹。未更改领域规则、绘制方式或阈值。

`before-event-wake-cache/` 保留一次五组完整比较：CPU、数值/颜色和播放通过，
多顶点到呈现 P50 与 Timeline 画布导航 P95 超限。后续身份估算与候选通知优化后
再做完整比较。启动期 `manual` 帧没有脚本测量输入，按原计划的“预热后”口径从
交互延迟门槛中排除，原始报告仍保留这些帧；已有测试防止把它们混进输入统计。

`Editor::rehost` 是 fixture/bootstrap 辅助入口；应用正常构造使用 `with_runtime`，
单文档插入的所有权转移在全部准入检查后执行。它不作为跨宿主多文档原子迁移协议。


## R2 退出门槛

| 编号 | R2 证据 |
| --- | --- |
| A02–A05 UI 部分 | Runtime 路径/预留统一；完整 Core/原生制作回归；共享唯一历史、无重复发布；SDK 版本/身份/旧计划错误沿用 |
| A08 | 无窗口成功/失败任务、普通错误归还同一 SDK、worker panic Faulted 冻结、其他文档可用、取消与拒绝清理 |
| A09 | 人的交互占用返回 Busy，保留外部候选；结束后的旧版本提交由 SDK 拒绝 |
| A10 | Runtime 唯一 drain；本地增量路径不重复；外部投影、断档 resync、快速外部保存元数据刷新与按正式事件唤醒 |
| A13 | 原生五项生命周期、真实无窗口进程、重开前后同一会话/历史/路径/modified/错误；明确 Quit 走协调并处理拒绝 |
| A16 | 10 对进程、145 项 CPU/原生输入与显示指标均满足冻结门槛；既有 UI 工作量回归通过 |

R2 不包含 IPC、ConnectedBackend、Python 重连/请求去重，也不把 R4–R6 的资源、Plan、
Preview、Proposal、blob 全套预算表述为已经完成。下一阶段为 R3。
