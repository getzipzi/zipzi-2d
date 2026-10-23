# 原生 UI benchmark 对比 — 2026-10-23

当前版本出现可重复的属性提交与直接 Opacity 编辑回归。Name、Opacity 文本、Enabled 的提交线程 CPU P95 接近或超过基线的两倍；直接 Opacity 路径的新预览呈现频率约下降 43%。动画播放与拖动仍接近 120 Hz 的窗口呈现节奏。这次是 editor 与 2d 两组组合版本的整体比较，不能单独归因到任意一个仓库或具体函数。

## 版本与方法

| 组合 | editor | kasane-2d |
| --- | --- | --- |
| 基线 | `279f569b5eed8eb31926b4898a4ff302cabc0a0f` | `01db56a5e572d9050197a0bf019f22077f68daa7` |
| 当前 | `61371c0e7b82d00df0ee8b02083783e4e17b3517` | `c7f165161ed7d9b15880532fb2ec9b1aa74e6acb` |

两个仓库在构建前均无未提交改动。基线由独立 detached worktree 构建，分别保存二进制；所有测量直接运行保存的二进制，期间不进行编译或测试。两个版本的 benchmark 驱动、vendored GPUI、追踪报告脚本一致。构建使用 `cargo build --locked --release -p kasane-editor --features test-support`，未启用 `bench-instrumentation`。

Apple M4 / 10 核 GPU / 16 GiB，macOS 27.0.1；CB27U1 3840×2160，逻辑 1920×1080，120 Hz。相同 Mao runtime 模型，属性控件均选择 ArtMesh215；每次使用独立配置，默认 Metal mesh overlay、关闭 HUD。模型及纹理哈希、二进制哈希、请求尺寸和 fixture 身份保存在 JSON 与本地 audit 中。

常规负载逐项交错运行两轮：基线→当前、当前→基线。画布和参数滑条追加第三组配对。画布与完整变形器每项 30 次预热、300 次正式输入；参数滑条 30+360，目标 120 Hz。属性 33 项各 3+30，另外对 Name、Opacity 文本、Enabled 各运行一组 5+96。96 次结果单独呈现，因为提交历史长度与 30 次负载不同。动画播放每轮约 10 秒，先播放预热 500 ms；动画画布/数值拖动每轮 2400 次、目标 240 Hz，沿用现有驱动，不排除拖动起始样本。

表内多轮结果为“各轮 P95 的中位数”，频率为各轮频率的中位数；不把它们表述为合并原始样本后的 P95。时间单位为 ms，负变化表示耗时下降。没有进行统计显著性检验，小幅差异仍可能受调度、帧相位和温度影响。

## 可重复的主要差异

| 指标 | 基线 | 当前 | 变化 |
| --- | ---: | ---: | ---: |
| 直接 Opacity：UI draw P95 | 11.19 | 20.18 | +80.4% |
| 直接 Opacity：输入到呈现 P95 | 53.04 | 69.46 | +31.0% |
| 直接 Opacity：新预览呈现 Hz | 89.40 | 50.97 | -43.0% |
| Name：提交 CPU P95（96 次） | 9.81 | 22.22 | +126.6% |
| Opacity 文本：提交 CPU P95（96 次） | 9.71 | 20.05 | +106.6% |
| Enabled：提交 CPU P95（96 次） | 9.34 | 18.44 | +97.4% |
| Part：提交 CPU P95（两轮 30 次） | 23.71 | 52.00 | +119.3% |
| Masks：提交 CPU P95（两轮 30 次） | 21.62 | 40.46 | +87.2% |

直接 Opacity 的三轮 UI draw P95、显示延迟和新预览呈现频率均同方向变化，倒序运行仍可重复。该负载通过原有 `PreviewOpacity` 命令入口执行，未包括 slider 控件分发。当前每轮 300 次正式请求均有呈现；频率下降不能用“只显示少量成功请求”解释，同步工作变慢也会降低实际输入速度。UI draw 是 GPUI 布局/绘制墙钟边界，不能把其全部增量直接认定为纯 CPU 计算。

该 Opacity 负载的场景准备 P95 为 0.48 → 0.50 ms，预览 GPU 为 5.30 → 4.14 ms，窗口合成 GPU 为 6.69 → 6.16 ms。GPU 阶段没有随 UI draw 一同退化，当前证据不足以把这次减速归因为 GPU 执行变慢。

96 次确认样本中，Name / Opacity 文本 / Enabled 的每次交互最慢原生事件 P95 分别为 15.84 → 28.37 / 15.30 → 25.71 / 10.33 → 19.61 ms。提交 CPU 使用线程 CPU 时钟；原生事件耗时为墙钟，排除了脚本主动等待的搜索防抖、弹层和 tooltip 停留。

## 画布、参数与动画

| 负载 | UI draw P95：基线 → 当前 | 输入到呈现 P95：基线 → 当前 | 新预览呈现 Hz：基线 → 当前 |
| --- | ---: | ---: | ---: |
| 平移 | 3.72 → 3.76 | 49.01 → 49.01 | 114.19 → 118.41 |
| 缩放 | 3.99 → 4.17 | 53.02 → 49.13 | 120.00 → 119.61 |
| 直接 Opacity | 11.19 → 20.18 | 53.04 → 69.46 | 89.40 → 50.97 |
| 参数滑条 | 3.56 → 3.45 | 48.76 → 45.95 | 113.37 → 110.89 |

| 动画负载 | UI draw P95：基线 → 当前 | 呈现 Hz：基线 → 当前 |
| --- | ---: | ---: |
| 播放（新动画纹理） | 3.23 → 3.22 | 119.85 → 119.85 |
| 画布持续拖动（窗口帧） | 3.62 → 3.71 | 119.75 → 119.77 |
| 数值持续拖动（窗口帧） | 3.11 → 2.96 | 119.85 → 119.92 |
| 数值持续拖动（新动画纹理） | 3.09 → 2.96 | 109.02 → 107.00 |

动画画布平移复用已有内容，原动画求值报告没有对应的新求值请求，不能把空的 `playback` 字段解释为未绘制。补充报告按实际拖动阶段统计 UI draw 和 native presented 时间戳。数值拖动的窗口合成频率和新动画纹理频率分别列出；它们测量的是不同工作。240 Hz 是请求目标，实际输入频率保存在 `native_phase` 中，不能据此声称达到 240 FPS。

参数滑条存在轮间波动；请求未呈现及 no-request 数量见下表，不能仅根据成功呈现样本的延迟判断体验。平移和缩放暂未观察到与直接 Opacity 相同幅度、同方向的稳定退化。

| 负载 | 版本 | 各轮请求终态（正式采样） |
| --- | --- | --- |
| opacity | baseline round 1 | `{"presented": 298, "rejected": 2}` |
| parameter_slider | baseline round 1 | `{"presented": 351, "accepted_not_composed": 2, "rejected": 2, "no_request": 5}` |
| opacity | current round 1 | `{"presented": 300}` |
| parameter_slider | current round 1 | `{"presented": 305, "accepted_not_composed": 50, "no_request": 5}` |
| opacity | baseline round 2 | `{"presented": 300}` |
| parameter_slider | baseline round 2 | `{"presented": 350, "accepted_not_composed": 1, "rejected": 4, "no_request": 5}` |
| opacity | current round 2 | `{"presented": 300}` |
| parameter_slider | current round 2 | `{"presented": 355, "no_request": 5}` |
| opacity | baseline round 3 | `{"presented": 299, "accepted_not_composed": 1}` |
| parameter_slider | baseline round 3 | `{"presented": 343, "accepted_not_composed": 12, "no_request": 5}` |
| opacity | current round 3 | `{"presented": 300}` |
| parameter_slider | current round 3 | `{"presented": 342, "rejected": 13, "no_request": 5}` |

## 变形器完整手势与共有失败

完整手势配对最终固定 Warp `5101be9b-3d35-45a7-8764-b8d62616ada4`，使用 round 5 / 6；四项拖动均完成 300 次正式输入。Mesh 与 Rotation 保留驱动的原有选择规则；两组版本的 fixture 对象、组件及受影响顶点数在 audit 中逐项保存。

| 负载 | UI draw P95：基线 → 当前 | 输入到呈现 P95：基线 → 当前 | 新预览呈现 Hz：基线 → 当前 |
| --- | ---: | ---: | ---: |
| mesh_single_vertex | 3.64 → 3.45 | 47.35 → 38.41 | 111.21 → 116.45 |
| mesh_multi_vertex | 3.56 → 3.41 | 29.10 → 34.92 | 114.43 → 106.47 |
| warp_deformer | 2.94 → 2.35 | 40.08 → 39.87 | 108.78 → 116.31 |
| rotation_deformer | 2.74 → 2.80 | 42.44 → 37.96 | 119.20 → 118.61 |

阶段指标仍有局部回归：Warp 同步输入处理 P95 为 1.03 → 2.18 ms（约 +112%），两轮均同方向变化；同时 scene 为 0.90 → 0.72 ms、预览 GPU 为 4.18 → 4.03 ms。不能因 UI draw 更快或整体延迟接近而忽略输入处理增量。多顶点拖动的显示延迟和频率轮间波动较大，逐轮数据保留，当前不足以确认稳定端到端退化。

默认 Warp59（`36d63e33-4a9e-4613-8ab4-644f7ed85041`）两组版本均在正式 Warp 输入前报 `The parent deformation is singular at this point`。Warp58（`384790bd-1caf-4ceb-8ad0-a125fb849cbc`）的 3+30 短测两组均通过，但 30+300 完整手势均在约第 212 个正式输入处报 `Cannot map this drag through the parent deformation`。这些失败逐项保留，整次运行排除出性能比较；短探针也不用于完整手势的结论。其他目标尝试与结果在机器可读报告中。

## 属性面板全部 33 项

下表为两轮每项 30 次的 P95 中位数。`—` 表示该阶段没有样本，例如只改变 live preview 的拖动没有同步 commit；它不代表已测得零耗时。UI draw 统计整个窗口的 CPU 布局/绘制墙钟；Commit CPU 为该阶段同步属性提交线程 CPU 时间。

| 项目 | UI draw：基线 → 当前 | 最慢原生事件/交互：基线 → 当前 | Commit CPU：基线 → 当前 |
| --- | ---: | ---: | ---: |
| name | 6.83 → 7.07 | 13.45 → 23.67 | 8.11 → 18.82 |
| runtime_id | 6.83 → 7.07 | 24.35 → 39.88 | 17.28 → 32.25 |
| part | 6.25 → 6.19 | 6.91 → 6.90 | 23.71 → 52.00 |
| masks | 6.38 → 6.32 | 20.50 → 36.27 | 21.62 → 40.46 |
| opacity_text | 7.73 → 7.87 | 13.00 → 23.40 | 8.15 → 17.93 |
| opacity_drag | 6.57 → 6.97 | 0.04 → 0.04 | — |
| multiply_hex | 9.14 → 8.86 | 9.89 → 11.79 | — |
| color_blend | 7.23 → 7.56 | 6.84 → 6.90 | 16.67 → 34.35 |
| enabled | 9.20 → 7.82 | 10.69 → 18.78 | 9.18 → 17.73 |
| picker_open_close | 9.51 → 9.30 | 8.84 → 8.61 | — |
| picker_tabs | 7.32 → 6.50 | 0.09 → 0.09 | — |
| picker_hex | 6.65 → 6.58 | 6.38 → 6.29 | — |
| picker_invalid_hex | 6.06 → 5.83 | 7.17 → 6.61 | — |
| picker_hue | 7.68 → 8.16 | 0.07 → 0.10 | — |
| picker_sv | 7.48 → 8.11 | 0.07 → 0.09 | — |
| picker_rgb_0_text | 7.20 → 7.43 | 6.23 → 6.31 | — |
| picker_rgb_1_text | 7.13 → 7.54 | 6.53 → 6.53 | — |
| picker_rgb_2_text | 7.38 → 7.42 | 6.32 → 6.45 | — |
| picker_hsv_0_text | 7.18 → 7.92 | 6.03 → 8.14 | — |
| picker_hsv_1_text | 6.74 → 6.92 | 6.46 → 6.28 | — |
| picker_hsv_2_text | 7.27 → 6.85 | 7.77 → 6.28 | — |
| picker_rgb_0_drag | 7.78 → 7.89 | 0.06 → 0.06 | — |
| picker_rgb_1_drag | 7.78 → 8.05 | 0.06 → 0.06 | — |
| picker_rgb_2_drag | 8.48 → 7.51 | 0.07 → 0.06 | — |
| picker_hsv_0_drag | 8.39 → 8.00 | 0.06 → 0.06 | — |
| picker_hsv_1_drag | 8.46 → 7.58 | 0.06 → 0.06 | — |
| picker_hsv_2_drag | 6.99 → 7.06 | 0.07 → 0.06 | — |
| picker_done | 8.86 → 9.32 | 8.69 → 8.35 | — |
| picker_cancel | 8.18 → 8.22 | 8.52 → 8.18 | — |
| multiply_reset | 8.47 → 9.30 | 9.93 → 10.96 | — |
| picker_swatches | 8.25 → 8.31 | 0.32 → 0.31 | — |
| idle | — | 0.00 → 0.00 | — |
| picker_idle | — | 0.00 → 0.00 | — |

所有正式属性输入都有 `properties_checked` 校验；需要改变数据的用例均实际改变属性或 live preview。两轮两个版本的 `idle` 和 `picker_idle` 均为 0 次 UI draw、0 次模型预览请求；仍可能有缓存窗口合成，不能声称 GPU 零工作。33 项每项只有 30 个输入，不适合用单轮 P99 判断罕见卡顿；原始每轮 P50/P95/P99/max、超预算数量、GPU 指标、请求终态均保存在 JSON。

## 完整性、限制与复现

本地 audit 共记录 40 次完整运行（其中 2 次为短探针，不用于主比较）、8 次无效运行；完整运行合计 36,336 次正式 benchmark/property ticks 与 drag samples，不把按时间驱动的 playback 请求数混入这个总数。有效运行均包含完成标记、唯一 end 记录、0 个丢失追踪事件；报告无 issues，未发现 GPU command、render 或 delivery 错误。27 项 Python 追踪报告测试通过。未修改产品源码，也未运行全量 Rust 测试套件。

这些结果来自单台机器、单个 Mao 模型、开启追踪和 test-support 的 Release 构建。自动输入从 GPUI 分发入口开始，不包括硬件/OS 输入送达、屏幕扫描与像素响应；`input_to_present` 不是 input-to-photon。120 Hz 的 8.33 ms 是单帧参考预算，不能把跨多个阶段的输入到显示延迟直接视为单帧 CPU 成本。两仓库同时升级，这次只证明版本组合间存在差异；应对同步属性提交/发布和直接 Opacity 更新路径进一步 profile，再分别隔离 editor 与 2d 的贡献。

- [机器可读完整结果](2026-10-23-ui-comparison.json)：所有逐轮摘要、有效性、CPU/GPU 分位数、请求终态、native phase、哈希及对比数据。
- 本地原始目录：`target/ui-benchmark-20261023-279f569/`，保留保存的二进制、基线工作树、构建日志、逐请求 CSV、JSONL、配置、环境、驱动脚本与失败记录。
- 本地 `run.py` 为两轮交错驱动，`followup.py` 为 96 次属性/第三轮画布及最初固定目标验证，`deformation_followup.py` 为完整手势目标选择及第三轮参数，`native_phase.py` 补充动画窗口阶段，`audit.py` / `summarize.py` / `write_report.py` 负责审计与归档。所有原始 trace 均拒绝覆盖。

单次复现继续使用原项目入口（两个版本都使用相同开关和模型）：

```sh
env -u KASANE_DEV_SESSION_DIR ./scripts/window-benchmark.sh \
  models/local/mao/runtime/mao_pro.model3.json target/ui-repro-canvas 300 120
env -u KASANE_DEV_SESSION_DIR KASANE_BENCH_PROPERTY_CASE=name KASANE_BENCH_WARMUP=5 \
  ./scripts/properties-benchmark.sh target/ui-repro-name 96 120
```

完整变形器复现：

```sh
env -u KASANE_DEV_SESSION_DIR KASANE_BENCH_DEFORMATIONS=1 KASANE_BENCH_WARP=5101be9b-3d35-45a7-8764-b8d62616ada4 \
  ./scripts/window-benchmark.sh models/local/mao/runtime/mao_pro.model3.json \
  target/ui-repro-deformations 300 120
```
