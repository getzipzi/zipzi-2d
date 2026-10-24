# 2d authoring 发布修复与复测 — 2026-10-23

本次修复落在 zipzi-2d 的通用发布路径：复用未变化 Binding/Motion 的验证结果，缓存身份集合，并按持久数据域裁剪 ChangeSet 汇总。editor 产品源码保持 `61371c0`，没有加入单属性优化。

修复前为 2d `c7f1651`，修复后为同一提交加本地补丁；源码补丁和二进制 SHA-256 见机器可读结果。最初 editor `279f569` / 2d `01db56a` 的基线对比保留在[原始报告](2026-10-23-ui-comparison.md)，根因证据见[定位报告](2026-10-23-ui-authoring-diagnosis.md)。本表采用重新运行的修复前/后 A/B。

## 独立 SDK

沿用同一份 SDK 探针、Mao 的 ArtMesh215、Release、5 次预热加 40 个样本；每次 `begin_owned_edit → replace_mesh → commit_to` 后在计时外 Undo 并校验内容恢复。两轮按 before/after、after/before 顺序执行；plain/probe 均独立运行。表中为两进程中位数的中位数，单位 ms、同步调用墙钟。

| 指标 | 修复前 | 修复后 | 变化 |
| --- | ---: | ---: | ---: |
| Name commit_to | 12.401 | 3.549 | -71.4% |
| Opacity commit_to | 12.279 | 3.565 | -71.0% |
| Name begin_owned_edit | 0.937 | 0.450 | -51.9% |
| Opacity begin_owned_edit | 0.978 | 0.466 | -52.4% |
| 完整 validate_structure | 6.982 | 7.023 | +0.6% |

完整 `validate_structure()` 仍执行全量检查。加速来自发布时复用已有验证结果；不会把对外的全量验证接口变成跳过检查的接口。插桩确认 Mesh 标签/透明度提交中的 `sdk.keys_in` 和 `core.validate_motion` 调用均为 0。SDK 变化汇总的 inclusive 耗时也下降，逐轮数据保存在 JSON 中；旧诊断版本具有更多细分 span，不能把新版本缺少这些细分 span 解释为对应调用次数为 0。

## 原生 UI

同一个 editor 提交分别链接修复前/后的 SDK，使用保存的未插桩 Release 二进制。Name、Opacity 文本、Enabled 各 5 次预热、96 次正式输入、120 Hz 请求频率，两轮顺序交错。构建和测试全部在测量前完成。每个进程都正常退出，报告无 issues，96 个输入均有有效变化且 benchmark_done 为真。

下表为每进程提交线程 CPU P95 再取两轮中位数，单位 ms。此边界包含 editor 属性提交，因此不能与上面的 SDK 同步墙钟直接相减。

| 原生属性 | 修复前 CPU P95 | 修复后 CPU P95 | 变化 |
| --- | ---: | ---: | ---: |
| name | 20.515 | 10.522 | -48.7% |
| opacity_text | 20.398 | 9.955 | -51.2% |
| enabled | 19.639 | 9.333 | -52.5% |

Opacity 文本会修改存储的 Binding keyform，不能只覆盖 Mesh 本体的外观更新。最终修复同时处理这一通用路径：Axis/Sample ID 不变时保留身份集合，依赖和所有权未变化时只重新校验实际变化的 Binding。回归测试放入第二个同级 Binding 和一个 Motion，确认 keyform 替换只调用两次目标 Binding canonicalize（替换和发布各一次），不重新验证同级 Binding 或 Motion，也不调用全模型 keys_in。

本次没有重测或修改旧 Opacity 拖动的 ObjectIndex 每帧失效路径，也不据此宣称拖动 FPS 已恢复。

## 与最初指定基线比较

基线为 editor `279f569` / 2d `01db56a`。沿用前次相同负载的基线测量，与最终修复版本对照：

| 原生属性 | 基线提交 CPU P95 | 修复后 | 相对基线 |
| --- | ---: | ---: | ---: |
| name | 9.805 | 10.522 | +7.3% |
| opacity_text | 9.709 | 9.955 | +2.5% |
| enabled | 9.340 | 9.333 | -0.1% |

独立 SDK 的 Name commit 中位数为 5.137 → 3.549 ms（-30.9%），Opacity 为 4.625 → 3.565 ms（-22.9%）。

原生提交已接近最初基线，SDK 独立提交低于该次基线值。这里是跨批次对照：原生基线每项为一组 96 次输入，修复后每项两组；尚未重新交错运行原始基线与修复版本。因此 0–7% 的小幅差异不足以认定为稳定回归。旧直接 Opacity 拖动没有修复后数据，不能宣称其每帧性能已恢复。

## 实现与正确性边界

- `document/publication.rs` 使用借用比较区分持久数据域、身份、验证依赖。身份比较直接检查 Axis/Sample/Pair ID，避免把 keyform 数值变化当成身份变化。
- `document/structure.rs` 新增基于已验证基线的发布校验。参数范围、绑定所有权、目标几何、共享约束或身份变化会撤销对应复用；已变化的记录仍执行原校验。全局结构、顺序、引用和其他记录检查仍保留。
- 验证证明绑定精确 snapshot identity；普通 Document clone/fork 不继承证明，防止内部 planner 直接修改分支内容后误用。已验证 checkpoint 随自身内容保留证明，Undo/Redo 恢复时重新关联当前快照；checkpoint 资源路径直接重写会丢弃证明。无有效证明或基线无效时回退全量检查。
- SDK 的 ObjectKey 集合按包含 session/generation/revision 的 Version 缓存，并在实际身份未变时共享 Arc；删除后重建的 erased_keys 仍参与 incarnation 更新。Binding、SceneBinding、Blend、Glue 的 typed replacement 先比较其自身拥有的 ID，减少替换期间的全模型退休扫描。
- `changes.rs` 基于实际数据而非声明的 ChangeKind 跳过未变化域。References 依赖涵盖 Mesh/Offscreen masks 和 Blend constraint 选择；Scene 对 Topology 分类的归一化保持原行为，包括内容相同的删除重建。

这仍包含借用的域比较、其余结构检查、candidate/checkpoint 处理等成本，并非整个提交都已变成只遍历改动对象。

## 验证

- `cargo test --locked -p zipzi-core --lib`：14 passed。
- `cargo test --locked -p zipzi-core --test checkpoint_size_tests`：1 passed。
- `cargo test --locked -p zipzi-sdk --features bench-instrumentation --lib`：12 passed。
- `cargo test --locked -p zipzi-sdk --features bench-instrumentation --test integration`：320 passed，2 ignored。
- `cargo test --locked -p zipzi-editor-core --lib document::properties::tests`：3 passed。
- Release editor 构建、格式检查及 `git diff --check` 通过；没有运行昂贵的整个双仓库完整套件。

新增覆盖包括无效依赖、身份冲突、ID 未变但 keyform 无效、快照/分支证明、checkpoint 往返、缓存工作次数，以及裁剪/完整 ChangeSet 的差分比较和 Undo/Redo 收据比较。SDK 集成套件还覆盖删除重建、旧 handle/address 失效、取消、预算和失败原子性。

## 保存的证据

- [机器可读结果](2026-10-23-2d-publication-fix.json)：逐轮 SDK/原生数据、插桩阶段、元数据及二进制/补丁哈希。两份 SDK Cargo.lock 的第三方包名/版本/来源集合一致。
- 本地 `target/2d-publication-fix-20261023-final/`：相同 SDK 探针、Cargo.lock、plain/probe 二进制、原始 JSONL、原生 frame trace、构建日志、`2d-fix.patch`、manifest 及复测脚本。
- 中间版本结果保留在 `target/2d-publication-fix-20261023/`，用于发现 Binding keyform 的保守回退；以上结果只采用最终版本目录。
