# UI draw 增长定位：Physics properties 与共享绘制路径

2026-10-24。基线：kasane-editor `44accd1` / kasane-2d `b86b13`；当前：kasane-editor `b4e4abc` / kasane-2d `e27230a`。前序未加诊断计数的性能结果见 [Physics 复测报告](2026-10-23-physics-ui-retest-44accd1.md)。

已经验证两条不同的原因：Physics 提交把资源版本变化当作控件身份变化；共享 Select 在值不变时重复通知，令新增的 Motion/Sound 属性区域在播放期间逐帧重建。两版 GPUI 和 vendored component 源码相同；这是新调用路径和额外工作量暴露出来的问题。

## Physics：资源版本变化触发整组控件重建

`properties::target` 将 PhysicsGroup 的 scope 从基线的 `Object` 改为 `Resource { revision: model_generation }`。`PropertyTarget` 的相等比较包含 scope；`PropertiesPanel::sync_form` 在 target 改变时清空 controls / selects / colors 并增加 epoch。因此一次数值提交，即使对象和字段集合不变，也销毁并重建整组控件。

正式测量的每组 40 次输入提交中，基线重建 0 个控件，当前重建 440 个，即每次 11 个。两种 Physics 文本负载、两轮都复现。含 5 次 warmup 的布局统计中，当前每轮新增 9,585 个 Taffy 节点，约每次提交 213 个；保留控件的诊断变体每轮只新增 1 个节点。

以下为诊断构建的整窗 UI draw P95（ms），不是原始性能报告的替换值。开关仅在同一对象/上下文下保留控件，仍更新 target 的资源版本；所有脚本均完成数值变化与 Undo 恢复验证。

| 负载 | 基线 | 当前 | 当前，保留控件 | 样本范围 |
|---|---:|---:|---:|---|
| Input weight text | 5.334 | 6.683 | 5.926 | 第 1 轮，未观察到编译器 |
| Node mobility text | 5.496 | 7.126 | 5.958 | 两轮 P95 的中位数，未观察到编译器 |

Input weight 的第 2 轮当前 normal 观察到编译任务，未用于上表的性能对照。计数结果仍保留。保留控件消除了相当一部分增长，但还存在新增 UI 和业务检查等剩余开销，不把全部差距归因于这一项。

## 共享 Select：不变的 searchable 值也会 notify

当前 Motion 属性定义加入当前 registration 的 fade / Sound 等字段；Sound 使用共享 Select。调用链为 `SelectState::render → list.set_searchable(searchable, cx) → cx.notify()`，setter 没有相等检查。通知令依赖列表状态的视图及其祖先重新构建，下一次 draw 又走相同路径。Timeline 的 playback speed 等现有面板也复用该 Select 路径。

在隔离的 component 副本中，仅增加“searchable 未变则返回”的诊断开关，保留相同界面内容和尺寸：

| 轮次 | 开关 | Properties render 次数 / draw 次数 | 每帧 views_built | 每帧复用布局节点 | 不变 setter 调用 / 通知 |
|---|---|---:|---:|---:|---:|
| 5 | 原行为 | 903 / 903 | 48.003 | 758.141 | 1806 / 1806 |
| 5 | 跳过重复通知 | 1 / 833 | 34.522 | 586.359 | 834 / 0 |
| 6 | 原行为 | 899 / 899 | 48.003 | 758.141 | 1798 / 1798 |
| 6 | 跳过重复通知 | 1 / 898 | 34.769 | 586.333 | 899 / 0 |

两轮都把 Properties 从几乎逐帧 render 降到整个阶段 1 次；视图构建数约降 28%，复用布局节点的遍历量约降 23%。这说明除了新增字段本身，原本静态的属性面板还丢失了保留绘制的收益。

两轮都观察到后台 Cargo / rustc，工作量对照成立，耗时改善幅度不作确定结论。记录的 UI draw P95 如下，调度波动明显：

| 轮次 | 原行为 P95 ms | 跳过重复通知 P95 ms |
|---|---:|---:|
| 5 | 4.102 | 7.877 |
| 6 | 3.701 | 3.519 |

负对照：只跳过 Select prepaint 中相同 bounds 的状态写回，没有降低 Properties 的逐帧 render 次数，两轮均如此。该项不是这次额外重建的主要触发点。相关原始记录和 3 个受干扰的 Physics 交叉实验均保留在 JSON。

## 共同的布局、绘制路径

`ui_draw_begin/end` 覆盖整个 `Window::draw`：准备帧 → view render / request_layout → prepaint（含 Taffy 求解、虚拟列表的可见行构建、hitbox / dispatch 节点）→ paint → 布局、文本和保留状态收尾。它是整窗 wall time，不能直接解释为 Physics 函数耗时、GPU 时间或输入到显示延迟。

Physics input weight，第 1 轮的各阶段平均 ms/frame：

| 阶段 | 基线 | 当前 | 当前，保留控件 |
|---|---:|---:|---:|
| build | 0.913 | 1.338 | 0.989 |
| prepaint | 1.392 | 1.689 | 1.521 |
| paint | 0.742 | 0.846 | 0.786 |
| 其中 Taffy compute_layout | 0.001 | 0.032 | 0.001 |

布局统计在 display callback 读取。脚本同步 dispatch 可在两个 callback 间连续执行 405 / 450 次 draw，所以这里以整段计数除帧数，包含 5 次 warmup；不能把 callback 区间平均数当成逐帧 P95。上面的 UI draw P95、控件重建数和 setter 通知数来自正式阶段，不含 warmup。Taffy 时间包含在 prepaint 内，不应再次相加。

增长主要在界面树构建、prepaint 遍历和 paint；Taffy 约束求解只占很小部分。原始二进制的 CPU 采样也显示 memmove、Style refine、元素状态、分配释放及 tab-stop / dispatch 维护分散占时，没有单个 Physics 业务函数独占 CPU。采样带有 profiler 开销；Physics 采样还用 5 Hz 拉长输入且观察到编译器，只作栈归因。

补充对照：将 Properties 内容替换为等尺寸空白后，两版播放的差距大幅收窄：

| 播放负载 | 基线 P95 ms | 当前 P95 ms |
|---|---:|---:|
| 原内容，两轮中位数 | 3.206 | 3.986 |
| 等尺寸空白，两轮中位数 | 3.070 | 3.077 |

这些配对未观察到编译器，但仍有帧相位、缓存和系统波动；不把剩余细小差距解释成通用 GPUI 算法回退，也不外推 Select 问题解释所有 Mesh / Parameter 场景。

## 建议的修复顺序

1. 拆开视觉控件身份与资源修订版本：同一对象、字段集合和上下文不变时刷新表单值与校验版本；只在对象、字段结构或编辑上下文变化时重建控件。继续保留 stale revision / preview / Undo 防护，不直接移除 Resource scope。
2. 让 `List::set_searchable` 在值不变时不通知；加入“关闭状态的 Select 不因重复配置引发父面板重绘”的定向工作量回归。Timeline、Properties 等共享使用点一起覆盖。
3. 前两项落地后再测剩余 build / prepaint / paint 开销，针对新增工具栏、字段与重复可用性检查缩小范围。当前证据不支持优先重写 Taffy 求解器或 Metal 渲染器。

## 复现与验证

共 35 个诊断进程，35 个均通过现有 properties / animation trace reporter；lost_events = 0。其中 23 个运行未观察到 Cargo / rustc 等进程，其余明确标记。用户要求保留后台任务，未暂停或终止其他任务。Cargo 即使处于休眠也被保守标记；出现标记本身不证明 CPU 争用。

所有诊断实现和开关仅存在于 `target/ui-draw-diagnosis-20261023/` 的源码副本和二进制，未修改产品实现。原有 benchmark 代码改动保留；本次只新增报告。诊断开关不能作为未经产品回归的正式修复。

机器可读数据：[2026-10-24-ui-draw-diagnosis.json](2026-10-24-ui-draw-diagnosis.json)。本地原始 trace、进程快照、CPU sample、构建日志、脚本与 `diagnostic.patch` 位于 `target/ui-draw-diagnosis-20261023/`；JSON 记录了版本、资产、二进制、脚本、补丁和逐份 trace 的 SHA-256。
