# Mesh properties 性能 benchmark（2026-10-19）

**结论：不能通过“这批控件没有 CPU/GPU 性能问题”的验收。**

全部 33 项交互已覆盖，最终采用的逐项数据有完整输入校验和明确请求终态。
GPU 各项 P95 均低于 120 Hz 的 8.33 ms 参考预算，但 tooltip 的 UI CPU 绘制和
部分同步输入处理存在可重复的超预算。这里的事件处理、UI draw 是墙钟边界，
尚不能将同步事件中的全部等待归因为纯 CPU 计算或某个 SDK 函数。

[机器可读汇总](2026-10-19-mesh-properties.json) 保存每项来源、样本量、
P50/P95/P99/最大值、超预算数量、请求终态及原有 UI benchmark 对照。

## 环境与执行

- Apple M4 / 10 核 GPU / 16 GiB，Mac16,10，macOS 27.0.1。
- 主显示器 3840×2160，逻辑 1920×1080，120 Hz；Release、真实原生窗口和 Metal。
- 沿用现有 Mao 模型：260 个 mesh；选择 ArtMesh215，158 个顶点，初始 opacity=1。
- 每项 30 次预热；主测每项 300 次输入，目标 120 Hz；有搜索防抖和弹层等待的交互保留实际等待。
- 性能复测使用现有 Timeline number probe 的 96 样本量；tooltip 使用 1,200 次 hover，获得 28 次 UI draw。
- 主测 9,900 次、性能复测 1,872 次、终态追踪修复后的复测 684 次，共 12,456 次正式属性输入。
  最终逐项有效清单共 9,984 次输入；另有原有窗口 benchmark 的 900 次输入对照。
- 基于工作树提交 `a2fbcd63b973751010ef03743f0fb07a28f3dbd0`；包含本次未提交的 benchmark 和追踪补充。
  设备、模型和代码版本已保存在机器可读汇总中；后续 CPU 修复汇总另记录 SDK 版本。

## 已确认的超预算

| 路径 | 首轮 | 复测 | 计时边界 |
| --- | ---: | ---: | --- |
| 原色/当前色 tooltip 显隐 | 22.49 ms | 20.33 ms | UI draw P95；复测 28 帧中 25 帧超过 8.33 ms |
| Multiply 行内 HEX | 8.67 ms | 8.42 ms | UI draw P95 |
| picker 打开/关闭 | 8.88 ms | 8.76 ms | UI draw P95 |
| picker Done | 8.81 ms | 8.77 ms | UI draw P95 |
| Multiply reset | 8.54 ms | 8.60 ms | UI draw P95 |
| Name 提交 | — | 25.98 ms | 每次交互中最慢同步事件的 P95 |
| Opacity 文本提交 | — | 27.41 ms | 每次交互中最慢同步事件的 P95 |
| Enable 切换 | — | 22.68 ms | 每次交互中最慢同步事件的 P95 |

runtimeId 主测的事件处理 P95 为 25.21 ms；该统计将各类原生事件合并，
不能与“每次交互最慢事件”直接作百分比比较。上述提交路径应进一步分解
事件内的编辑、通知、布局和等待成本；tooltip 应进一步定位触发全窗口绘制的成本。
本轮发现不构成“这些控件新引入了回归”的因果证明。

## GPU、空闲与请求终态

- 最终有效数据中，scene 求值 P95 最高 **3.69 ms**；预览 GPU P95 最高 **4.71 ms**，
  7,936 个预览 GPU 样本没有超过 8.33 ms。
- 窗口合成 GPU P95 最高 **6.93 ms**；31,614 个合成 GPU 样本中有 90 个超过
  8.33 ms（约 0.28%）。未发现 Metal command failure；这些单帧尖峰仍保留在数据中。
- 关闭和打开 picker 的静止基线各约 2.75 秒，均为 **0 次 UI CPU draw、0 次模型预览请求**。
  窗口仍有缓存场景合成（49 / 50 次），因此不能描述为 GPU 零工作；picker 没有引入明显额外合成频率。
- 最终采用数据有 7,357 个已呈现预览、577 个明确的 `delivery_coalesced`、
  2 个 `accepted_not_composed`，没有不明 `incomplete`；UI-only 输入不要求产生新模型纹理。
- 输入到呈现仅统计有准确纹理关联的样本；保留其他请求终态，避免只看成功显示的低延迟。
  目标 120 Hz 不等于实际 120 FPS；这里不测 OS 输入投递或 input-to-photon。

## 逐项测试清单与数据

各列均为毫秒 P95。UI draw 包含整个窗口当时的 CPU 布局/绘制，并非单个控件的独占耗时。
数字拖动沿用 7 px 步进序列；量化导致部分输入不改值，滑动输入有 268/300 次有效值变化。
Multiply picker 是 RGB 控件，不包含 Alpha UI。

| 测试项 | 正式输入数 | UI CPU | 预览 GPU | 合成 GPU |
| --- | ---: | ---: | ---: | ---: |
| `name` | 300 | 7.00 | 3.34 | 6.84 |
| `runtime_id` | 300 | 6.88 | 3.24 | 6.84 |
| `part` | 300 | 6.29 | 3.45 | 6.44 |
| `masks` | 300 | 6.65 | 2.99 | 6.49 |
| `opacity_text` | 300 | 7.57 | 3.64 | 6.93 |
| `opacity_drag` | 300 | 6.56 | 3.76 | 5.62 |
| `multiply_hex` | 96 | 8.42 | 3.99 | 6.61 |
| `color_blend` | 300 | 7.12 | 3.47 | 6.55 |
| `enabled` | 300 | 7.73 | 3.84 | 6.65 |
| `picker_open_close` | 300 | 8.88 | 3.71 | 6.85 |
| `picker_tabs` | 300 | 6.87 | — | 5.88 |
| `picker_hex` | 300 | 6.86 | 3.72 | 6.37 |
| `picker_invalid_hex` | 300 | 5.86 | — | 6.21 |
| `picker_hue` | 300 | 7.63 | 3.71 | 5.85 |
| `picker_sv` | 300 | 7.35 | 3.65 | 5.76 |
| `picker_rgb_0_text` | 300 | 7.47 | 4.49 | 6.18 |
| `picker_rgb_1_text` | 300 | 7.44 | 4.59 | 6.27 |
| `picker_rgb_2_text` | 300 | 7.50 | 4.71 | 6.57 |
| `picker_hsv_0_text` | 300 | 7.43 | 4.51 | 6.37 |
| `picker_hsv_1_text` | 300 | 7.02 | 4.17 | 6.28 |
| `picker_hsv_2_text` | 300 | 7.01 | 4.13 | 6.24 |
| `picker_rgb_0_drag` | 300 | 7.54 | 3.79 | 5.79 |
| `picker_rgb_1_drag` | 300 | 7.65 | 3.84 | 6.07 |
| `picker_rgb_2_drag` | 300 | 7.73 | 3.76 | 5.84 |
| `picker_hsv_0_drag` | 300 | 8.13 | 3.65 | 5.95 |
| `picker_hsv_1_drag` | 300 | 7.75 | 3.70 | 5.84 |
| `picker_hsv_2_drag` | 300 | 7.69 | 3.81 | 5.77 |
| `picker_done` | 96 | 8.77 | 3.57 | 6.45 |
| `picker_cancel` | 96 | 8.02 | 3.64 | 6.40 |
| `multiply_reset` | 96 | 8.60 | 3.47 | 6.78 |
| `picker_swatches` | 1200 | 20.33 | — | 6.22 |
| `idle` | 300 | — | — | 6.21 |
| `picker_idle` | 300 | — | — | 5.20 |

`part` 覆盖搜索、选择与清空；`masks` 覆盖搜索添加和移除；`color_blend` 遍历全部 18 种模式。
RGB/HSV 的六个数值通道分别覆盖文本输入和拖动；picker 另覆盖色环、SV、HEX、非法草稿、
Tab、确认、取消、关闭、色样 tooltip 与空闲。

## 既有 UI benchmark 对照

复用同一 Release 二进制、Mao 模型和独立窗口配置，运行原有 pan、zoom、直接 opacity
负载各 300 次。原有 opacity 为命令入口；选中 mesh 也沿用原 runner 的规则。
该对照用于理解已有成本，不是逐控件的严格 before/after。

| 原有负载 | UI CPU P95 | 预览 GPU P95 | 合成 GPU P95 |
| --- | ---: | ---: | ---: |
| `opacity` | 10.56 | 5.23 | 6.28 |
| `pan` | 3.61 | 4.20 | 5.64 |
| `zoom` | 3.80 | 2.04 | 5.35 |

原有直接 opacity 的 UI CPU P95 已是 10.56 ms，说明已有共享 UI/作者编辑路径也存在预算压力；
不能把本次全部 CPU 成本归因到新控件。

## 追踪完整性与产物

主测发现 1,797 个完成 GPU 渲染、但缺少 UI 接收终态的中间帧，涉及 opacity_drag、
multiply_hex、picker_done、picker_cancel、multiply_reset。结果邮箱原本会覆盖旧结果，
但没有为该覆盖写 trace。本次增加 `delivery_coalesced` 事件，并让专项报告拒绝不明终态。

分析时旧 trace 和旧报告保持原样。上述五项最终数据使用追踪修复后的独立采样替代；
tooltip 使用 1,200 次复测替代主测的小样本。修复后的五项均通过完整性检查，
观察到明确的覆盖终态，未把这些中间帧作为成功呈现样本。该修复不改变邮箱保留最新结果的策略。
复测选择的 mesh、初始 opacity 和模型规模与主测一致。

- 主测：`target/properties-mao-run-01/`。
- 性能复测：`target/properties-confirm-<case>/`。
- 终态修复复测：`target/properties-terminal-<case>/`。
- 原有 UI 对照：`target/properties-mao-ui-reference/`。
- 以上为历史运行目录。按用户要求，原始 trace、临时配置、日志和代码快照已清理。
- `validated_cases` 中每项的 `source` 保留历史来源标识；逐项统计和请求终态已保存在本文的 JSON 汇总中。
- [修复前短跑结果](2026-10-19-mesh-properties-short-run.json) 另保存全部 33 项、每项 3 次正式输入的复跑结果。

## 验证与复跑

默认功能集 cargo check、Release 原生运行、cargo fmt、diff 检查通过；
26 项针对性的 Rust 测试（picker、mesh properties、UI performance、mailbox）和
36 项 Python 工具测试通过。既有 dev 生命周期计时测试曾在并行构建检查期间偶发失败，
停止并行构建后完整重跑通过。

```sh
./scripts/properties-benchmark.sh target/new-properties-run 300 120
ZIPZI_BENCH_PROPERTY_CASE=picker_swatches ./scripts/properties-benchmark.sh target/new-tooltip-run 1200 120
```

[完整负载和计时说明](../../../tools/benchmarks/README.md#mesh-properties-native-window-benchmark)。
本报告记录修复前结果。后续改动和剩余预算问题见
[CPU 修复与小样本验证](2026-10-19-mesh-properties-cpu-fix.md)。
