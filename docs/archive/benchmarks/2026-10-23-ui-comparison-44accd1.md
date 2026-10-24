# 原生 UI benchmark：44accd1 基线对比 — 2026-10-23

按用户要求保留后台任务完成本次报告。50 个正式进程全部通过 trace 完整性检查，但后台 Rust／Xcode 编译多次出现，**本次只能做受干扰条件下的描述性对照，不能确认稳定优化或回归**。Name／Opacity 文本／Enabled 提交 CPU P95 的跨轮中位数分别变化 −5.9%／−1.0%／−2.6%；多数 UI draw 指标偏高，其中动画数值拖动约 +20.6%、Warp 约 +30.2%、Rotation 约 +45.6%，应在无并行编译时复核。

后续已增加 Physics、精简 Mesh 并复测受干扰项目，见[后续复测报告](2026-10-23-physics-ui-retest-44accd1.md)；本页保留原始受干扰结果。

## 版本与方法

| 组合 | kasane-editor | kasane-2d |
| --- | --- | --- |
| 基线 | `44accd1e071c5834ca2bc97c833bbd3bbbfc5bba` | `b86b13b10b8aef7346b71009c36aa59ef47b9cf7` |
| 当前 | `b4e4abc737f88bdb895dfec960444dd3183369b7` | `e27230ab2fed1df4b4d0743d28fb1ae3fca3405e` |

两个仓库构建前均干净。基线从指定提交归档到独立的配对源码目录，当前直接使用工作区；两者均以 `cargo build --locked --release -p kasane-editor --features test-support` 构建，未启用 `bench-instrumentation`。分别保存二进制并记录 SHA-256，本驱动在全部构建结束后串行运行已保存二进制，没有同时启动编译或测试；其他后台任务的编译干扰见下节。两版 benchmark 驱动、追踪报告脚本、vendored GPUI 和 Cargo.lock 完全一致。

Apple M4（10 核 GPU）、16 GiB、macOS 27.0.1；CB27U1 3840×2160、逻辑 1920×1080、120 Hz。同一 Mao runtime 模型和纹理，默认 Metal mesh overlay，关闭 HUD，每进程隔离配置。用户在采样前已暂停后台 Editor 和 Rive；每进程开始时保存后台进程和热状态快照，其他后台应用仍可能影响调度。

前两轮逐负载交错，版本顺序为“基线→当前”“当前→基线”；画布、参数及重点属性补充第三轮“基线→当前”。画布每种负载、四种变形器手势均为 30 次预热＋300 次正式输入，参数为 30＋360，目标 120 Hz。属性 33 项各 3＋30；Name／Opacity 文本／Enabled 另测三轮 5＋96。动画播放约 10 秒（预热 500 ms）；动画画布和数值连续拖动各 2400 次、目标 240 Hz，沿用原驱动的起始样本口径。

所有表均为每进程分位数或频率的跨轮中位数，没有合并原始样本。单位 ms；负变化表示耗时下降。未进行显著性检验，小幅变化需结合逐轮方向及调度、帧相位、温度波动解读。CPU 是提交线程 CPU；UI draw 是布局和绘制墙钟；新纹理呈现 Hz 与窗口合成 Hz 分别报告。

## 后台干扰与解释限制

主比较的 50 个进程只有运行开始时的进程／热状态快照。以下 6 个快照已发现后台编译器；编译也可能在其他进程运行中途启动，因此不能将其余进程自动视为干净样本。后台任务包括另一个 Codex 启动的 Cargo check，以及 Xcode 编译。原始结果全部保留，未按结果好坏筛选或删除。

| 轮 | 版本 | 负载 | 开始快照中的编译进程数 |
| --- | --- | --- | ---: |
| round-1 | current | parameters | 2 |
| round-1 | current | playback | 2 |
| round-2 | baseline | playback | 11 |
| round-2 | baseline | properties | 11 |
| round-2 | current | animation_canvas | 13 |
| round-2 | current | playback | 13 |

曾尝试重新测量一组参数负载，并每秒检查后台编译器；基线完成时未观察到编译，但当前版本运行中又出现 rustc。用户随后明确要求保留后台任务、按受干扰结果出报告，因此停止复测。这两个诊断进程共 720 次输入，保存在 diagnostic-rerun 中，不纳入主比较的 50 个进程／37,248 次输入。

以下变化列均为观测差值，不是已经隔离系统噪声后的版本收益或回归。两轮播放的 UI draw P95 中，基线从 2.880 升到 7.079 ms，当前从 3.777 升到 9.192 ms；第二轮双方均有编译器活动，不能忽略这一环境变化。

## 重点属性提交

| 指标 | 基线 | 当前 | 变化 |
| --- | ---: | ---: | ---: |
| Name：提交 CPU P95 | 6.726 | 6.327 | -5.9% |
| Opacity 文本：提交 CPU P95 | 9.077 | 8.983 | -1.0% |
| Enabled：提交 CPU P95 | 6.594 | 6.424 | -2.6% |

逐轮提交 CPU P95 与对应轮变化：

| 属性 | 基线三轮 | 当前三轮 | 逐轮变化 |
| --- | --- | --- | --- |
| name | 6.726／6.581／6.824 | 6.243／6.327／6.367 | -7.2%／-3.9%／-6.7% |
| opacity_text | 9.077／9.449／8.877 | 9.051／8.964／8.983 | -0.3%／-5.1%／+1.2% |
| enabled | 6.594／8.448／6.550 | 6.424／6.324／6.845 | -2.6%／-25.2%／+4.5% |

## 画布、参数与变形器

| 负载 | UI draw P95 | 输入处理 P95 | 输入到呈现 P95 | 新预览呈现 Hz |
| --- | ---: | ---: | ---: | ---: |
| 平移 | 3.580 → 3.967 | 0.051 → 0.052 | 40.497 → 40.754 | 119.202 → 119.999 |
| 缩放 | 3.640 → 4.053 | 0.049 → 0.044 | 48.492 → 48.161 | 119.999 → 119.999 |
| 直接 Opacity | 3.613 → 3.718 | 0.597 → 0.514 | 37.435 → 40.710 | 118.795 → 118.803 |
| 参数滑条 | 3.425 → 3.740 | 0.922 → 0.642 | 40.010 → 40.728 | 116.363 → 117.999 |
| 单顶点 | 3.406 → 3.887 | 0.662 → 0.644 | 32.847 → 37.076 | 118.415 → 108.712 |
| 多顶点 | 3.273 → 4.016 | 0.800 → 0.640 | 37.360 → 40.719 | 112.800 → 119.400 |
| Warp | 2.396 → 3.121 | 2.263 → 2.113 | 40.030 → 40.512 | 109.439 → 115.620 |
| Rotation | 2.502 → 3.644 | 0.861 → 1.080 | 40.874 → 39.057 | 112.225 → 117.615 |

直接 Opacity 通过既有 `PreviewOpacity` 命令入口，不包括 slider 控件分发。变形器固定 Warp `5101be9b-3d35-45a7-8764-b8d62616ada4`，Mesh 和 Rotation 使用两版一致的原选择规则；对象、组件和顶点数见 JSON audit。属性控件均选择 ArtMesh215（`dab3e07e-c588-4d0b-a8ff-de896174d04d`）。

## 动画

| 负载 | UI draw P95 | 呈现 Hz |
| --- | ---: | ---: |
| 播放（新动画纹理） | 4.979 → 6.484 | 115.241 → 114.563 |
| 画布连续拖动（窗口帧） | 2.983 → 4.944 | 119.974 → 119.949 |
| 数值连续拖动（窗口帧） | 2.871 → 3.463 | 119.974 → 119.923 |
| 数值连续拖动（新动画纹理） | 2.873 → 3.472 | 103.092 → 105.089 |

动画画布平移复用内容，空的 playback 求值分组不表示没有窗口绘制；使用实际拖动阶段的窗口 CPU／GPU 和 presented 时间戳补充报告。240 Hz 是输入请求目标，实际输入频率见 native_phase，不代表 240 FPS。

## 属性面板全部 33 项

以下每项两轮、每轮 30 次，不能与 96 次确认负载混合；提交历史长度不同。`—` 表示没有该类计时样本。

| 属性 | UI draw P95 | 最慢原生事件/交互 P95 | 提交 CPU P95 |
| --- | ---: | ---: | ---: |
| name | 8.037 → 8.519 | 11.136 → 11.461 | 5.560 → 4.922 |
| runtime_id | 8.364 → 8.538 | 34.982 → 32.392 | 27.340 → 24.156 |
| part | 6.987 → 7.042 | 7.991 → 7.677 | 44.127 → 41.335 |
| masks | 6.665 → 7.324 | 35.236 → 34.436 | 38.017 → 36.014 |
| opacity_text | 8.445 → 9.552 | 12.472 → 14.978 | 7.360 → 7.746 |
| opacity_drag | 6.924 → 8.221 | 0.036 → 0.033 | — |
| multiply_hex | 9.427 → 10.350 | 10.000 → 10.244 | — |
| color_blend | 7.395 → 8.055 | 6.582 → 7.689 | 24.729 → 24.949 |
| enabled | 7.667 → 8.415 | 6.126 → 5.878 | 5.046 → 4.888 |
| picker_open_close | 9.524 → 10.632 | 8.948 → 9.295 | — |
| picker_tabs | 5.819 → 6.776 | 0.076 → 0.062 | — |
| picker_hex | 7.128 → 7.582 | 7.204 → 7.384 | — |
| picker_invalid_hex | 5.814 → 6.894 | 6.637 → 7.601 | — |
| picker_hue | 7.786 → 11.601 | 0.068 → 0.114 | — |
| picker_sv | 7.266 → 9.373 | 0.070 → 0.107 | — |
| picker_rgb_0_text | 7.640 → 10.869 | 6.380 → 11.037 | — |
| picker_rgb_1_text | 9.734 → 9.270 | 9.190 → 9.078 | — |
| picker_rgb_2_text | 8.692 → 9.801 | 10.395 → 9.799 | — |
| picker_hsv_0_text | 8.695 → 10.717 | 9.698 → 11.240 | — |
| picker_hsv_1_text | 8.224 → 9.892 | 8.708 → 10.553 | — |
| picker_hsv_2_text | 9.022 → 11.063 | 9.330 → 11.968 | — |
| picker_rgb_0_drag | 9.746 → 10.225 | 0.075 → 0.074 | — |
| picker_rgb_1_drag | 11.245 → 10.527 | 0.083 → 0.086 | — |
| picker_rgb_2_drag | 11.157 → 10.553 | 0.080 → 0.072 | — |
| picker_hsv_0_drag | 10.492 → 10.507 | 0.069 → 0.066 | — |
| picker_hsv_1_drag | 9.122 → 11.405 | 0.091 → 0.106 | — |
| picker_hsv_2_drag | 11.252 → 10.290 | 0.084 → 0.082 | — |
| picker_done | 11.242 → 12.393 | 11.378 → 10.845 | — |
| picker_cancel | 10.552 → 11.231 | 11.631 → 11.145 | — |
| multiply_reset | 10.395 → 11.184 | 12.027 → 12.679 | — |
| picker_swatches | 7.895 → 9.347 | 0.241 → 0.276 | — |
| idle | — | 0.000 → 0.000 | — |
| picker_idle | — | 0.000 → 0.000 | — |

## 请求终态与完整性

分位数只统计拥有对应边界的请求；未呈现请求保留。下表逐轮列出画布和参数终态，完整属性 preview_outcomes、动画 outcomes 和 native phase 关联结果见 JSON。

| 负载 | 版本 | 轮 | 请求终态 |
| --- | --- | ---: | --- |
| canvas/opacity | baseline | 1 | `{"presented": 297, "submitted_not_presented": 3}` |
| canvas/pan | baseline | 1 | `{"presented": 300}` |
| canvas/zoom | baseline | 1 | `{"presented": 314}` |
| deformations/mesh_multi_vertex | baseline | 1 | `{"presented": 300}` |
| deformations/mesh_single_vertex | baseline | 1 | `{"presented": 300}` |
| deformations/rotation_deformer | baseline | 1 | `{"presented": 300}` |
| deformations/warp_deformer | baseline | 1 | `{"accepted_not_composed": 44, "presented": 256}` |
| parameters/parameter_slider | baseline | 1 | `{"accepted_not_composed": 1, "no_request": 5, "presented": 354}` |
| canvas/opacity | current | 1 | `{"presented": 296, "rejected": 4}` |
| canvas/pan | current | 1 | `{"presented": 300}` |
| canvas/zoom | current | 1 | `{"presented": 310}` |
| deformations/mesh_multi_vertex | current | 1 | `{"presented": 300}` |
| deformations/mesh_single_vertex | current | 1 | `{"presented": 300}` |
| deformations/rotation_deformer | current | 1 | `{"accepted_not_composed": 2, "presented": 298}` |
| deformations/warp_deformer | current | 1 | `{"accepted_not_composed": 1, "presented": 299}` |
| parameters/parameter_slider | current | 1 | `{"no_request": 5, "presented": 355}` |
| canvas/opacity | baseline | 2 | `{"accepted_not_composed": 71, "presented": 229}` |
| canvas/pan | baseline | 2 | `{"presented": 300}` |
| canvas/zoom | baseline | 2 | `{"presented": 310}` |
| deformations/mesh_multi_vertex | baseline | 2 | `{"presented": 300}` |
| deformations/mesh_single_vertex | baseline | 2 | `{"presented": 300}` |
| deformations/rotation_deformer | baseline | 2 | `{"accepted_not_composed": 35, "presented": 265}` |
| deformations/warp_deformer | baseline | 2 | `{"accepted_not_composed": 4, "presented": 296}` |
| parameters/parameter_slider | baseline | 2 | `{"accepted_not_composed": 28, "no_request": 5, "presented": 327}` |
| canvas/opacity | current | 2 | `{"accepted_not_composed": 1, "presented": 299}` |
| canvas/pan | current | 2 | `{"presented": 300}` |
| canvas/zoom | current | 2 | `{"presented": 310}` |
| deformations/mesh_multi_vertex | current | 2 | `{"presented": 300}` |
| deformations/mesh_single_vertex | current | 2 | `{"presented": 300}` |
| deformations/rotation_deformer | current | 2 | `{"accepted_not_composed": 4, "presented": 296}` |
| deformations/warp_deformer | current | 2 | `{"accepted_not_composed": 15, "presented": 285}` |
| parameters/parameter_slider | current | 2 | `{"accepted_not_composed": 60, "no_request": 5, "presented": 295}` |
| canvas/opacity | baseline | 3 | `{"accepted_not_composed": 1, "presented": 299}` |
| canvas/pan | baseline | 3 | `{"presented": 300}` |
| canvas/zoom | baseline | 3 | `{"presented": 310}` |
| parameters/parameter_slider | baseline | 3 | `{"accepted_not_composed": 2, "no_request": 5, "presented": 353}` |
| canvas/opacity | current | 3 | `{"accepted_not_composed": 1, "presented": 299}` |
| canvas/pan | current | 3 | `{"presented": 300}` |
| canvas/zoom | current | 3 | `{"presented": 310}` |
| parameters/parameter_slider | current | 3 | `{"no_request": 5, "presented": 355}` |

50 个进程全部完成，报告 issues 为空、唯一 end 且 lost_events=0。正式 benchmark/property ticks 与连续拖动 samples 共 37,248 次；按时间驱动的播放请求不混入该计数。每个属性输入均有 properties_checked；重点属性每次均实际改变数据，commit CPU 样本数与输入数一致。这里“通过”仅指数据完整性，不能作为性能环境合格结论。四个完整属性进程的 idle／picker_idle 均为零次 UI draw、零次模型预览请求。27 项报告工具测试通过。本次未修改产品源码，未运行全量 Rust 测试。

两个仓库同时升级且后台存在编译干扰，不能将差异单独归因于 Editor、SDK 或具体函数。仅覆盖上述 Mao 原生窗口负载；新增 U2–U6 创作流程及独立 Timeline headless/SDK editing-chain 不在本次性能结论范围内。自动输入从 GPUI 入口开始，不包括硬件／OS 输入送达和屏幕扫描／像素响应；输入到呈现不是 input-to-photon，也不能等同于单帧 CPU 时间。

## 证据与复现

- [完整机器可读数据](2026-10-23-ui-comparison-44accd1.json)：版本、二进制／资产／脚本哈希、逐轮 P50／P95／P99／max、CPU／GPU 阶段、请求终态、有效性和后台进程快照。
- 本地原始目录：`target/ui-comparison-44accd1-20261023/`，包含配对基线源码、两份二进制、构建日志、环境信息、逐请求 CSV／JSON、原始 trace、配置、审计与汇总脚本。

```sh
python3 target/ui-comparison-44accd1-20261023/run_all.py
python3 target/ui-comparison-44accd1-20261023/finalize.py
python3 target/ui-comparison-44accd1-20261023/write_report.py
```

驱动跳过已有完整结果，拒绝覆盖已有 trace。重新采样应复制驱动到新的输出目录，保留相同的已保存二进制与哈希；要验证性能差异，需先停止并行编译并持续监测。rerun_clean.py 为本次中止的环境诊断尝试，不作为主复现入口。
