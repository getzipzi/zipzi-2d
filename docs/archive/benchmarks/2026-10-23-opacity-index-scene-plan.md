# Opacity 索引复用与 ScenePlan 身份影响 — 2026-10-23

已修复直接 Opacity 拖动每帧重建 ObjectIndex，以及 editor Name 所经 SceneEdit::Patch(Label) 路径的两次全模型身份枚举。两项都保留了现有 authoring 检查、历史语义与旧快照。

## 版本与测量

- 原始基线：editor `279f569` / 2d `01db56a`。
- 本次修复前：editor `61371c0` / 2d `c7f1651` 加[上一轮通用发布修复](2026-10-23-2d-publication-fix.md)，对应原生二进制 SHA-256 `30399497b5f38bedde941a3d173407b209ff4f4b01183fe60bc7ae20da77a267`。
- 本次修复后：修复前版本加本文的 editor 索引刷新与 Core ScenePlan 身份证明。完整补丁与二进制哈希保存在 JSON 和本地 manifest。

本次重新运行三版本，而非沿用以前的基线数值。Mao、Release/test-support、相同 benchmark 驱动与配置；第一轮 baseline → before → after，第二轮反向。构建和测试全部在测量前结束，测量期间没有编译、测试或采样器。

直接 Opacity 沿用原有 PreviewOpacity 命令负载，30 次预热加 300 次正式输入，目标 120 Hz；同进程保留平移/缩放阶段。Name 使用实际原生文本控件，5 次预热加 96 次正式输入，每次均校验有效变化。以下是各进程 P95 的两轮中位数；Hz 也是各进程频率的中位数，并非合并原始样本后的统计。

| 指标 | 原始基线 | 本次修复前 | 本次修复后 |
| --- | ---: | ---: | ---: |
| 直接 Opacity：UI draw P95 | 11.93 ms | 20.74 ms | 3.55 ms |
| 直接 Opacity：新预览呈现频率 | 85.74 Hz | 49.22 Hz | 119.40 Hz |
| 直接 Opacity：输入到呈现 P95 | 53.83 ms | 70.75 ms | 40.27 ms |
| Name：提交线程 CPU P95 | 10.38 ms | 10.28 ms | 9.32 ms |

Opacity 的每帧 draw 耗时低于同批原始基线，呈现频率高于基线；这些结论针对该 Mao 负载和单台机器，不外推为所有模型的性能保证。Name 相对本次修复前的 CPU P95 下降约 9.4%。本次原始基线与修复前的 Name 已接近，因此不能把消除枚举的收益直接等同于前一份跨批次报告中剩余的 7.3%。

所有 12 个原生进程正常退出、benchmark_done 为真、lost_events 为 0、报告 issues 为空。Name 每个进程均为 96 次输入、96 次有效变化。Opacity 的请求终态逐轮为：

| 版本 | 第一轮 | 第二轮 |
| --- | --- | --- |
| baseline | `{"presented": 300}` | `{"presented": 300}` |
| before | `{"presented": 300}` | `{"presented": 300}` |
| after | `{"presented": 298, "accepted_not_composed": 2}` | `{"presented": 300}` |

修复后共 600 个正式 Opacity 输入，598 次呈现、2 次 accepted_not_composed；延迟分位数针对实际呈现请求。不能把这些终态隐藏后声称每次输入都独立呈现。新预览呈现 Hz 与窗口合成 Hz 是不同指标；输入到呈现也不包含硬件/OS 输入送达及屏幕像素响应。

## 工作次数与索引开销

独立 CPU 探针使用与定位时相同的 Rust 源码和实际 PreviewOpacity 入口。模型仍投影出 8150 个对象。两个插桩进程各 5 次预热、60 次正式输入：每进程 `editor.object_index_build = 0`，`editor.object_index_refresh = 60`，覆盖输入、索引读取和 CPU preview 三个计时阶段。

plain 版本的两进程中位数再取中位数如下，单位 ms：

| 阶段 | 修复后 |
| --- | ---: |
| PreviewOpacity 输入（包含局部刷新） | 0.003646 |
| object_index 缓存读取 | 0.000041 |
| 随后 CPU preview | 0.201479 |
| 三个阶段相加后的每次中位数 | 0.205199 |

局部刷新已计入输入阶段，不能只展示索引读取变快而忽略转移的工作。此前定位时的 14.07 ms/次是完整索引替换成本，本次通过工作次数直接确认该工作已经消除；CPU 探针没有重新计时旧版本，原生三版本对照才是本次完整的配对性能证据。

## 实现

Opacity 接受 candidate 修改后，调用已有 refresh_property_index 更新目标 Mesh 属性并同步 projection revision。继续复用 SnapshotTable、对象结构、关系和未变化的记录。旧的 Opacity 手势也被纳入提交/取消的属性刷新；提交失败时读取已经回到 committed model 的值，避免缓存保留 candidate 数据。

Core ScenePlan 新增只读 preserves_identities 证明。证明来自生成后的 delta、reparent 和 field patch 内容：没有结构 delta 或转换，且字段安装不移除身份时才成立。Glue pair 清除、删除、绘图角色生命周期和转换保持保守的身份跟踪。SDK edit_scene 与 apply_scene_plan 共用此证明，不按调用方声明或 ChangeKind 猜测，锁、expected value、stale plan、预算和取消检查仍走原路径。

## 验证

- editor-core Opacity 回归：1 passed；连续输入、cancel、commit、失败 commit、值恢复后无变化 commit 均不重建预热索引，并比较完整重建结果。保留旧 Arc<ObjectIndex> 与 candidate 快照，验证未变化记录/关系/绑定/拓扑的共享。
- editor-core 属性单元测试：3 passed，实际 set_object_property(Name) → SceneEdit::Patch(Label) 全链为 0 次 sdk.keys_in。
- editor-core document 集成：15 passed；锁相关集成：16 passed。
- SDK integration：321 passed、2 ignored。新增直接 edit_scene 和 apply_scene_plan 的 Label 工作次数验证，并验证清除/重建相同 Glue pair 后旧 handle 在提交、Undo、Redo 后持续失效；Core 删除/转换计划的证明为 false。
- Release 构建、两个仓库的格式与差异检查通过；没有运行整个工作区完整套件。

[机器可读结果](2026-10-23-opacity-index-scene-plan.json) 保存全部原生摘要、逐轮数据、工作次数和哈希。`target/ui-followup-20261023/` 保留保存的三版本二进制、index plain/probe 二进制、原始 JSONL/trace、构建日志、补丁及复测/汇总脚本。
