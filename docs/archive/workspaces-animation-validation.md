# Animation workspace 首版验收

日期：2026-10-12。阶段一至五功能验收通过。代表性 Mao 模型在下述机器上达到平均约 60 Hz 的原生窗口播放，仍有呈现节奏抖动；不把该结果解释为所有模型、硬件或每一帧都满足 16.67 ms。

## 环境与版本

- Apple M4，10 核 GPU，16 GiB 内存，Mac16,10；macOS 27.0.1（26A434）。
- 主显示器 3840 × 2160，逻辑 1920 × 1080 / 120 Hz；镜像副显示器为 75 Hz。没有打开 Metal HUD。
- release 原生窗口和 headless 性能测试，窗口逻辑尺寸 1100 × 740、scale factor 2；测试截图为 2200 × 1480。全量回归使用 debug。
- 编辑器基线为第四阶段提交 `b1c546f9101d31b6455cad03965011cbd3873835`，加第五阶段工作区改动；相邻 kasane-2d 为 `c90bdc5f2f00b540c87635745d0f3a4f66dd97bb`，本阶段没有 SDK 改动。

## 实际模型覆盖

| 本地模型 | drawable | 带遮罩的 drawable | 原生 Offscreen | motion / expression | Physics / Pose |
| --- | ---: | ---: | ---: | --- | --- |
| `mao/runtime/mao_pro.model3.json` | 260 | 37 | 0 | 7 / 8 | 均有 |
| `Shirousagi/Shirousagi.model3.json` | 24 | 2 | 0 | 0 / 0 | 均无 |
| `heijiao/heijiao.model3.json` | 151 | 0 | 0 | 0 / 0 | 均无 |

三个本地模型均通过 `animation_acceptance` 的真实编辑器链路：导入保持 Modeling → 调整一个参数试摆并隐藏一个对象 → 切 Animation → 确认模型默认参数与运行时可见性 → 切换文档后返回 → 切回 Modeling → 建模像素、参数、history 和 modified 恢复基线 → 保存、关闭并重新打开项目 → 资源数量保留、文档干净、预览暂停。

Mao 另外覆盖：循环超过动作时长仍保留绝对时钟；回到起点；非循环动作中依次切换两个表情；1.5 s → 0.25 s → 1.5 s 的完整 snapshot 和 Metal 画布像素相同；清除表情立即改变图像；动作到达结尾自动暂停并钳制到时长。Physics / Pose 全程使用 SDK 的固定步进与调度。两个无动画资源模型正确显示空状态并禁用播放。

Mao 原始动作均循环，因此测试只在临时导入文档中把第二个动作设为非循环。本地模型都没有原生 Offscreen；Metal 测试在临时 Mao 文档的根 Part 上增加一个 Kasane Offscreen，并导入常量 ModelOpacity 动作。源 model3、motion、纹理没有修改。这覆盖实际纹理、遮罩及几何的合成路径，不代表已经验证所有原生 Cubism Offscreen 资源。

真实 Mao 派生文档与小型重叠 fixture 都在 opacity 0 / 0.5 / 1 与 `kasane-sdk-observe` 实际 Metal 图像比较，最大通道差不超过 **1/255**；零透明度全部透明，0.5 与 1 图像不同，且 0.5 结果可区分于最终合成图像整体乘 0.5。整体透明度在逐 drawable、mask / Offscreen 合成前应用一次，保留 Part 和 Offscreen 各自透明度。

Mao 投影得到 15 条资源诊断，包含未解析目标等已知限制。验收要求它们在资源面板中可见，没有把“能播放”解释成全部通道兼容。Sound 仍只诊断、不播放音频。

## 原生窗口播放

`animation-window-benchmark.sh` 使用应用自己的 16 ms 定时器、后台求值器、Metal worker 和系统 compositor。选择 Mao 的第一个循环动作，预热 500 ms 后记录约 10 s。以下是最终原生运行 `target/animation-native-phase5-final/summary.json` 的数据。

| 指标 | p50 | p95 | p99 | 最大 |
| --- | ---: | ---: | ---: | ---: |
| CPU 动画求值（ms） | 1.263 | 1.511 | 1.625 | 2.209 |
| CPU Metal 提交（ms） | 1.093 | 1.405 | 1.545 | 1.854 |
| Metal GPU 执行（ms） | 3.984 | 5.260 | 6.024 | 6.355 |
| 请求入队至 UI 接受纹理（ms） | 2.450 | 2.887 | 3.115 | 10.946 |
| UI 接受帧间隔（ms） | 16.662 | 18.094 | 18.523 | 25.254 |
| 系统实际呈现新纹理间隔（ms） | 16.667 | 25.000 | 25.000 | 33.334 |

稳态 **601 次请求全部被接受，601 个不同的动画纹理实际呈现**，接受频率 59.992 Hz，实际呈现频率 **59.949 Hz**。CPU 和 Metal 的待处理槽位峰值均为 **1**；trace 丢失事件为 0，GPU command 均成功完成。启动、切换和结束阶段合计有一次 CPU 取消、一次渲染合并、一次旧帧拒绝，稳态没有持续拒帧。

CPU 求值和 GPU 执行均在 16.67 ms 预算内。剩余波动表现在原生定时器／呈现间隔；当前数据不能独立确定每次抖动的来源。保留平均 60 Hz 的结果和 p95 / 最大间隔，不通过改变 Physics / Pose 步长或丢弃物理状态来掩盖抖动。

“UI 接受”表示采用共享队列中已提交的纹理，GPU 完成由 Metal 时间戳单独记录；UI 接受时 GPU 可以仍在执行，因此该耗时可能小于 GPU 执行时间。“实际呈现”按每次纹理发布后的 `compose_surface` 与系统 `presented` 关联，取该纹理首次呈现，处理纹理指针复用；它不是输入到光子的端到端测量。不同阶段存在流水线重叠，不能把表中 p95 直接相加。

## 长 seek、取消与内存

headless 性能脚本使用真实 EditorView、CPU / Metal worker 和 GPU 画布截图，以测试时钟按 60 Hz 发出目标；没有原生 compositor，不能用它的接受率冒充显示刷新率。它同时执行三个模型的完整场景，并单独记录分数时间 seek 的 checkpoint 数量和预算。

缓存和长 seek 使用 SDK 默认的 1 s checkpoint 间隔、**16 MiB** 保留预算。`SeekCacheStats` 是 checkpoint 估算值，不包含共享文档与单次 seek 的临时状态。连续 canonical advance 不主动生成 seek checkpoint，原生稳态 trace 中 checkpoint 为 0 属于预期。纹理缓存沿用 **128 MiB** 的淘汰预算，当前活动场景资源可以超过此值；该预算不是整个进程或 GPU 内存的硬上限。

CPU 和 Metal worker 各保留一个最新待处理请求，结果通道容量各为 1，Metal 仅允许一个 command 在途。seek / 文档切换提高接受屏障，过期 CPU 工作在 canonical 步之间取消，旧 GPU 结果被身份校验拒绝；持续播放允许落后于当前时钟的有效帧被接受。

完整测量数值保存在 `target/animation-benchmark-phase5-final/results.json` 与 `summary.json`，下表记录最终复测。

| seek 目标（s） | 求值耗时（ms） | checkpoint 数量 | 估算保留字节 |
| --- | ---: | ---: | ---: |
| 20.125 | 80.344 | 20 | 6,411,600 |
| 40.125 | 79.269 | 40 | 12,816,480 |
| 80.125 | 149.472 | 52 | 16,652,688 |
| 120.125 | 137.938 | 52 | 16,652,688 |

同一 detached evaluator 依次执行这些 seek，80 s 后 checkpoint 稳定为 52 个、15.881 MiB，低于 16 MiB 预算；这里是有缓存的递进 seek，不是每次从零冷启动。向 CPU worker 发出 10,000 s 的过期循环目标，等待 20 ms 后 scrub 到 0.5 s，最终 GPU 图像交付耗时 **21.958 ms**，没有等待过期目标回放完毕。

headless 最终运行 600 个时钟 tick，共形成 526 个不同请求，525 个被接受，接受频率 **52.467 Hz**；最大目标与显示时间差 **433 ms**，最终目标仍成功排空交付。该轮存在明显停顿：CPU 求值 p95 2.709 ms、最大 78.362 ms，Metal GPU p95 5.855 ms、最大 269.750 ms。保留这些异常值，不以该测试驱动的接受率代替上述原生窗口结果；尚未定位这次停顿的具体来源。

该轮 CPU / Metal 待处理峰值仍为 1、丢失事件为 0。每秒采样的进程 RSS 从 416.5 MiB 下降至 159.9 MiB，GPU allocated size 在 121.2–128.3 MiB 间；没有观测到持续增长。直接求值器长 seek 的预算数据来自 `results.json`，worker trace 的 checkpoint 最大值只统计 worker 自己的 evaluator，两者不混用。

10 s 内存采样用于观察短期走势，不能证明任意时长不存在泄漏。队列容量、checkpoint 预算检查和纹理缓存淘汰回归共同限制可保留状态，不以单次 RSS 波动判断泄漏。

## 回归修复与检查

验收发现返回暂停文档时可能空白：UI 已释放上一文档 GPU 图像，但 core 仍认为该目标请求过，不会重新发出。现在激活文档及关闭活动标签后的回退都会提高动画接受屏障并重新请求当前目标，保留资源、时钟和表情触发记录；旧激活的异步结果不能覆盖新画面。core 回归同时验证两种路径，实际模型场景验证返回后 GPU 图像可用。

- `cargo test --locked --workspace`：**362 passed，0 failed，4 项现有 ignored**；另外通过三个真实模型完整验收和真实 Metal 截图目标。真实 Mao 透明度 / Offscreen 测试实际运行，没有跳过。
- `cargo check --locked --workspace`、`cargo fmt --all -- --check`、`git diff --check`。
- Python 动画 trace 报告测试和现有 frame trace 报告测试；benchmark shell 脚本语法检查。
- 已检查 Mao 组合表情及无动画资源模型截图；模型、面板、空状态和播放控件可见。输出位于忽略的 `target/`，没有提交受许可约束的资源或截图。

## 复现

从仓库根目录运行，macOS / Metal 必需；每次使用新的输出目录：

```sh
cargo test --locked --workspace
cargo check --locked --workspace
cargo fmt --all -- --check
python3 scripts/test_animation_trace_report.py
python3 scripts/test_frame_trace_report.py

# 原生窗口需保持可见和激活；600 / 60 = 约 10 s。
./scripts/animation-window-benchmark.sh models/local/mao/runtime/mao_pro.model3.json target/animation-window-run 600

# 默认三个本地模型，600 个播放 tick；可在末尾传入自定义 model3 路径。
./scripts/animation-benchmark.sh target/animation-headless-run 600

# 不采集性能，只运行真实模型完整场景。
cargo test --locked -p kasane-editor --test animation_acceptance
```

原生结果包含 `environment.txt`、`application.log`、`trace.jsonl`、`summary.json`；headless 另外包含 `run.log`、`results.json` 和模型截图。JSONL writer 有固定容量并报告丢失事件；报告拒绝不完整 trace、丢失事件、失败 GPU command 和超预算 checkpoint。

许可模型不在公共仓库中。缺失模型会显式输出 `SKIP` 并在结果中缺席；没有本地资源时只能确认 portable fixture 回归，不能声称重复了本报告的实际模型验收或性能测量。

## 2026-10-13：高刷播放与偶发长帧复查

播放已改为窗口帧回调，取消 60 Hz 目标时间截断。SDK 保留完整的 60 Hz
模拟状态，从该状态求值显示时间的分数尾步；尾步不累积到后续物理模拟。
75 / 120 / 144 Hz 连续采样与冷 seek 完全一致，连续采样最多推进两个步。
首次原生窗口实测约 119.3 Hz，但仍存在 16.67 ms 的呈现间隔。

后续 trace 增加线程 CPU 用时、接受到合成、UI 绘制、drawable 等待、合成 GPU
以及 GPU 完成到系统呈现的分段。一次默认优先级求值的墙钟耗时为 36.95 ms，
线程 CPU 仅 1.77 ms；一次渲染准备耗时为 30.29 ms，线程 CPU 仅 1.30 ms。
这说明长耗时主要落在阻塞／调度等待，不能把墙钟耗时全部视为算法计算。
原始高刷 trace 也有模型和 GPU 及时完成、系统呈现多等一个周期的情况，
未将这部分归因为 SDK。

两个专用预览线程现在使用 macOS `QOS_CLASS_USER_INTERACTIVE`，仅作用于
动画求值与 Metal 提交；没有改变模拟步长、GPU 背压、显示同步或三缓冲配置。
空闲时仍在有界 mailbox 上阻塞。线程 CPU 计时仅在 trace 开启时采集。
`KASANE_TRACE_DEFAULT_WORKER_QOS=1` 可在 trace 模式下恢复默认调度作对照。

同一 Release 二进制、Mao、120 Hz 显示环境，顺序对照如下。长间隔定义为超过
该轮实际唯一呈现时间戳中位间隔的 1.5 倍；系统呈现时间不是硬件扫描测量。

| 配置 | 时长 | 新动画帧呈现 Hz | 长间隔数 | 最大间隔 ms | 求值非 CPU 时间 P99 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| 默认优先级，第 1 轮 | 20 s | 118.25 | 22 | 50.00 | 3.465 |
| 交互优先级，第 1 轮 | 10 s | 120.00 | 0 | 8.334 | 0.0183 |
| 默认优先级，第 2 轮 | 10 s | 119.00 | 10 | 16.67 | 2.321 |
| 交互优先级，第 2 轮 | 20 s | 120.00 | 0 | 8.334 | 0.0186 |

四轮均无 trace 丢失，CPU / 渲染待处理槽位峰值均为 1。对照支持保留交互
优先级，但短时测量不能保证所有系统负载和模型永久无掉帧。失焦的两轮和
受并行面板重构影响的构建失败不进入结果。报告现在按唯一呈现时间戳计算
Hz，避免多个关联纹理在同一系统呈现时刻重复计数。

本地原始数据为 `target/animation-pacing-{default-qos-1,qos-4,default-qos-2,qos-5}/`；
`target/animation-pacing-comparison.json` 保存汇总及二进制 SHA-256。

本轮验证：动画交互与 worker 定向回归 10 项通过；原生 Metal 9 项通过、1 项
既有忽略；trace 报告 6 项通过；工作区编译检查通过。扩大到名称包含
`animation` 的全部 UI 测试时，另一个并行 docking 改动中的
`animation_add_menu_places_panel_in_the_clicked_group_and_restores_it`
出现面板节点归属断言失败；没有将该轮报告为全量测试通过。
