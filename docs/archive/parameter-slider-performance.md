# Parameters slider 性能（2026-09-29 / 2026-09-30）

2026-09-29 排查的原始实现每次拖动都构造、布局全部参数行，包括屏幕外的 Slider、标签和按钮。
Mao 有 128 个参数，原始全量列表的 Release UI 绘制 p95 达到 15.33 ms，超过 120 fps 的 8.33 ms 帧预算。
同期模型场景生成 p95 仅 0.12 ms。当前版本的 Debug 排查和缓存失效原因见本文后半部分。

## 最终实现

- 保留工作区已有的 `uniform_list` 虚拟化，只创建可见行的元素，不再每帧复制全量参数。
- 参数行使用独立 Entity 和缓存；Slider 的通知只使对应行失效。缓存使用明确的 3.5 rem 行高，滚动位置在文档结构变化时重置。
- 相同量化步进值和 Release 不重复提交模型更新；成功提交后更新本地值。
- 保持参数更新立即执行。按显示帧延后模型更新的实验增加了延迟，未采用。

## 窗口实测

Apple M4，主屏 3840×2160 / 1920×1080 @ 120 Hz，Release，窗口 1100×740 logical px。
Mao model3，30 次预热、360 次测量，目标输入 120 Hz；隔离配置、关闭测试进程的 Metal HUD 环境变量。
合成鼠标事件经过真实 Slider 命中测试、订阅回调、模型更新及异步 Metal 呈现，不包含 OS 输入送达时间。

基线使用本仓库 HEAD 的原始全量列表，加同样的测量入口；最终版本包含虚拟化、行缓存及重复值过滤。
基线并非本次开始时已经含有虚拟化的未提交版本。源码快照、环境和原始 trace 见下方归档。

| 指标 | 原始全量列表 | 最终版本 |
| --- | ---: | ---: |
| UI draw p50 | 13.97 ms | 2.46 ms |
| UI draw p95 | 15.33 ms | 2.66 ms |
| UI draw p99 | 15.86 ms | 2.79 ms |
| 窗口实际呈现频率 | 65.59 fps | 120.00 fps |
| 不同模型预览呈现频率 | 61.01 fps | 118.33 fps |
| 输入到呈现 p95 | 59.45 ms | 34.14 ms |
| 有效模型请求呈现 | 334 / 360 | 355 / 355 |

最终版本 360 次输入中有 5 次落在相同参数值，未重复生成预览；不同模型预览的间隔 p95 为 8.33 ms，p99 为 16.67 ms。
120 fps 描述呈现节奏，不代表输入到显示的延迟为 8.33 ms，也不保证其他模型、窗口尺寸或 Debug 构建的表现。
仅行缓存相对已虚拟化列表的提升较小；主要收益来自避免全量列表布局。

## 复现

```sh
ZIPZI_BENCH_PARAMETERS=1 scripts/window-benchmark.sh \
  models/local/mao/runtime/mao_pro.model3.json target/parameter-benchmark 360 120
```

不设置 `ZIPZI_BENCH_PARAMETERS` 时，原有 pan / zoom / opacity 工作负载保持不变。
benchmark 将 input scope 保持到 GPUI 完成事件队列 flush，确保 Slider 的异步订阅回调正确关联输入。
报告区分窗口呈现频率和不同模型帧呈现频率；窗口失焦、trace 丢失、测试未完成会标记为无效。

验证包括编辑器单元及 headless 测试、报告脚本测试，并覆盖重复点击/松手不再求值、128 行滚动后的编辑和重置。

测量归档：[原始列表](archive/benchmarks/2026-09-29/parameters/eager/summary.json)、[最终版本](archive/benchmarks/2026-09-29/parameters/optimized/summary.json)。
各目录保留压缩原始 trace、环境和 SHA-256，`sources.json.gz` 保留本次比较的源文件。

## 2026-09-30：Debug 拖动掉帧定位（修复前）

本次针对 Mao，编辑器 commit `92458788`，Kit 固定在 `5059de47`，SDK commit `b37e3b62`。
机器和显示器仍为 Apple M4、主屏 120 Hz，窗口 1100×740 logical px；关闭测量进程的 Metal HUD，使用隔离配置。
全部对照保留 Debug assertions，未修改工作区的运行代码和 Cargo profile。
普通 Debug 构建已恢复，二进制 SHA-256 与排查开始时一致。

### 已确认的执行路径

1. Kit 的 `SliderTrack` 和 `SliderThumb` 使用 `on_drag`，拖动期间存在 GPUI `active_drag`。
   固定版本 `crates/base/src/slider.rs:614`、`:756` 分别注册 `DragSlider` 和 `DragThumb`。
2. GPUI [`Window::dispatch_mouse_event`](../vendor/gpui-pre/src/window.rs) 在 `active_drag` 的每个 MouseMove 后调用 `self.refresh()`（第 5841 行）。
   [`Window::refresh`](../vendor/gpui-pre/src/window.rs) 把 `window.refreshing` 设为 `true`（第 2274 行），使本帧绕过所有 view 缓存。
3. [`prepaint_view`](../vendor/gpui-pre/src/view.rs) 的缓存命中条件要求 `!window.refreshing`。
   基线 59 个测量区间内的完整 UI 帧均为 `refreshing=1, active_drag=1`；每帧重建 6 个可见参数行，
   同时重新调用两个层级树面板及 Properties 面板的 render。虚拟列表仍然有效，没有重建全部 128 行。
4. 单独取消拖动时的全窗口刷新后，两个树面板及 Properties 面板可复用缓存，但参数面板仍每帧重建 6 行。
   Kit 的 `crates/component/src/dock/tab_panel.rs:782` 将整个 panel 包在 `panel.cached(...)` 中；
   子行通知会使祖先失效，外层 panel 重建时 `prepaint_view` 又把 `window.refreshing` 临时设为 `true`（第 501 行），
   因而内部 [`ParameterRowView::cached`](../crates/editor/src/panels/parameters.rs) 也无法命中。

两层缓存失效相互独立。只优化参数行 render 或重复值过滤，无法阻止这条窗口刷新路径。

### 单变量对照

所有实验运行同一份临时诊断源码。`selective_drag` 只把拖动 MouseMove 的全窗口 refresh 改为调度一帧；
`uncached_parameter_panel` 只绕过 ParametersPanel 外层缓存，保留每个参数行的缓存及原来的 GPUI 子树刷新规则。
前三项是 5 次预热、60 次测量；GPUI 编译优化的两项另用 30 次预热、360 次测量确认。

| 配置 | 可见参数行重建 / UI 帧 | UI draw p95 | 窗口呈现 fps | 不同模型帧呈现 fps |
| --- | ---: | ---: | ---: | ---: |
| 普通 Debug，原有行为 | 6 | 19.50 ms | 45.49 | 44.61 |
| 仅取消拖动时的全窗口 refresh | 6 | 15.13 ms | 55.61 | 54.37 |
| 再绕过参数面板外层缓存 | 1 | 10.82 ms | 73.75 | 69.60 |
| 仅将 `gpui-pre` 的 dev `opt-level` 设为 2，保留原有行为 | 6 | 6.39 ms | 118.64 | 82.80 |
| GPUI 优化 + 两项缓存实验 | 1 | 3.11 ms | 119.67 | 93.96 |

最后两项只优化 `gpui-pre`，编辑器、Kit、SDK、其他依赖仍使用普通 dev 设置，GPUI 的 Debug assertions 也保持开启。
这确认了未优化的 GPUI CPU 执行成本是 Debug 与先前 Release 表现不同的重要因素。
窗口呈现 fps 包含重复模型图像，不能据此声称模型预览已达到 120 fps。

基线在连续输入区间内的分段 p95 为：request-layout 1.48 ms、root prepaint 11.56 ms、root paint 5.12 ms、
frame finish 1.47 ms。Taffy 布局时间包含在这些阶段中，不能另加，也不能把各项 p95 相加作为总 p95。
参数 execute p95 为 2.53 ms，scene 生成 p95 为 0.38 ms。
`set_preview_values` 在提交前同步求值并缓存结果，实际求值已计入 execute；仅看 scene 耗时会漏掉这一部分。
参数提交约占 3 ms 的输入处理时间，UI draw 约 19 ms 才是当前最大的主线程开销。

基线模型帧从 published 到 UI 接收的等待 p95 约 18.5 ms；这段等待的累计时间约 98% 与 UI draw 重叠，
说明后台帧大量时间在等主线程结束绘制。drawable 获取 p95 约 0.025 ms，预览 GPU 和合成 GPU 的 p95 分别约 5 ms、6 ms。
这些 GPU 阶段异步执行，不能与主线程时间的分位数直接相加。

### 修复范围与实验限制

可以先在 dev profile 单独优化 GPUI，以减少日常 Debug 的掉帧，并保留 Debug assertions：

```toml
[profile.dev.package.gpui-pre]
opt-level = 2
```

行为层面的修复需要让 slider 拖动避开通用 drag preview 导致的全窗口失效，并处理 ParametersPanel 的嵌套缓存边界。
本次仅验证因果关系，没有合入这些行为改动；通用拖放、hover、滚动及嵌套缓存的正确性还需要专门验证。
直接全局删除 `prepaint_view` 的临时 `refreshing` 实验曾触发文字布局缓存索引越界，已放弃。
表中的成功实验保留该规则，通过仅绕过 ParametersPanel 外层缓存实现每帧更新一行。

360 次优化对照中，窗口呈现接近 120 Hz，但模型帧仍有拒绝或被更新帧替代的情况。
`present_frame` 会拒绝 scene revision 已过期的帧，UI 当前纹理也可能在合成前被新帧替代；这些结果均保留在统计中。
本次没有把所有缓存实验当作完成的性能修复。

### 证据与复现

归档在 [parameters-debug](archive/benchmarks/2026-09-30/parameters-debug/manifest.json)，
包含每项完整 JSONL trace 的 gzip、关联请求 CSV、标准报告、分段报告、环境、源码快照和 SHA-256。
窗口失焦、首帧超时及崩溃的探索运行未纳入上表。
标准报告的唯一 issue 为 `debug build: do not use these numbers for release acceptance`，用于标识 Debug，未忽略其他完整性问题。

在临时源码副本中应用归档的 `diagnostic.patch` 后，使用以下命令；补丁仅供诊断，包含实验开关和启动预热安排：

```sh
cargo build --locked -p zipzi-editor
bash docs/archive/benchmarks/2026-09-30/parameters-debug/run-probe.sh my-debug-baseline 0 0 60 5
bash docs/archive/benchmarks/2026-09-30/parameters-debug/run-probe.sh my-selective-drag 1 0 60 5
bash docs/archive/benchmarks/2026-09-30/parameters-debug/run-probe.sh my-uncached-panel 1 1 60 5

cargo build --locked -p zipzi-editor --config 'profile.dev.package.gpui-pre.opt-level=2'
bash docs/archive/benchmarks/2026-09-30/parameters-debug/run-probe.sh my-optimized-gpui 0 0 360 30
```

## 2026-09-30：正式优化

已在开发构建中单独将 `gpui-pre` 的 `opt-level` 设为 2，保留 Debug assertions。
编辑器的参数轨道和 thumb 改为应用内的鼠标拖动状态，继续调用 Kit 的 `SliderState` 完成映射、步进和 Change / Release 通知；
保留 Base Slider 的语义及 Indicator 的 bounds，鼠标移动不再创建通用 `active_drag`。
ParametersPanel 在 paint 时注册窗口鼠标监听，越出轨道或参数行后仍能继续拖动；松手、失焦、文档结构变化和禁用状态会结束交互。

缓存边界通过 Kit 既有的 `DockAreaRenderer` / `TabGroupRenderer` 接口在编辑器层实现。
`EditorDockSkin` 转发 Component 的 Dock 外观，`EditorTabGroupSkin` 仅将 ParametersPanel 放在未缓存的 absolute / size_full 容器中，
由可见参数行承担缓存；其他面板、tab bar、drop indicator 和空面板继续交给 Kit 的原有 renderer。
此前新增的 GPUI `Render::cacheable()` 策略已撤回，`gpui-pre` 没有源码差异，原有嵌套缓存保护和通用拖放保持原来的行为。

正式实现使用原有窗口 benchmark 测量，无临时诊断开关。Mao、120 Hz、1100×740 logical px、1310×1364 physical 渲染目标、隔离配置和关闭 HUD。
基线使用排查开始时保存的未优化二进制；两项均为 30 次预热 / 120 次测量。
最终实现使用同一 Debug 二进制的临时 macOS app bundle，以避免裸二进制的启动焦点问题；配置继续隔离，未修改运行代码。

| 指标 | 原始 Debug | 正式优化 Debug |
| --- | ---: | ---: |
| UI draw p95 | 19.26 ms | 2.69 ms |
| 窗口呈现 fps | 46.12 | 120.00 |
| 不同模型帧呈现 fps | 45.05 | 117.98 |
| 合成输入到呈现 p95 | 72.90 ms | 34.26 ms |

两次报告均完整、trace 丢失为 0，唯一 issue 是 Debug 不适合作为 Release 验收的标识。
120 次正式输入中 2 次重复值未提交，其余 118 个请求均呈现。
窗口 120 fps 包含重复模型图像；输入延迟统计不包含 OS 输入送达时间。
首次 360 次基线重跑因窗口失焦中断，未用于比较。
应用层替代实现的三次裸二进制尝试分别遇到失焦或首帧超时，也未用于比较；此前使用 GPUI 新增策略的测量属于已撤回方案。

44 项 Rust 测试通过（另有 1 个性能 benchmark 默认忽略），9 项报告脚本测试通过。
新增测试覆盖 thumb 按下不跳值、越界拖动、松手不重复求值、失焦后不恢复拖动，以及应用层 Dock renderer 的布局、子缓存复用和鼠标监听。
Dock 缓存测试使用真实 Component skin，并同时验证指定面板保留行缓存、其他面板保留原来的外层缓存、weak view 正常工作。
既有 128 行滚动、数值输入、重置、联动、文档切换和 native Metal 测试均通过。

本地证据在 `target/parameter-optimization/mao-debug-baseline-2`、`target/parameter-optimization/mao-debug-app-cache-4` 和 `comparison.json`，包含完整 trace、报告及源码和二进制 SHA-256。
可用同一脚本重新测量 Debug；脚本仍默认使用 Release：

```sh
ZIPZI_BENCH_PROFILE=debug ZIPZI_BENCH_PARAMETERS=1 scripts/window-benchmark.sh \
  models/local/mao/runtime/mao_pro.model3.json target/parameter-debug-check 360 120
```

报告脚本对 Debug 返回非零退出码；检查 `summary.json` 的 issues，只有上述 Debug 标识时测量仍完整。
