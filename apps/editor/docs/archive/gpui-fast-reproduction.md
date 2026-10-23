# GPUI Fast 性能复现与 editor 回退原因

> 正式修复现已落地，本文保留修复前的实验数据。正式代码、回归测试与新一轮真实窗口测量见 [正式修复验证](../gpui-fast-cache-boundary-fix.md)。

日期：2026-10-13。结论：**上游的加速方向可以复现；editor 的主要回退来自 Fast 0.1.7 对虚拟列表中显式缓存边界的错误识别。** 修正该识别的实验原型，在不修改 editor UI 的情况下消除了所测 CPU 平移回退。

这是 [升级与全矩阵报告](gpui-fast-benchmarks.md) 的后续调查。前轮“自动视图保留在 editor 中更慢”的测量成立，但不能将它解释成“整个 Fast 后端没有收益”，也不能仅归咎于 editor 的通知范围。

## 版本、环境与边界

- 按要求先在 `../gpui-kit` 执行 `git pull --ff-only`：`201b55a4` → `8173d61c`，分支 main；拉取前后工作区干净。
- Kit 自带基准使用该 checkout，分别不启用和启用 `gpui-fast`。后端为 `gpui-pre 0.3.9` 与 `gpui-fast 0.1.7`。
- 专用性能工具来自 [GPUI Fast 的 gpui_perf](https://github.com/longbridge/gpui-fast/tree/371769f/crates/gpui_perf)，源码 SHA `371769f`。在 Kit 的 `target/fast-reproduction/harness` 中用独立 manifest 编译**未经修改的工具源码**，依赖换为上述已发布的两个后端，均不含 editor 的 Metal 补丁。
- 上游文档的部分原始数字来自 Linux、`gpui-pre 0.3.7`；本机是 Apple M4 / macOS 27.0.1。因此复现的是同机对照的方向与数量级，不宣称逐项复刻原来的绝对时间或百分比。
- editor 仍固定 Kit `507e82b`，包含上一轮升级变更。Kit 新增的 3 个提交分别修改 ResizablePanel fixed、表格列宽保护、Input 垂直滚动约束。本轮没有暗中更换 editor 依赖。
- 全部使用 release。先完成编译，再串行测量。原生 `gpui_perf --auto` 按其默认设置运行 macOS 保持 CPU 时钟的辅助进程；这与 editor 常规运行条件不同，绝对时间不跨工具比较。
- 表格中的“三轮中位数”是每轮估计值或分位数再取中位数，不是将样本合并后计算；CPU 帧耗时不等于 GPU 时间、FPS 或输入至呈现延迟。

## 上游收益是否存在

### Kit 自带的真实组件基准

命令：`cargo bench --locked -p gpui-base --bench text_view_scroll`，与增加 `--features gpui-fast` 的同一基准对照。每轮 Criterion 30 样本、1 秒 warmup、2 秒 measurement，三轮交替执行。以下是每轮 Criterion 时间估计的中位数。

| 负载 | 普通 GPUI | Fast | 耗时下降 |
| --- | ---: | ---: | ---: |
| 长 Markdown 滚动 | 2.230 ms | 0.932 ms | 58.2% |
| 含 inline code 的 Markdown 滚动 | 3.475 ms | 2.485 ms | 28.5% |

这是 Base 的真实 `TextView`、文本布局和帧驱动，不是 editor 的模型求值基准。它测量无节拍限制的工作与渲染，不测显示延迟。短时测试有波动，例如普通 Markdown 三轮估计为 2.433 / 2.230 / 1.599 ms；每一轮 Fast 都更快。

### 官方原生展示程序

同一 `gpui_perf` 源码，9 个场景，各 30 帧 warmup + 300 帧；普通 GPUI 每轮一次，Fast 每轮分别 retention on/off；三轮。下表为主线程 CPU P50 / P95，单位 ms。

| 场景 | 普通 GPUI | Fast、retention off | Fast、retention on |
| --- | ---: | ---: | ---: |
| Spinner | 4.24 / 4.65 | 1.23 / 1.48 | 0.66 / 1.00 |
| ScrollSidebar | 4.29 / 4.59 | 1.32 / 1.81 | 0.65 / 0.92 |
| ScrollPage | 4.30 / 4.63 | 1.31 / 1.86 | 0.21 / 0.78 |
| ScrollTable | 2.72 / 2.95 | 1.18 / 1.63 | 0.48 / 0.67 |
| RefreshTable | 0.04 / 3.18 | 0.04 / 1.47 | 0.04 / 0.73 |
| ScrollList | 2.18 / 2.53 | 0.95 / 1.18 | 0.42 / 1.19 |
| WorkspaceQuotes | 0.60 / 9.80 | 0.34 / 3.71 | 0.70 / 3.52 |
| WorkspaceScroll | 4.65 / 9.71 | 2.47 / 3.71 | 1.89 / 3.44 |
| WorkspaceHover | 4.30 / 9.64 | 2.25 / 3.75 | 1.71 / 3.40 |

侧栏滚动 P50 从 4.29 降到 0.65 ms，降低 84.8%，接近上游 README 的 87% 加速方向。行情刷新 P95 从 9.80 降到 3.52 ms，降低 64.1%，但不是原 Linux 测试的 73%。行情/定时刷新混有几乎不工作的帧，P50 很小且不稳定，不能单看它判断更新帧成本。

**Fast 的收益不全来自自动视图保留。** 例如侧栏在 retention off 时已经从普通 GPUI 的 4.29 降到 1.32 ms；布局节点与文本测量等优化仍存在。行情的 retention off/on P95 仅为 3.71 / 3.52 ms，远小于换后端的收益。这解释了为什么上一轮仅切 retention 的结果不能代表整个 Fast 后端。

### 官方合成与 headless 对照

原样运行上游 `retained_bench.rs`：60 个 panel ×64 个 label，每种配置 200 次迭代，三轮。以下为每轮平均迭代耗时的中位数（合成 TestAppContext，不代表真实窗口呈现）：

| 每轮通知的 panel 数 | retention off | retention on | 下降 |
| --- | ---: | ---: | ---: |
| 0 | 4.749 ms | 0.155 ms | 96.7% |
| 1 | 8.890 ms | 0.646 ms | 92.7% |
| 6 | 9.116 ms | 1.405 ms | 84.6% |
| 60 | 11.813 ms | 8.381 ms | 29.1% |

通知会触发 TestAppContext 的自动绘制，工具还显式 draw，因此通知场景按“迭代”解读，不将它冒充一次 native frame；静止场景没有此额外通知。

另运行 `gpui_perf --headless` 的 form、layout、workspace、scroll、chat 相关场景（三轮、300 帧，原始 JSON 含所有匹配项）。代表性平均 CPU 帧耗时中位数：

| 场景 | retention off | retention on |
| --- | ---: | ---: |
| form-idle | 0.492 ms | 0.079 ms |
| scroll-child-view | 0.523 ms | 0.061 ms |
| workspace-quotes | 1.747 ms | 1.708 ms |
| layout-unchanged | 1.131 ms | 1.103 ms |
| chat-scroll | 0.250 ms | 0.256 ms |

收益有场景条件，不能只选最好的行。`layout-unchanged` 的实现仍每轮 `update + notify` 整个表，不等于上面的零通知窗口。`--verify` 也对本轮所选场景进行 30 帧绘制对照，全部通过，包括工具支持的滚动层 on/off oracle。

## editor 的具体失效链

这里不是泛泛推测“状态太大”或“所有面板都 notify”。通过临时引擎日志、帧计数器和两种独立消融，定位到以下链条：

1. `ParametersPanel` 的 `uniform_list` 返回 `ParameterRowView.cached(...)`。这些行以独立布局根测量，没有 `parent_layout_key`。
2. Fast 的 `Window::rebuild_here` 在父布局 key 缺失时返回 `None`。这只表示该视图不能作为一个独立 gap 插入祖先的保留帧，本来不应该抹掉 `.cached()` 的语义。
3. 但 `RetainedSubtree::is_cached()` 用 `rebuild.as_ref().is_some_and(|r| r.is_cached())` 判断缓存边界。于是实际已缓存的列表行被判成未缓存。
4. `mark_changed_retained_views_dirty` 据此没有维护该行的缓存边界。行内普通 `SlideNumberInput` / `InputState` 在渲染时写入的依赖，被当作需要再次构建的变化，沿参数行、参数面板、Dock 向上传播。
5. Kit `Input::render` 安装 token presentation、style、padding 等时会 `Entity::update`；editor 行渲染也会 `number_input.update(configure)`，参数面板会 `sorting.update(reconcile)`。即使内部发现值没变、不发 notify，Fast 仍会记录 render 期间的写入。这些写入暴露了错误的缓存边界判定。

关键运行日志（8 个可见参数行都出现同样结果）：

```text
CACHE ... parameter-row ... recognized=false rebuild=false layout=true parent=None
```

稳态时 trace 显示 `refreshing=false`，依赖解释为 `written=[InputState / SlideNumberInput / SortState]`，而 `globals=[]`、`states=0`。因此不能将本轮回退归因于每帧全局 Theme 改写或强制 window refresh。启动阶段的 refresh 和全局初始化日志已排除。

相关代码：

- [参数虚拟行和数字输入](../../crates/editor/src/panels/parameters.rs)
- [数字输入的 render](../../crates/editor/src/ui/slide_number_input.rs)
- [Fast 缓存判定与失效传播](../../vendor/gpui-fast/src/fast/retained.rs)
- [Fast rebuild_here](../../vendor/gpui-fast/src/fast/splice.rs)

现有普通容器中的嵌套缓存测试没有覆盖“虚拟列表行缺少父布局 key”的条件，所以之前通过缓存回归测试与这里发现问题不矛盾。

## 两种独立验证

### 引擎原型：缓存身份与可独立重建能力分开保存

不改 editor UI，只给 retained record 独立的 `cache_boundary` 标志，创建、复用和 splice 复制时保留它。实验开关仅改变 `is_cached` 的判定。三模型 ×3 轮交替；每轮 10 次 warmup +100 次平移；以下为各轮 P95 的中位数，ms。

| 模型 | 原判定、retention off | 原判定、retention on | 原型、retention off | 原型、retention on |
| --- | ---: | ---: | ---: | ---: |
| mao | 1.016 | 1.682 | 1.041 | 0.764 |
| shirousagi | 0.886 | 1.477 | 0.855 | 0.664 |
| heijiao | 0.769 | 1.492 | 0.762 | 0.709 |

Mao 开启 retention 时从 1.682 降到 0.764 ms（下降 54.6%）；相对原判定且关闭 retention 的 1.016 ms，原型开启 retention 也快约 24.8%。其余两模型方向一致。

一次带统计的 Mao 对照，100 次测量恰好各产生 100 次 draw：

| 每帧指标 | 原判定、retention on | 原型、retention on |
| --- | ---: | ---: |
| views built | 33.5 | 7.5 |
| views reused | 4.5 | 5.5 |
| Taffy compute 调用 | 22 | 2 |
| 新建布局节点 | 15 | 0 |
| prepaint 平均时间 | 0.548 ms | 0.108 ms |
| scroll layer 合成 | 0 | 0 |

`views_built` 在 retention off 时只覆盖被记录的缓存视图，不能把 off/on 的该计数直接当成全部 Render 次数；这里两列均为 on，口径一致。

### editor 侧绕过：数字输入有自己的显式缓存边界

单独给数字输入 `.cached(StyleRefinement::default().w_full().h_6())`，保持原 Small Input 的高度；引擎不改。三轮 CPU 平移 P95 中位数：

| 模型 | 原 editor、retention on | 数字输入缓存原型、retention on |
| --- | ---: | ---: |
| mao | 1.646 | 0.740 |
| shirousagi | 1.481 | 0.655 |
| heijiao | 1.489 | 0.673 |

抑制排序和 configure 的无变化写入有一定帮助，但无法单独消除回退；数字输入缓存及引擎缓存身份修正都能消除主要成本，形成相互独立的因果证据。

## 正确性、限制与代码状态

- 撤回实验改动后的最终工作区，以及两种最终原型，分别运行 editor headless 交互回归：**150 passed、0 failed、1 ignored**；包含参数编辑、键盘输入、拖动、排序、撤销等既有覆盖。
- 三模型每种 on/off、原始/原型组合的末帧 quad 与 sprite 几何、文本/图像描述、颜色和 clip 一致（分别 268 /259 /281 条；规范化掉 quad draw-order 编号）。这是图元对照，不冒充逐像素截图或所有帧 oracle。
- 第一个数字输入缓存试验遗漏高度，第二个用了不匹配的高度；分别被交互测试和图元对照发现。那些日志保留作排查记录，**不采用它们的加速数作为结论**。上表只采用修正到 `h_6()` 且对照通过的 `exact-*` 数据。
- 此轮修复原型的收益测的是 CPU 输入分发与 UI draw，尚未重新测原型的真实窗口“输入→呈现”延迟。画布仍持续生成和合成 Metal 纹理，scroll-layer 合成计数为 0，不能期待把侧栏滚动的百分比直接搬到模型画布或端到端延迟。
- **本次调查结束时，实验性 UI 改动、引擎探针与临时引擎修改均已撤回。** 后续已用不带实验开关和日志的 [正式 patch](../../vendor/cache-boundary.patch) 落地引擎修复；原实验 patch 仅归档于本机 `target/gpui-fast-upgrade/diagnostics/engine-probe/`，没有混入 `vendor/native-metal.patch`。
- `gpui-kit` main 保持拉取后的干净状态。没有提交、推送、发布或向上游发送消息。

## 复跑与原始数据

Kit 数据在 `../gpui-kit/target/fast-reproduction/`：`run.py`、`runs.json`、`environment.json`、两个后端的二进制、三轮日志与 JSON。`retained-*.log` 是额外三轮原版 retained_bench。editor 数据在 `target/gpui-fast-upgrade/diagnostics/`：

- `trace-idle.log`、`engine-0.log`：依赖写入和错误缓存身份证据。
- `engine-repeat-*`、`engine-summary.json`：引擎原型三模型三轮；文件名 variant 4 表示启用 `GPUI_BENCH_CACHE_BOUNDARY`，并非 editor 数字输入缓存。
- `exact-*`、`exact-summary.json`：尺寸与图元对照通过的 editor 侧原型；variant 4 表示数字输入缓存。
- `engine-tests.log`、`ablation-exact-tests.log`：两个原型的交互验证。
- `engine-probe/editor-engine-probe`、`ablation/editor-ablation`：保留实验二进制；各子目录的 patch 保存对应实验源码。

`target/` 被 Git 忽略，原始数据在本机；报告和正式 patch 可随仓库保存。

```sh
# Kit 自带组件基准，两种后端都应使用 release/bench profile。
cd ../gpui-kit
cargo bench --locked -p gpui-base --bench text_view_scroll -- --sample-size 30 --warm-up-time 1 --measurement-time 2
cargo bench --locked -p gpui-base --bench text_view_scroll --features gpui-fast -- --sample-size 30 --warm-up-time 1 --measurement-time 2

# 普通 editor CPU 基准；临时布局统计探针已清理，模型路径必须是绝对路径。
cd ../kasane-editor
KASANE_BENCH_MODEL="$PWD/models/local/mao/runtime/mao_pro.model3.json" GPUI_VIEW_RETENTION=1 cargo test --locked --release -p kasane-editor --bin kasane-editor canvas_ui_cpu_benchmark -- --ignored --nocapture

# 当前源码已包含正式修复；反向 check 核对补丁与源码一致，不修改源码。
git apply --reverse --check vendor/cache-boundary.patch
```

上游资料：[性能声明](https://github.com/longbridge/gpui-fast/blob/371769f/README.md)、[retained mode](https://github.com/longbridge/gpui-fast/blob/371769f/docs/retained-mode.md)、[测量方法](https://github.com/longbridge/gpui-fast/blob/371769f/CONTRIBUTING.md)、[scroll layers](https://github.com/longbridge/gpui-fast/blob/371769f/docs/scroll-layers.md)。
