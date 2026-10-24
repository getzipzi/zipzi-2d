# UI authoring 回归定位 — 2026-10-23

属性提交回归已在不依赖 editor 的独立 SDK 探针中复现，主要来自 2d authoring 的全结构校验、身份枚举和跨域变化汇总。直接 Opacity 的逐帧减速另有明确路径：旧的全量 ObjectIndex 失效逻辑遇到新版更大的 authoring 投影，每帧重建由约 6.3 ms 增至约 14.1 ms。两者应分别修复，不能把每帧预览都解释成 SDK commit。

这是定位结果；未修改两个主工作区的产品源码。新增时钟只位于 `target/ui-diagnosis-20261023/source/zipzi-2d/` 的隔离工作树，受 `bench-instrumentation` 特性控制。

## 证据与实验边界

沿用 [原生 UI 对比](2026-10-23-ui-comparison.md) 的 Mao、Apple M4、Release 环境与基线：editor `279f569` / 2d `01db56a`，当前 editor `61371c0` / 2d `c7f1651`。

1. 现有 editing-chain：两个组合版本分别运行，retained index，5 次预热、24 次正式交互、每手势 6 次更新、两轮 plain/probe。它把 editor 提交、SDK 发布和 CPU preview 分开。
2. 独立 SDK：同一份 Rust 源码仅依赖 zipzi-core/SDK，不依赖 editor 或 GPUI。两个版本各两轮 plain/probe，5+40 个样本。选择 ArtMesh215，调用 `begin_owned_edit → replace_mesh → commit_to`；每次提交后在计时外 Undo，并断言内容恢复、receipt.changed 为真。另单独测量 `validate_structure`、fork 和内存预算估计。
3. 独立 ObjectIndex：同一份 zipzi-editor-core 探针、与原窗口负载相同的首个可见 Mesh，5+60 次交替 Opacity 输入，每次读取索引并执行/present CPU preview；两个版本各两轮 plain/probe。索引存在与场景接受均有断言。
4. 原生调用栈：使用此前保存的两个未插桩 Release 窗口二进制，只在 Opacity 正式阶段各 `sample` 3 秒。该运行只用于调用路径确认，不把采样期间的耗时当成性能基线。

所有构建在测量前完成。SDK 对照和 ObjectIndex 对照各自的第三方 Cargo.lock 包名/版本/来源集合完全相同；差异在对应的 Zipzi/Editor 源码。plain 时细分时钟编译移除，绝对耗时以 plain 为准。下表为两进程中位数的中位数，单位 ms、同步调用墙钟；不能与原生 UI 报告的线程 CPU P95 直接作同一指标比较。

## 1. 不经过 editor，2d 提交仍显著变慢

| SDK-only 指标 | 2d 基线 | 2d 当前 | 变化 |
| --- | ---: | ---: | ---: |
| Name：commit_to | 5.137 | 11.983 | +133.3% |
| Opacity：commit_to | 4.625 | 12.255 | +165.0% |
| begin_owned_edit | 0.460 | 0.905 | +96.5% |
| validate_structure 单独调用 | 3.438 | 7.044 | +104.9% |
| fork_candidate 单独调用 | 0.297 | 0.436 | +46.9% |
| estimated_content_bytes 单独调用 | 0.035 | 0.027 | -22.0% |
| Name：replace_mesh | 0.061 | 0.061 | -1.1% |

Mesh 字段替换本身仍约 0.06 ms，回归集中在创建/发布 candidate 的通用工作。单独验证函数约翻倍，说明不需要 GPUI、布局、GPU 或 editor 属性表即可复现主要问题。SDK-only 提交每次 Undo，历史长度固定；原生 96 次负载会积累历史，二者绝对值不应混用。

现有完整 Editor 链也得到同方向结果：

| Editor 链指标 | 基线 | 当前 | 变化 |
| --- | ---: | ---: | ---: |
| Name commit | 6.325 | 15.540 | +145.7% |
| Opacity commit | 5.855 | 15.007 | +156.3% |
| Opacity gesture begin | 1.615 | 3.484 | +115.8% |
| Opacity gesture update | 0.172 | 0.183 | +6.8% |
| Opacity gesture finish | 5.466 | 13.524 | +147.4% |

## 2. SDK 发布开销的具体来源

`modules/zipzi-sdk/src/edit.rs:679` 对任意提交执行完整 `candidate.validate_structure()`。该调用与“遍历所有 Binding/Motion”的框架在基线就存在，回归不是这些调用突然从 0 次变成 1 次；authoring 改造扩大了里面的身份和字段验证工作。

以下是当前 Opacity commit 的细分 span 均值，再取两个进程的中位数；属于插桩归因，不替代 plain 总耗时。结构校验父阶段包含下面的 Binding、Motion、身份校验，不能重复相加。

| 阶段 | 每次提交调用数 | 耗时 ms |
| --- | ---: | ---: |
| 全结构校验，inclusive | 1 | 8.627 |
| Mesh Binding 复制并 canonicalize | 185 | 2.000 |
| Scene Binding 复制并 canonicalize | 170 | 1.165 |
| validate_motion，全模型所有 Motion | 7 | 3.733 |
| 全局结构身份集合 | 1 | 0.695 |
| before/after 全对象 keys_in | 2 | 0.953 |
| changes::summarize，inclusive | 1 | 1.513 |
| 其中 summarize_organization | 1 | 0.614 |

具体机制：

- `zipzi-core/src/document/structure.rs:245` 对全部绑定 `binding.clone()` 后重新 canonicalize。`bindings.rs:25` / `:307` 现在还要 normalize 稳定 Axis/Sample ID、检查身份冲突，并为每个 keyform 重新解析 samples/keys 坐标。这些绑定没有因单个 Mesh 名字或透明度提交而改变。
- `structure.rs:114` 的 `binding_identities()` 会复制完整身份数组及其字符串，再插入全局身份 HashSet。公开的借用迭代器已存在，但完整验证当前走的是复制接口。这个阶段不是全部回归来源，只是一个可独立消除的分配成本。
- `motions.rs:449` 在每次完整验证中扫描所有 Motion；每个 Track/Key 的 `contains_base_identity` 又查询 expression、motion registration、physics 子项、Pose 等身份。与基线相比，身份检查域变大；7 个 Motion 的验证已是毫秒级，即便此次只改 Mesh。
- `zipzi-sdk/src/edit.rs:694` 重建 before/after keys；`session.rs:1140` 现在还包含 BindingAxis/Sample、MotionKey、ExpressionEntry、Physics 子对象等身份。`begin_owned_edit` 也会枚举 base_keys。身份不变的 scalar patch 仍支付这些全局枚举成本。
- `changes.rs:515` 起依次汇总 Motion、Physics、Pose、Organization、Scene、Binding、References、Content、Resources、Document。部分域内部有过滤，但 Organization 等仍先构造两份全模型集合、排序/比较。源码跟踪表明 ChangeSet 汇总从 C1 开始引入，后续 C2–C6 逐域扩展；不是最后一次小型代码整理才独立引入的功能。

Name 还存在附加路径：editor `document/properties.rs:250` 把单字段标签改为通用 `edit_scene(SceneEdit::Patch)`，完整链的 `editor.apply_properties` self 均值从微秒级升至约 1.4 ms。它有贡献，但小于 SDK 的通用发布增量。

## 3. Opacity 每帧回归来自 ObjectIndex 重建

原生栈均经过：`PropertiesPanel::summary → PropertyTarget::title → Document::object_index → ObjectIndex::build`，并包含索引/JSON/字符串释放。`preview_opacity` 在 editor `document/edits.rs:137` 每次调用 `invalidate_projection(false)`；`false` 保留几何拓扑，但并不保留 ObjectIndex 的 projection revision。Properties 标题读取索引时版本已失效。

同一 CPU 探针量化到：

| 指标 | 基线 | 当前 | 变化 |
| --- | ---: | ---: | ---: |
| object_index()：重建并替换缓存 | 6.277 | 14.070 | +124.2% |
| 随后 CPU preview | 0.332 | 0.424 | +27.6% |
| 投影对象数 | 2109 | 8150 | +6041 |
| 关系数 | 4989 | 12092 | +7103 |

插桩确认每个被测输入都调用 1 次 `editor.object_index_build`。计时包括重建和旧缓存替换时的析构，不仅是 builder 函数本体；这与原生栈中的字符串/serde JSON 释放工作一致。

主要新增对象包括 MotionKey 3469、BindingAxis 490、BindingSample 1576、ExpressionEntry 224、GlueVertexPair 161，以及 Physics/Model3/CDI 子项。editor C4 提交 `db8569d` 在 `objects/build.rs:220` 加入 Axis/Sample 投影，C2 等加入 MotionKey 投影。新增对象是有效 authoring 功能；问题在于透明度每次输入会把这些未变化的数据也重建。不能通过删掉 authoring 对象作为正式修复。

约 7.8 ms 的索引重建增量与原 UI draw P95 增加约 9 ms 的量级一致，且原生调用路径相符；两者的测量边界和统计量不同，不能宣称它们逐毫秒严格相等。这里尚未做禁用重建后的原生 A/B，因此对最终可恢复 FPS 的幅度不作保证。

## 修复顺序与需要保留的契约

1. **2d：复用未变化域的验证结果。** 对 identity-preserving 字段编辑，以不可变共享数据/明确依赖和修订为依据，避免重新 canonicalize 所有 Binding、重新验证全部 Motion。涉及参数范围、关系、身份、导入/恢复等变化时仍须验证对应依赖或执行完整验证。不能直接删除 commit 的正确性检查。
2. **2d：让身份与 ChangeSet 工作随实际变化增长。** 复用身份集合；对明确保留身份的编辑避免枚举全模型；基于实际修改的域/对象派生变化摘要，同时保留 Undo/Redo、对象替换、删除重建的 incarnation/epoch 语义。
3. **editor：修复旧 Opacity 预览失效边界。** 复用结构和关系图，只刷新目标 Mesh 的外观记录，沿用已有 property preview 的局部更新思路，并保证旧 Arc 快照不变。这样可同时避免 builder 与旧索引析构成本。
4. 再检查单字段 Label 是否需要完整 ScenePlan；在保持锁、stale baseline、身份、历史和收据语义的前提下复用专用字段路径。

这次没有进行产品优化补丁或完整 Rust 测试。下一步修复应先加必要的工作次数/不可变快照回归，再跑受影响模块和相同原生负载；不需要先重跑全部 UI benchmark 或昂贵全套测试。

## 保存的证据

- [机器可读诊断](2026-10-23-ui-authoring-diagnosis.json)：逐进程数据、嵌套阶段、对象分类、现有 editing-chain 结果、探针哈希、版本与隔离工作树的时钟补丁。
- 本地 `target/ui-diagnosis-20261023/` 保留相同 Rust 探针、各自 Cargo.lock、plain/probe 二进制、原始 JSONL、构建日志、调用栈、源代码归档和 `diagnostic-spans.patch`。
- `run_sdk_probe.py` 与 `run_index_probe.py` 构建完成后再执行两轮版本交错测量；`profile_native.py` 只抓 Opacity 阶段栈。
- 第一次旧 editing-chain 脚本在系统 Python 缺少 `hashlib.file_digest` 时失败，未产生测量；改用 Python 3.14 后的 `baseline-chain-01` 是有效来源。
