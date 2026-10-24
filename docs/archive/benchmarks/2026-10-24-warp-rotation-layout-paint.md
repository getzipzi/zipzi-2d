# Warp / Rotation 剩余 UI draw：布局与绘制路径定位

## 结论

确认了菜单之外的额外 UI 工作：Parameters 分组标题每帧多处理 4 个复用布局节点和 33 条元素状态；Mesh / Deformer 工具栏新增的创建按钮共多处理 12 个复用布局节点和 32 条元素状态。二者合计解释普通对照中全部 **+16 个复用布局节点**。

但这不等于解释了上一轮全部 P95 差异。细粒度回退能稳定恢复节点与状态数量，整帧 CPU / P95 的改善方向仍受轮次影响。本轮未确认新的布局求解故障，也不能把剩余耗时全部判给这两个新增控件。

## 对照范围与可信度

- 基线：editor `44accd1` / SDK `b86b13`。当前：editor `b4e4abc` / SDK `e27230a`，包含已验证的 Properties 控件、Select 通知和菜单查询修复。
- 30 次原生窗口诊断运行：普通对照 6 次、面板消融 12 次、同一二进制的标题 / 工具栏回退 12 次。每次均运行 Mesh 单点、多点、Warp、Rotation；每阶段预热 30、正式 300、请求 120 Hz。正式 tick 共 36,000；Warp / Rotation 各 9,000。
- Mao 模型及 21 个资源沿用上一轮校验；固定 Warp ID。所有运行的 fixture 完全相同：Warp 4,929 个受影响顶点、Rotation 2,947 个。全部正常退出、benchmark_done、lost_events=0。
- 保留用户后台任务。1 Hz 监测在这 30 次运行中未观察到编译进程；这不能排除其他后台负载、CPU 频率和缓存竞争。
- 两侧使用同一份 GPUI 逐帧埋点，基线保留其组件与 SDK 源码。记录 setup / build / prepaint / paint / finish 的 wall 与当前线程 CPU 时间，以及每帧工作量；诊断开销存在，不能替代未埋点的性能验收。
- 表中“CPU”是每次运行的正式帧平均 CPU 时间，再取轮间中位数；P95 是每轮逐帧 P95 的轮间中位数。阶段分别取中位数，不能直接相加得到总数。
- `scene_primitives` 是 CPU 场景记录数（包括字形），不是 GPU draw call 数；UI draw 的 paint 是 CPU 绘制遍历，不是 GPU 执行时间。

## 普通对照：平均增量主要出现在 build

| 场景 / 版本 | UI draw CPU ms | UI draw wall P95 ms | build CPU | prepaint CPU | paint CPU | 布局求解 wall |
|---|---:|---:|---:|---:|---:|---:|
| Warp / 基线 | 2.005 | 2.401 | 0.629 | 0.590 | 0.604 | 0.0232 |
| Warp / 当前 | 2.185 | 2.507 | 0.720 | 0.685 | 0.615 | 0.0242 |
| Rotation / 基线 | 2.293 | 2.599 | 0.710 | 0.697 | 0.682 | 0.0239 |
| Rotation / 当前 | 2.278 | 2.795 | 0.757 | 0.680 | 0.647 | 0.0223 |

Warp 的 build 与 prepaint 有增量；Rotation 的 build 有增量，但整帧平均 CPU 没有稳定增加。Rotation 高 P95 轮次的 CPU P95 也同步升高，因此不能简单解释为线程被调度挂起。数据不足以区分频率 / 缓存竞争与未隔离的长尾计算。

普通对照两侧每帧 `style_writes=0`、`children_writes=0`，`compute_layout_calls=19`，求解约 0.02 ms；视图构建约 17、复用约 17。没有观察到布局反复失效、整棵视图树新增重建或大量新增文本 shaping。

## 面板消融

保持 Dock 面板尺寸，分别将指定面板的内容换成空 Div；标题和工具栏保留。此控制用于定位子树，不作为功能修复。

| 消融 | Warp 复用节点：基线→当前 | Rotation 复用节点：基线→当前 | 解释 |
|---|---:|---:|---|
| Properties 内容 | 387→403 | 382→398 | 属性内容不是 +16 节点来源 |
| Mesh / Deformer 内容 | 387→403 | 382→398 | 树内容不是 +16 节点来源；工具栏仍存在 |
| Parameters 内容 | 313→325 | 308→320 | 差异由 16 降到 12；分组标题位于该内容子树 |

清空 Properties 后新增图元差异减少 16 条，清空树内容后差异减少 22 条（Warp）/ 13 条（Rotation）。它们会增加缓存场景与交互数据的复制工作，但消融没有稳定消除整帧耗时差异，不能据此认定 Properties 或树内容是新的主要瓶颈。

## 同一二进制的细粒度回退

在当前诊断构建中加入运行时开关：恢复旧分组标题实现、移除树面板新增创建按钮、同时做两项。所有其他当前代码、数据与埋点相同；三轮交错顺序。默认关闭开关时的节点、元素状态、命中框及图元计数与普通当前版本一致。

| 控制 | Warp / Rotation 复用节点 | Warp / Rotation 元素状态 | Warp CPU ms | Rotation CPU ms | Warp wall P95 ms | Rotation wall P95 ms |
|---|---:|---:|---:|---:|---:|---:|
| 当前 | 403 / 398 | 3023 / 3048 | 2.135 | 2.390 | 2.526 | 2.919 |
| 旧分组标题 | 399 / 394 | 2990 / 3015 | 2.148 | 2.256 | 2.493 | 2.535 |
| 旧树工具栏 | 391 / 386 | 2991 / 3016 | 2.084 | 2.384 | 2.411 | 2.805 |
| 同时回退 | 387 / 382 | 2958 / 2983 | 2.097 | 2.399 | 2.406 | 2.724 |

分组标题还解释了每帧新增的 1 个布局节点分配：当前每帧创建 16 个，恢复旧分组标题后为 15 个，与基线一致。恢复两个树工具栏会减少 2 个图元记录；其他新增图元属于当前新增的可见内容。

同时回退时，Warp 的 P95 轮间中位数由 2.526 ms 降至 2.406 ms（−4.8%），Rotation 由 2.919 ms 降至 2.724 ms（−6.7%）。整帧平均 CPU 的对应变化为 −1.8% / +0.4%，部分单轮结果和分别回退结果不单调。

节点 / 状态差异是确定的因果证据；总耗时收益还存在较大不确定性，因此不能宣称“回退这两处即消除剩余退化”，也不应通过删除功能来修复。完整各轮数据保存在同名 JSON。

## 共同布局与绘制调用链

1. `Window::draw_roots` 的 request_layout 构建阶段进入 Dock 标签与面板标题；Parameters 的可见参数行有 `.cached(...)`，分组标题却直接调用 `group_header(...)`，随面板每次构建重建按钮。树面板标题每次也构建新增的创建按钮。
2. `Button::render` / `Interactivity` 生成并细化样式、注册布局节点。即使最终 style 没变化，也支付按钮结构复制、样式比较和元素 ID / 状态处理成本；这与 Taffy 实际求解分开。
3. prepaint 经 `splice_prepaint` / `reuse_prepaint` 拷贝保留子树的命中框、元素状态、dispatch tree、文字布局等。保留缓存避免重新 render，但仍有交互和状态数据维护。
4. paint 经 `splice_paint` / `reuse_paint` 重放场景、鼠标监听、元素状态和 tab stops。已有 Warp CPU 栈中的 memmove、SmallVec 扩展、元素状态哈希表、tab-stop SumTree 与这条路径吻合。引擎实现本身在基线与当前之间没有变化。

相关源码：

- `crates/editor/src/panels/parameters.rs:390`：可见行构建，参数行缓存与分组标题直接构建。
- `crates/editor/src/panels/parameters/headers.rs:5`：分组标题，将折叠与名称分成独立按钮。
- `crates/editor/src/panels/hierarchy_tree.rs:261`：新增创建按钮进入工具栏。
- `crates/editor/src/docking/skin.rs:696`：普通面板与 Parameters 的缓存边界。
- `vendor/gpui-fast/src/window.rs:3699`、`:4039`、`:4107`：阶段、prepaint / paint 重用。
- `vendor/gpui-fast/src/fast/splice.rs:603`、`:843`：保留子树局部重绘。

## 后续修复边界

值得验证的优化是保留现有功能，对稳定的分组标题和面板工具栏建立语义状态缓存；只在标题、计数、折叠、选择 / 焦点、disabled、语言 / 主题或相关 authoring 状态变化时更新。只在外层加 cached 而继续在 render 中读取整个 EditorView，仍可能被广泛 owner 通知打破缓存。

在此之前，不应修改 Taffy 求解器、跳过必要的 cached prepaint / paint 数据维护，或承诺能收回上一轮全部 9%～12% P95 差异。当前更可靠的结论是：确认新增元素的每帧维护成本；整帧残余退化的大小与全部来源仍未稳定验证。

## 工件与工作区

- 原始 trace、噪声记录、fixture 校验、阶段与工作量统计、隔离源码、构建日志和脚本：`target/remaining-ui-draw-20261024/`。
- 三个诊断二进制 SHA、每次 trace SHA、模型 / 产品校验和全部轮次数据见同名 JSON。
- 基线切换组件源码时，第一轮构建遇到 facade 缓存不一致；强制重编译 facade 与下游后成功。失败产物未进入采样。禁用 sccache，使用独立 Cargo target，原工作区 release 二进制保持为上轮 `3cc5eb24...`。
- 检查发现工作区 `menus.rs` 精确回到 HEAD，缺少上一轮修复；已备份该状态并恢复已通过测试的同一修复快照。未向产品源码加入诊断埋点或本轮候选优化。
