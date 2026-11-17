# 密集输入两个优化方向的深入研究

2026-11-10。基于 `7dfe6c0` 的当前源码、9 份本机历史原生窗口 trace 和 24 个离线时序案例。本文补充并修订[总方案](dense-input-scheduling.md)的实施优先级。没有修改 Editor 运行代码，也没有把离线模拟当作新的原生性能实验。

结论：**优先分离候选更新与场景求值，按交互选择最早的安全合并点。暂不将 ContinuousLatest 设为默认。** 两者是正交决策：一个决定“哪些工作可以少做”，另一个决定“何时做剩余工作”。不能为了统一调度器而统一所有操作的计算边界。

## 1. 证据与复现

### 1.1 本次实际做了什么

- 沿 Mesh、Warp/Rotation、参数、Seek、套索、选择笔刷、Inflate 的代码追踪输入、候选、全场景求值、请求与帧接收。
- 重新解析 `target/mesh-follow-pointer/leading-frame/` 下 8 个完成且未丢事件的历史运行，及 `target/mesh-drag-performance/detail-after/trace.jsonl` 的详细阶段计时。按输入父子 trace 归属关联，排除 warmup。
- 新增 [离线研究脚本](../../tools/benchmarks/dense-input-scheduling-study.py)，分析实际阶段耗时和请求间隔，同时用零计算成本的确定性模型研究调度规则。三个[回归测试](../../tools/tests/test_dense_input_scheduling_study.py)验证突发后持续等待的反例、样本延迟统计的盲点和延后工作归属。

```powershell
uv run --locked python tools/benchmarks/dense-input-scheduling-study.py --trace-dir target/mesh-follow-pointer/leading-frame --trace target/mesh-drag-performance/detail-after/trace.jsonl --output target/python/scratch/dense-input-study/results.json
uv run --locked python tools/test_tools.py -p test_dense_input_scheduling_study.py
```

没有本机历史 trace 时，省略 `--trace-dir` 和 `--trace` 仍可重现全部时序模型。JSON 分开存放 `synthetic` 与 `existing_traces`，并记录历史二进制 SHA-256；原始 trace 不随仓库分发。

### 1.2 已测成本分布

以下为 `detail-after` 的每个 workload 120 个有效输入的**均值**，单位 ms。该历史二进制 SHA-256 为 `43d112e1a5d1b277264b90a1893d8c3fa282f55d8da0337ff75247ac3125279d`；这是定位成本的单次诊断，不是新策略的配对性能结论。

| 操作 | 同步输入总耗时 | 候选更新 `preview_control_edit` | 场景准备 `scene_begin/end` | 其中 `snapshot` |
|---|---:|---:|---:|---:|
| Mesh 单点 | 2.645 | 0.079 | 1.979 | 1.144 |
| Mesh 八点 | 2.700 | 0.094 | 2.012 | 1.160 |
| Warp | 4.548 | 2.278 | 1.910 | 1.060 |
| Rotation | 4.571 | 2.266 | 1.963 | 1.077 |

`snapshot` 包含在场景准备中，不能将四列相加。候选更新也可能包含嵌套 SDK 工作。剩余同步输入成本不能归零或假定全部属于手势解析。

另一个二进制上的 8 份 leading-frame 对照中，未启用限流的普通 Mesh 输入，场景准备仍约占同步输入均值的 75%；这支持“场景求值是该 Mesh 工作负载的主要成本”，不能推广为所有模型、工具的固定比例。旧实验的完整表现见 [Mesh 实验](../archive/benchmarks/2026-11-10-mesh-dense-input.md)。

## 2. 方向一：逐事件语义与昂贵工作分离

### 2.1 Mesh 已存在可用的惰性边界

当前调用链如下：

```text
ViewportSurface::apply_modeling_input
  → Editor::update_canvas_gesture_with_modifiers
  → DocumentSession::input
  → ToolController::input / Gesture::update
      → 更新 current / dragged / modifiers / 锁轴 / 旋转累计量
      → ControlGesture::update
          → Document::preview_control_edit
              → mesh_edit::positions：Glue / 父变换逆解、合法性
              → preview_controls_with：写入交互 Draft 候选
  → invalidate_scene：失效缓存、增加 scene revision、reconcile
  → UI redraw_canvas_for_input
      → request_preview_for_input
          → preview_for_if_needed → preview_for → DocumentSession::scene
              → Document::snapshot → PreviewState::replace / frame
              → CanvasScene::with_positions 或 geometry
          → 按视口构建 Request → worker mailbox
```

证据入口：[交互处理](../../crates/zipzi-editor-core/src/session.rs)、[候选写入](../../crates/zipzi-editor-core/src/document/edits.rs)、[snapshot](../../crates/zipzi-editor-core/src/document.rs)、[预览请求](../../crates/zipzi-editor/src/presentation.rs)。

这意味着可以先做一个小范围改动：**所有原始 Mesh 更新继续进入 Core；Core 逐事件维护合法候选；UI 将“需要新场景”合并到同一调度周期。** 保留首个立即预览，其余调用只登记 dirty，回调从最新候选准备一次场景。修改的主要位置是 UI 请求边界，而非整个工具系统。

`evaluation_count()` 当前累计的是 `DocumentSession::scene()` 缓存缺失次数，不是候选应用次数。这个计数下降只能证明场景重建减少；需要另记候选次数才能判断优化到哪一层。

### 2.2 为什么它比直接丢 Move 更容易保持语义

假设一个周期里输入 A、B、C，A/B 合法，C 违反 `PreviewConstraint`：

| 路径 | 候选处理 | 周期结束后的候选 |
|---|---|---|
| 不限流 | A 成功、B 成功、C 拒绝 | B |
| 原始输入 Latest | A 成功，B 被覆盖，C 拒绝 | A |
| 只合并场景准备 | A 成功、B 成功、C 拒绝；最后只构建一次场景 | B |

实际 [ControlGesture](../../crates/zipzi-editor-core/src/tools/controls.rs) 还会维护 `constrained` 提示，并在没有先前合法候选或发生非 `PreviewConstraint` 错误时向上报错取消。保留逐事件候选路径，可以原样保留这些行为，也自然保留早期 Shift 锁轴和跨圈旋转。

这里的正确性来自保留当前领域执行顺序，不依赖“这个工具看起来像绝对拖动”的推断。不过仍须验证：输入间是否有必须观察到已求值场景的读操作；若有，那个读是同步求值边界，不能返回伪装成最新的旧缓存。当前控制点拖动以 press 的固定场景/变换为基准，适合先验证这种做法。

### 2.3 第一批接口与数据流

将现有 `FrameInput` 的调度状态复用于两种不同 payload，保持一个操作只有一处主节流点：

- **Mesh 安全模式：** payload 是 `PreviewDemand { owner, last_input, dirty_scope }`。每个输入已更新候选；回调只请求最新候选的场景。多个 demand 合并失效范围，而非保留陈旧场景或复制候选。
- **参数目标模式：** payload 是完整参数目标。回调执行参数更新和随后的预览准备，二者不再各等一帧。

`PreviewDemand` 应按文档/会话归属汇集受影响视口。一个文档的基础 `scene()` 已有共享缓存，不应把每个视口包装成各自的一次模型求值；各视口仍需各自的变换、隔离投影和 Request。首期只允许当前 capture 登记这类 demand，不引入全局命令队列。

建议在 UI 提供职责明确的局部入口，名称为示意：

```text
handle_input_result       保留错误、capture、状态栏、即时控件反馈
request_interactive_scene 立即准备或标记 dirty，返回调度结果
flush_interactive_scene   校验 owner，取当前候选，准备并发送请求
finish_interactive_scene 释放时撤销旧票据，为最终候选请求预览
cancel_interactive_scene 清除 dirty，领域回滚后走正常重绘
```

不能简单把 `redraw_canvas_for_input` 全部延后。它还同步 notice、状态栏和 canvas 通知；`redraw` 更包含 Runtime EditorContext、视口和动画时钟同步。应仅拆出昂贵请求部分，保留必要副作用。纯旧场景上的鼠标/工具反馈依旧可通知，图像和几何 overlay 继续按接收帧绑定。

所有候选 revision、projection invalidation、Draft 检查按原规则推进。不要为了“每帧一个版本”冻结 revision 或跳过 `reconcile`。Release 撤销旧票据后提交并请求终态；Cancel 清除需求后回滚。布局变化、外部发布和操作切换沿总方案的 barrier 规则处理。

特别要分清“本次事件没有改变候选”和“没有待显示变化”：B 已成功、scene 尚未请求，C 被拒绝时，本次 `model_changed=false` 也不能清掉 B 的 dirty；Release 与最后候选相同、没有再改模型时，仍须处理已有 dirty。参数 Release 则应与 applied 比较，不能因为它等于控件当前 desired 就跳过尚未应用的最终值。

### 2.4 收益边界

记每个周期输入数为 `n`，实际场景准备数为 `m`；每事件必需工作为 `G`，候选成本为 `C`，场景成本为 `E`：

```text
原路径：       n × (G + C + E)
只合并场景：   n × (G + C) + m × E
再合并候选：   n × G + m × (C + E)  （只在语义允许时）
```

Mesh 的历史场景成本占比约 75%，若场景次数减半，对这部分同步 CPU 成本的静态估算约减少 37.5%；场景次数降至四分之一则约减少 56%。这是按旧均值拆账的量级估算，未计新的调度成本、缓存效应、通知和线程竞争，不是性能实测或 FPS 预测。

Warp/Rotation 候选约 2.27ms，本身已经占同步输入约一半。只合并场景仍能减少工作，但不能替代候选级优化。不要为了 Mesh 中约 0.1ms 的候选成本先承担一次全面 Core 重构；应在后续高成本工具上证明进一步拆分的收益。

### 2.5 不同操作的真正合并点

| 操作 | 已确认行为 | 建议最早的安全切入点 |
|---|---|---|
| Mesh | 候选写入与全场景求值分开；部分失败保留最后合法候选 | 先保留逐事件候选，只合并 scene 请求 |
| 参数/Pad | `set_preview_parameters → set_preview_values → PreviewState::replace` 在接受新值前完整求值 | 合并参数目标，必须在调用 setter **之前**；成功后的 scene 会复用已求值帧，但几何准备仍有成本 |
| Seek | `locate_animation` 更新时间/状态；重计算主要由 `animation_request` 发往动画 worker | 保留 Begin/End/最新 pointer；合并 scrub 更新及其请求副作用，避免再叠一层 worker 等待 |
| 套索 | Move 主要追加相隔至少 2px 的点，完成时才执行区域选择 | 保留路径采样；先看 overlay/绘制成本，不期待减少模型求值 |
| 选择笔刷 | 每条轨迹段遍历候选并累计权重、距离余量 | 保留每段和权重顺序；优先减少 overlay 重建/通知，单独研究候选遍历或分段批处理 |
| Inflate | Move 更新中心；BrushTick 已由帧回调驱动，按固定步长积分并预览候选 | 不再套第二层帧限流；测 tick 内成本和失败回滚后再决定优化 |

参数链路的关键证据是 [`PreviewState::replace`](../../crates/zipzi-core/src/preview.rs) 的“先求值，成功才提交参数与缓存”。因此“只合并 scene 请求”不是参数的通用解法。合法范围/有限值可在接收目标时廉价验证，但若某些中间参数目标会触发求值失败，仍要明确其失败与恢复契约；不能承诺所有被跳过的错误都与逐样本执行等价。

Seek 已保持同一拖动的 acceptance epoch，让旧请求完成后仍可按序显示；最终定位改为精确目标。Mesh 与参数也有连续操作中接受中间已完成帧的规则，见 [`present_frame`](../../crates/zipzi-editor-core/src/workspace/presentation.rs)。新增合并不能简单要求“只有最新 desired 序号才允许显示”，否则高频输入可能让画面一直等不到合格帧。

### 2.6 候选级拆分的具体约束

需要进一步优化 Warp/Rotation 时，可将 `ControlGesture::update` 逐步拆成：

1. **启动：** 在首次跨阈值时准备 targets、冻结基准、创建 Draft。保持即时，不能把起步事件也覆盖掉。
2. **摄取：** 每个事件更新 current、dragged、修饰键、锁轴和展开角度，形成带序号的 `ControlChange`。`last_ingested` 与 `last_applied` 分开，去重不能误用未应用目标。
3. **准备/校验：** 生成网格逆解或变换写入计划，明确哪些错误为软约束、哪些应取消。只有经证明与中间候选无关的目标可在这一步之前覆盖。
4. **应用：** 写入候选、更新最后合法状态、失效投影，最后统一准备 scene。

可以考虑让 `mesh_edit::positions` 返回可复用的 `PreparedMeshEdit`，避免校验后再算一遍。但它只证明几何逆解完成，不自动证明全部 SDK 写入与候选校验都能成功。`preview_controls_with` 后续仍调用 Draft/SDK；必要的校验不能随旧 intent 一起丢弃。

若要保留“最后合法候选”同时合并写入，至少需要区分 `latest_valid_prepared` 和 `latest_feedback`。C 非法时保留 B 的计划，却显示 C 的约束提示；不能一个 Latest 槽同时承担二者。计划还须绑定 source/version、目标集合和语义代次，且应用必须与旧逐事件路径差分验证。这一复杂度正是候选层应后置的原因。

## 3. 方向二：更严格的帧调度

### 3.1 两种策略的精确定义

`LeadingLatest` 是现有样板的节奏：首更新立即执行，后续只保留最新；回调执行尾值后开放新的 leading。持续密集输入下，理论上可以每轮一个 leading 加一个 trailing。它不是严格每帧一次。

`ContinuousLatest` 是前一版提议的候选：尾值执行后保持节流，并登记下一回调；只有遇到一个没有 pending 的回调才回到空闲。它将持续更新压到每回调一次，但“有没有 pending”并不等于“是不是过载”。

两者都保留同步 Release/Cancel。首次处理、错误和语义边界不计入所谓每帧一次的常规更新上限。

### 3.2 反例：短突发能导致长期多等一帧

设帧间隔为 16.67ms，最初输入发生在 1.00、1.01、1.02ms；之后恢复每帧一次，时间为 17.67、34.33、51.00ms……

```text
1.00ms   两策略都立即应用
1.01/.02 两策略都保留最新
16.67ms  两策略都应用 1.02ms 的尾值
          Leading 开放；Continuous 继续节流
17.67ms  Leading 立即应用；Continuous 入槽
33.33ms  Continuous 应用 17.67ms 的值；仍然继续节流
34.33ms  Leading 立即应用；Continuous 再次入槽
……       每次回调都有 pending，无法退出节流
```

从稳定阶段看，二者每秒都应用 60 次，Continuous 却给每个输入增加约 15.67ms 等待。这个反例不需要慢 GPU、任务积压或计时噪声；来自退出条件本身。因此不能用它作为所有输入的默认替代。

### 3.3 24 个时序案例的代表结果

下表为**纯调度模型**：60Hz 固定帧机会，输入首相位 1ms，摄取/应用成本为零；统计 250–2000ms 的稳定阶段。回调与输入同时到达时先处理回调。这里的“值年龄”是每 0.25ms 观察当前 applied 所对应原始样本的年龄，尚未经过渲染。

| 输入模式 | 应用次数/秒，Leading → Continuous | 被应用样本等待 P95，ms | applied 值平均年龄，ms |
|---|---:|---:|---:|
| 30Hz | 29.71 → 29.71 | 0 → 0 | 16.69 → 16.69 |
| 60Hz，无起步突发 | 60 → 60 | 0 → 0 | 8.29 → 8.29 |
| 60Hz，起步短突发后恢复 | 60 → 60 | 0 → 15.67 | 8.32 → 23.99 |
| 90Hz | 90.29 → 60 | 4.56 → 10.11 | 7.05 → 15.63 |
| 120Hz | 120 → 60 | 7.33 → 7.33 | 7.81 → 15.66 |
| 240Hz | 120 → 60 | 3.17 → 3.17 | 7.57 → 11.49 |
| 1000Hz | 120 → 60 | 1.00 → 1.00 | 7.88 → 8.86 |
| 每帧突发四个输入 | 120 → 60 | 15.64 → 15.64 | 8.32 → 23.96 |

次数不是整数的行来自有限观察窗口，不代表频率漂移。这里没有 CPU 竞争，所以不能据此断言现实中 Continuous 更慢；它可能通过省下昂贵计算减少真实卡顿。但模型已经证明：减少计算次数不自动减少状态年龄；只统计“被应用的最新样本等待”也会漏掉值持续停留的代价。

每帧四输入的反例尤其关键：Continuous 可能等待上一帧之后早已到达的尾值；Leading 则能先显示当前突发的首值。在突发内指针差异很小时，少做一半计算的价值必须与晚一帧的风险一起测量。

### 3.4 更晚取最新值，未必更早显示

帧回调在 draw 前触发，但其创建的新请求还要经过主线程场景求值、worker、GPU 可用性，再到图像 latch。设：

```text
F = 帧回调时刻
L = 本轮视口采纳已完成图像的时刻
E = 同步场景准备耗时
R = 请求入队到图像可用的耗时

回调的新目标赶上本轮的必要条件：F + E + R ≤ L
```

实际 `L` 还受 UI build、布局、旧帧采纳和平台过程影响。这是因果条件，不是当前 API 可提供的可靠显示 deadline；GPUI 的 `on_next_frame` 没有给本模块一个可直接使用的端到端呈现截止时间。

离线脚本另设独立结果准备时间 3ms、本轮 latch 在 F+1ms 的假想管线：回调产生的请求必然错过本轮。把 latch 改为 F+4ms 后则可能赶上。同样的调度次数，显示年龄结论会变化。因此脚本将该结果标为 `hypothetical_presented_age_ms`，不能当作屏幕延迟报告。

这也解释了为何不能只看到“回调里取到了更近的指针位置”就认定跟手更好。已有原生实验中纯等待方案被淘汰，与这个风险一致，但不是对 ContinuousLatest 的直接 A/B 证明。

### 3.5 现有 trace 是否证明帧边界存在大量重复？

重新关联两份启用态密集实验的 scene 请求，按有无 `mesh_move_immediate` 标记识别 leading 与推定 trailing，统计“尾场景准备结束 → 下一个 leading 场景开始”：

| 运行/操作 | 相邻尾→首对数 | 最短间隔，ms | 中位间隔，ms |
|---|---:|---:|---:|
| paired-03 / 单点 | 90 | 7.62 | 8.44 |
| paired-03 / 八点 | 90 | 7.75 | 8.90 |
| paired-06 / 单点 | 89 | 7.53 | 8.57 |
| paired-06 / 八点 | 89 | 7.45 | 8.59 |

四组共 358 对，没有一对小于 4ms。数据来自实验二进制 `18f244720b77733dc6a7f1f3526a042df1baa1ff4d3c419a8dc05a87bf924da9`。这里没有独立帧回调 trace，只能按样本归属推定尾/首；不能据此证明它们是否命中同一个呈现截止点。

源码允许尾值后立即 leading，但现有这组负载未展示该极端时序。我之前把“消除边界重复”列为优先改进动机，证据不足，应降低优先级。这些相隔约 8–9ms 的计算可能分别帮助不同的显示机会，不能直接归类为浪费。

## 4. 是否有比简单 Continuous 更好的规则？

### 4.1 默认先保留 LeadingLatest

采用方向一的合并点后，继续复用已验证的 LeadingLatest。这样可以独立回答“换合并边界是否足够”，不会把语义改造和节奏改造混在同一实验中。路径语义敏感的输入在候选之后合并；可安全替换目标的输入在候选之前合并。

### 4.2 真有短间隔重复时，优先研究短冷却而非整帧封闭

可选规则是记录最近一次昂贵工作结束时间 `last_done`，仅在新请求与它相隔小于 `g` 时合并；到 `last_done + g` 或更早的帧机会消费最新需求。`g` 应根据实际重复间隔与耗时确定，并明显小于帧周期，不能预设为 16ms。

这仍需要一次带 generation 的短 timer、同一 pending 槽、帧/timer 双唤醒去重以及隐藏/取消清理。若执行完仍有需求，再安排下一次；不要每个输入重置截止点。相比 Continuous，它不会只因 60Hz 流一直有 pending 就强迫每次等下一帧。

但这不是当前推荐立即实施的方案：现有密集 trace 的尾→首最短已超过 7ms，一个 2–4ms 的保护窗口几乎不会命中，反而增加机制。先补原生帧边界负载再判断是否有这种问题。

### 4.3 真过载时，自适应需要独立的退出信号

若在 500/1000Hz 真实输入或慢模型中确认 UI 饥饿，可以只在持续过载时启用严格模式。进入信号至少包括原始输入密度与候选/scene 工作耗时，退出信号必须来自**原始输入统计**，不能只看调度后的 pending 是否为空。

例如分别记录每窗口周期到达数量、覆盖数量、最早未服务时间、最近实际求值耗时，以及连续多个周期的工作占用。持续高占用才进入；到达率恢复或没有可避免的重复工作后退出，使用滞回防抖。具体阈值留给实测，不能从本次零成本模型反推。

这种规则仍无法兼得所有场景的最低 CPU 和最低延迟：严格模式主动牺牲中间响应换取 UI 预算；必须报告等待和尾延迟。Release、Cancel 和错误恢复继续绕过预算，不能为满足“每帧一次”而延后结束操作。

### 4.4 更远期才考虑截止时间或 worker 反馈

如果以后获得可靠的帧截止时间和完整阶段测量，可以研究 `latest_start = deadline - cost_estimate - margin`，让计算尽量靠近但赶得上显示。这要求估计 CPU/GPU 波动并处理 missed deadline，目前只有 `on_next_frame` 不够。

worker 繁忙/完成可以提供拥塞提示，不能作为所有领域写入的硬门闩。也不能每来一个目标就取消正在运行的任务；若输入始终快于求值，持续取消会造成没有任何结果完成。已有连续帧接收规则应保留，过期操作/基准版本则必须拒绝。

## 5. 建议的原生验证顺序

### 第一轮：只换合并边界

同一个二进制、同一 LeadingLatest 节奏，对照三种模式：Off、现有原始 Move Latest、逐事件候选 + scene 合并。最先测普通/密集 Mesh，再测带约束失败轨迹；只在语义等价通过后比较性能。

新 trace 至少分开记录 raw ingress、语义摄取、候选准备/写入、scene 求值、frame callback begin/end、request 和实际图像采纳/CPU 提交。尾回调中的工作必须纳入主线程工作总量，不能仅统计 `input_sync_ms` 就声称 CPU 减少。

关键判据：最后合法候选、最终文档、提示状态、历史/Undo 完全符合原领域规则；scene 次数降低；普通输入首响应不增加；密集输入更新间隔和墙钟采样姿态年龄同时报告。

### 第二轮：只换调度节奏

固定第一轮选定的合并边界，再对照 Leading、Continuous，以及有证据才加入的短冷却/过载模式。负载包括：

- 30/60/90/120/240/500/1000Hz，围绕显示刷新率的非整数比例；60/120/144Hz 显示设备分别测。
- 单次突发后恢复每帧一个输入、每帧四输入、恰在回调前后到达、突然停手、Release 前最后输入。
- 低成本 Mesh、高成本 Warp、参数 Pad、Seek；分别看 CPU 和 worker 是否过载。
- 稳定墙钟轨迹与真实鼠标投递分开，记录实际到达率。当前 benchmark 的 burst 是每次唤醒串行投递多个事件，主线程工作会改变后续唤醒时刻，不能把配置的 `HZ × burst` 当作实际事件率。

指标必须包含 raw 到 desired/applied、首响应、场景/候选次数、每秒主线程总耗时、更新间隔和最大停顿、Release 到终态、每个墙钟采样点的姿态年龄/误差。避免只看被显示样本的延迟而漏掉中间停留。

### 实施决策

1. **现在可实施：** 提取通用调度状态；Mesh 保留逐事件候选、合并 scene；参数在 setter 前合并完整目标；补齐两类生命周期与 trace。
2. **随后验证：** Seek 接入；对 Warp/Rotation 的高成本候选逐步拆分摄取、准备、应用，保留错误边界。
3. **暂不默认启用：** ContinuousLatest。先证明存在节流可以解决的过载，并修正突发后持续等待的退出条件。
4. **有数据再做：** 短冷却、窗口预算、后台候选或截止时间调度。单次计算过慢时需要降低/转移计算成本，仅降低调用频率不能消除那次主线程停顿。
