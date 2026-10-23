# SDK 增量发布后的原生 UI 复测 — 2026-10-23

相对同一 editor 链接的保守发布 SDK，增量发布的 Name／Opacity 文本／Enabled 提交 CPU P95 分别下降 27.6%／17.3%／28.0%，三轮配对均同方向。直接 Opacity 拖动维持约 120 Hz，未观察到与旧回归相同的明显退化。36 个原生进程全部通过完整性检查。

阶段 A–D 完成后，主对比固定当前 editor 产品代码，仅切换保守发布 SDK 与本次增量发布 SDK；最初基线也在同批重新运行。以下均为实际保存的 plain Release 二进制结果。

## 版本与方法

- 保守发布：editor `3f24179`，SDK `4bb2fee`，SDK 无 A–D 补丁。
- 增量发布：同一 editor，SDK 同一 HEAD 加 A–D 工作区补丁；editor 工作区代码差异只在 `#[cfg(test)]` 中，未改变 Release 产品代码。
- 最初基线：editor `279f569` / SDK `01db56a`，沿用先前保存且 SHA-256 已核实的二进制，重新测量。与当前的 benchmark 驱动及报告脚本逐文件一致；产品其余部分跨版本变化，因此只将保守／增量组合用于隔离本轮 SDK 贡献。

Apple M4（10 核 GPU）、16 GiB、macOS 27.0.1，CB27U1 3840×2160／逻辑 1920×1080／120 Hz；同一 Mao runtime 模型（属性控件均选择 ArtMesh215，canvas 沿用首个可见 Mesh 的选择规则）、Metal mesh overlay、关闭 HUD、每进程隔离配置。构建使用 `cargo build --locked --release -p kasane-editor --features test-support`，未启用 `bench-instrumentation`。两个变体全部构建并保存后才开始测量，测量期间不编译、不运行测试。

三轮版本顺序依次为“保守→增量→原始”“原始→增量→保守”“增量→保守→原始”。每轮的 Name／Opacity 文本／Enabled 各 5 次预热＋96 次正式输入、120 Hz 请求；canvas 同进程的 pan／zoom／opacity 各 30 次预热＋300 次正式输入、120 Hz 请求。

下表为三个进程各自 P95 的中位数；Hz 为三个进程频率的中位数，不合并原始样本。变化列相对保守发布；小幅差异可能受帧相位、温度和调度影响，没有进行统计显著性检验。

## 属性提交

单位 ms。Commit CPU 是同步属性提交线程 CPU，不能与 SDK 单独调用的墙钟相减。

| 指标 | 最初基线 | 保守发布 | 增量发布 | 相对保守 |
| --- | ---: | ---: | ---: | ---: |
| Name：提交 CPU P95 | 9.957 | 8.758 | 6.343 | -27.6% |
| Opacity 文本：提交 CPU P95 | 9.919 | 10.588 | 8.759 | -17.3% |
| Enabled：提交 CPU P95 | 9.059 | 8.635 | 6.221 | -28.0% |

逐轮提交 CPU P95 与对应配对变化：

| 属性 | 保守三轮 | 增量三轮 | 各轮变化 |
| --- | --- | --- | --- |
| name | 8.746／9.383／8.758 | 6.211／6.343／6.621 | -29.0%／-32.4%／-24.4% |
| opacity_text | 10.588／10.014／10.829 | 8.759／9.105／8.729 | -17.3%／-9.1%／-19.4% |
| enabled | 8.913／8.635／8.368 | 6.399／6.221／6.196 | -28.2%／-28.0%／-26.0% |

Name／Enabled 可使用有限 Mesh 局部验证；Mao 的 Opacity 文本实际修改 Binding keyform payload，继续原 canonicalize 与广泛验证，但摘要及身份判断可使用阶段 C 的局部发布。三条路径的收益不能都归因于阶段 D。此处 plain UI 数据用于总耗时，阶段工作次数依据[实施记录](../../plans/sdk-incremental-publication.md)。

## 拖动与画布对照

| 指标 | 最初基线 | 保守发布 | 增量发布 | 相对保守 |
| --- | ---: | ---: | ---: | ---: |
| 直接 Opacity：UI draw P95（ms） | 11.361 | 3.492 | 3.450 | -1.2% |
| 直接 Opacity：输入到呈现 P95（ms） | 53.435 | 40.699 | 40.754 | +0.1% |
| 直接 Opacity：新预览呈现 Hz | 88.592 | 119.202 | 119.999 | +0.7% |
| 平移：UI draw P95（ms） | 3.575 | 3.632 | 3.574 | -1.6% |
| 缩放：UI draw P95（ms） | 3.909 | 3.946 | 3.743 | -5.1% |

直接 Opacity 的输入到呈现 P95 轮间波动较大：相对保守版本的逐轮变化约为 +2.9%／-37.9%／+20.3%，中位数接近，不能认定为稳定改善或回归。

直接 Opacity 使用既有 PreviewOpacity 命令负载；热路径的 ObjectIndex 复用在本轮之前已完成，保守／增量两组都包含该修复。这里用于观察回归，不能把其帧级轮间差异直接归因于提交时的局部验证。UI draw 是布局／绘制墙钟，新预览 Hz 不等于窗口合成 Hz；输入到呈现不含硬件／OS 输入送达、屏幕扫描与像素响应。

## 完整性与请求终态

36 个进程均正常退出、benchmark_done 为真、报告 issues 为空、唯一 end 且 lost_events=0。27 个属性进程各 96 次正式输入、96 次有效变化、96 次 commit CPU 计时；9 个 canvas 进程每模式 300 次输入，共 10,692 个正式输入 ticks。

直接 Opacity 的逐轮终态（分位数只统计具有相应计时的请求，未呈现情况完整保留）：

| 版本 | 第一轮 | 第二轮 | 第三轮 |
| --- | --- | --- | --- |
| original | `{"presented": 300}` | `{"presented": 300}` | `{"accepted_not_composed": 1, "presented": 299}` |
| conservative | `{"accepted_not_composed": 1, "presented": 299}` | `{"presented": 300}` | `{"accepted_not_composed": 13, "presented": 287}` |
| incremental | `{"presented": 300}` | `{"presented": 300}` | `{"presented": 300}` |

## 证据与边界

- [机器可读结果](2026-10-23-sdk-incremental-ui-retest.json)：36 个进程完整摘要、每轮输入参数、原生 CPU／GPU 与延迟分位数、请求终态、配对变化和完整 provenance。
- 本地 `target/ui-incremental-publication-20261023/`：源码 patches 与未跟踪源文件归档、逐文件哈希、Cargo.lock 哈希、模型与纹理哈希、硬件信息、三个保存的二进制及其 SHA-256、构建日志、原始 trace.jsonl、requests.csv／json、每进程日志、复现／汇总脚本。
- 报告脚本测试 27 项通过；使用前一阶段已通过的 Core／SDK／Editor 差分和回归门禁，本次未再跑全工作区。
- 本次仅为上述原生 UI 负载；没有重跑独立 SDK 与完整 Editor property-chain 的 plain/probe 成对耗时，也没有覆盖全部 33 项属性、变形器或动画负载。不能据此将计划的全部性能退出项标记为通过。

复现已保存的二进制测量（已存在完整结果的进程会跳过；拒绝覆盖未完成 trace）：

```sh
python3 target/ui-incremental-publication-20261023/run_native.py
python3 target/ui-incremental-publication-20261023/summarize.py
```
