# zipzi-python2 实施记录

日期：2026-10-25。对应[建设计划](../python-sdk-rewrite-plan.md)及[独立新包](../../../crates/zipzi-python2/README.md)。

**下一阶段调整：**[P3A：统一 API 与 agent 契约重构](p3a-api-agent-contract.md)已拆为独立方案，当前仅完成方案，尚未实施/验收。先统一公开返回与恢复、生成/手写边界、分页/Session 关闭及随包文档和 agent 门禁，再进入 P3B 剩余 Modeling 和 P4 剩余资源工作。现有 Buffer/二进制工作保留并单独记录证据；下文历史通过记录不自动覆盖新接口，当前包使用方式仍见包 README。

当前执行主线接回 [Runtime R0–R4](../sdk-runtime/python2-r0-r4.md)。`1e0043f`
已保存 P2 网格切片；其后的工作区补齐宿主发现/唯一启动、会话身份/列表/绑定，
并通过安装纯包的真实 Editor 冷启动、重开及原生 UI 发布验收。其后工作区的
[事件、参数和批量查询](../sdk-runtime/python2-r3-events.md)已接回 R3 最小链路，
该切片已提交为 `c0e186b`。其后[网格绑定与显式 CPU 采样](../sdk-runtime/python2-r4-bindings.md)
接回首批 R4 数值链路，源码测试为 50 项、类型反例为 8 条；完整 R4 仍待扩展。
最新 [P3 场景/身份记录](../sdk-runtime/python2-p3-scene-identity.md)补齐选定场景/组织、
网格 BlendShape/带绑定拓扑与 SDK 引用/标签地址；57 项源码行为、16 条类型反例及
安装/Editor 证据已更新。R4 仍未达到全部退出门槛。下面 P1/P0 记录保留原始范围。

2026-10-25 [Runtime R1–R3 / Python P1–P2 整体复验](../sdk-runtime/r1-r3-python-p1-p2-acceptance.md)
修复了丢失 Busy 响应后的 Edit 状态恢复及 Runtime 源接口清单分类。
后续按用户要求补齐[P2 资源/Plan 生成和源码分发](p2-resources-generator.md)，并复测 Runtime、安装客户端、真实 Editor 及两组冻结性能门禁。整体复验已通过；Editor 首轮越线、对照及扩样数据也完整保留在记录中。

## 项目边界

本项目从新包 `crates/zipzi-python2` 开始，Python 导入名为 `zipzi`。P0/P1 的公共类型、生命周期、两后端、薄 PyO3、包级测试、构建和测量工具均已移入新目录。旧 crate 中的临时特性接线和源码改动已撤回；旧接口去向清单和旧包性能采集工具不再属于本项目。

可复用的是共享 Rust SDK/Runtime 和独立 Rust 测试。`MeshCreateSpec`、`MeshOperation` 及默认值/ID 分配在 SDK；`LocalHost`、候选写入、来源、任务与预算在 Runtime。新包仅适配这些类型化处理器。共享层的新增代码单独计数。

## 阶段状态

| 阶段 | 已实现 | 尚未满足的退出门槛 |
| --- | --- | --- |
| P0 | 选定六项工作负载的 Local/Connected 计时和独立 Rust/Python 探针；冻结比较及负控制 | 后续领域工作负载；计数范围之外的隐式领域 Clone 和 allocator 内部复制不计入当前负载复制数字 |
| P1 | 选定 API、两后端、来源/资源/历史/IO、36 项公共行为与生命周期矩阵、严格类型及独立正常 wheel 验收 | 当前选定试点门槛已满足；长任务、后续领域和发布平台矩阵另列 |
| P2 | [网格历史切片](p2-mesh-generator.md)及[资源/Plan 补齐](p2-resources-generator.md)：Rust 单一声明、编译查询注册、18 个产物、有限形状和配对源码发行验证 | 当前范围退出证据见整体复验；未实施领域按 P3–P5 扩展 |
| P3 | [已开始：场景/身份切片](../sdk-runtime/python2-p3-scene-identity.md)，含生成的场景操作、网格 BlendShape/带绑定拓扑、SDK Ref/标签地址 | 完整场景绑定/转换/集合及剩余字段/内容地址；R4 全量资源/IO/任务预算 |
| P3A | 下一阶段；仅有[独立重构方案](p3a-api-agent-contract.md)，未实施 | 最终 API、完整调用恢复、统一生成 facade、分页/关闭、文档自省与 agent 任务门禁，全部按独立方案重新验收 |
| P3B | P3 剩余领域，待 P3A | 按统一契约补完整 Modeling/地址；已有数值与身份预期保留 |
| P4 | 工作区已有 Buffer/二进制资源实现与测试；完整阶段未验收 | 已公开包装纳入 P3A；完整 IO/格式、昂贵任务、资源峰值及正式发行证据继续按计划交付 |
| P5/P7 | 未开始 | 按调整后的计划交付；已有 CPU 建模求值归 Runtime R4，不代表 P5 动画已实现 |
| P6 | 纯包/原生包分别构建；隔离安装 recipe；Runtime R3 宿主发现与真实 Editor 选定链路 | 完整版本/平台配对、发行 app 安装位置和全部领域安装验收 |

能力范围见 [`CAPABILITIES.md`](../../../crates/zipzi-python2/CAPABILITIES.md)。源码接线、独立验证和阶段验收分开记录；当前选定 P1 试点已满足退出门槛，P0 试点统计有明确范围，总体后续领域仍按阶段跟踪。

## P1 当前链路

- `local()` 显式拥有进程内 Runtime，`connect(endpoint=...)` 绑定现有宿主。两者进入同一 Rust 处理器。
- `MeshSpec` 创建与 `Rename/SetPositions` 修改意图分开；省略的持久 ID 由 SDK 生成。便利方法构造同一操作，显式批次按序执行并单独提交。
- Snapshot 拥有不可变 tuple 数据及确切 formal/edit 来源。候选序号、Runtime/Session、正式版本与候选身份都在测试中断言。
- 普通领域/转换失败使候选不可提交，即使用户捕获异常。执行前拒绝的 Busy/预算保留候选。只读查询失败独立处理。
- 创建会话、开始 Edit、查询、写入、commit、Job 启动/终态查询及清理丢响应都有恢复归属。`OutcomeUnknown.recover()` 或所属 Edit/Job 按原请求恢复；其他 Edit 不能消费其 pending 请求，也不被这种 Busy 污染。
- Job 完成由宿主主动消费；本地调用、等待、close 和常规原生析构释放 GIL。Job 结果转换到 Python 后才释放 Runtime 结果槽。
- 本地适配没有 JSON 字符串/Value 树往返。实际 Local 调用在禁止 Python `json.dumps/loads` 的测试中完成建模、查询及保存。

当前连接入口也支持环境发现与显式 `start=True` 唯一启动；详细新证据见 Runtime
接回记录。尚未接线的领域不发布空壳 API。

## 2026-10-25 资源与 Plan 切片

提交 `ad868c8a` 保存独立新包基础，`7cf4bed` 保存本节资源与 Plan 切片。本节 19 项行为及初步测量是当时的验收事实；后续生命周期/P0 结果见下节；P2 网格生成是随后开始的独立切片。

- SDK 新增 `ImageCreateSpec`，身份/default 在 SDK；Runtime 增加按 client 持有的 PreparedImage 表，与文档和历史各自持有引用。独立准备失败不污染 Edit，创建/替换失败与其他写入共用候选失败作用域。
- `EditOperation` 包含网格、接纳/替换图片及应用 Plan；两个后端共用 `Runtime::apply_authoring`。批次只推进一次序号；Plan 仅可位于批次开头，后续项可继续编辑。过期、错误 owner 或释放引用使候选不可提交，无正式部分发布。
- 纯 Python `PreparedImage` 与 `Plan` 共用 `_owned.py` 的少量持有/释放/恢复逻辑。像素/报告不可变，报告在过期和 close 后仍可读。准备/规划请求从发送前已有恢复 owner，关闭不能误消费待确认读取，丢失释放响应恢复后仍保持幂等关闭。
- 当前 RGBA 控制负载上限 1 MiB，P4 再做二进制/大图片；`resources.usage()` 是只读 Runtime 总量和真实 Rust 图片缓冲统计，不是进程 RSS 或完整 Rust 堆指标。原生契约更新为 `zipzi-python2-2`，IPC 新能力为 `typed_resources_plans`。
- Release 源码验证通过 **19 项 unittest**（多项跨两后端和多种故障）、严格 Pyright 正例和原有三条精确反例。保留原有 10 项断言，扩展本地禁止 Python JSON 的测试，加入 A/B 独立、接纳后关闭、创建/替换、Plan 来源、失败读隔离、混合回滚、响应恢复、保存重开，以及两后端各 100 次取消后底层条目/存活字节归零。
- Rust 定向测试：Runtime authoring/host/modeling/protocol/resources 共 27 项，以及 SDK `image_edit::` 1 项、`image_memory::` 7 项。SDK identity、像素读回和候选替换有独立固定预期；未运行昂贵的全 SDK/Editor/GPU。
- wheel 分别重建并装入 `target/python2/installed-client` 与 `installed-local`，从 `/tmp` 以 `-I` 运行扩展后的 `installed_recipe.py`。连接环境没有原生包；两者均验证纹理、候选 Plan、网格同一步历史，以及新 Runtime/新宿主从磁盘重开后像素一致。
- P0 新增图片和 Plan 工作负载，采用相同 5 进程 × 32 样本/4 预热。基线见下节；不把尚缺的 Rust 复制/堆峰值或 Connected 性能填成零。


## 验证入口

```sh
# 当前 36 项新包测试，其中多项对 Local/Connected 或多种故障分别验证；加严格正反类型检查。
./tools/test-sdk.sh python2
./tools/test-sdk.sh python2 --release

# 独立共享 Rust 语义和来源/预算。
cargo test --locked -p zipzi-sdk --test integration mesh_editing::
cargo test --locked -p zipzi-sdk --test integration image_edit::
cargo test --locked -p zipzi-runtime --test resources --test modeling --test authoring --test protocol --test host

# 仓库接入和 P0 工具证据检查。
python3 tools/workspace/cargo_domain.py --check
uv run --locked python tools/test_tools.py -p test_python2_tools.py -v
uv run --locked python tools/test_tools.py -p test_runtime_dependencies.py -v

# 互不覆盖 Python namespace 的两个分发。
uv build --wheel --out-dir target/python2/client-wheels crates/zipzi-python2/client
uv build --wheel --out-dir target/python2/native-wheels crates/zipzi-python2
```

已验证 Debug 源码两后端、严格 Pyright 正例以及准确位于三条错误参数行的反例；两个 wheel 已分别构建，并在纯包连接环境和纯包+原生本地环境运行 `tests/installed_recipe.py`。测试从 `/tmp` 使用 `-I`，不插入源码路径。wheel 内容分别为 `zipzi/` 与 `zipzi_native/`。

Rust 聚焦门禁覆盖 SDK 网格编辑及 Runtime 会话/来源/预算/任务；不运行完整 SDK/Editor、GPU 或授权素材验收。当前实际环境为 Rust 1.99.0、CPython 3.14.7、macOS。全仓既有格式差异不在本项目批量改写。

## P1 生命周期补齐与 P0 schema 2

`f2ef33c` 已保存本节 P1/P0 验收；`7cf4bed` 之后新增 [P1 生命周期矩阵](p1-lifecycle-matrix.md)，当前源码行为测试共 36 项。排队取消、进入 IO 后取消、实际 IO 失败、executor 失败、worker panic、保留结果槽准入、无后续调用完成、明确 close 和最后 owner 释放均有对应断言；IPC 另验证请求/客户端过期、暂时离线、宿主重启、丢响应及同一 owner 的旧异常不能消费新请求。

Job 共用 `_recovery.py` 的原请求持有原语，状态查询不释放结果，取消与 close 分开。成功/失败均缓存，Edit 跟随其提交 Job 的终态更新。Connected 的明确正常关闭调用 `release_client`，保留正式 Session；另一请求尚未确认时只断开并等待 grace expiry，不发新写 ID。关闭确认本身丢失仍有恢复 owner。原生契约为 `zipzi-python2-3`，IPC 要求 `typed_job_lifecycle`。

Fault 控制与计数分配器分别位于 `test-support` / `bench-instrumentation`，正常 wheel 不包含控制或探针入口。Runtime CPU fixture 现在真正异步运行 work 并在 idle 时消费 completion，不把 worker 放在请求线程内执行。正常 wheel 的两种独立环境重跑图片、Plan、替换、Undo/Redo、Job 状态/终态取消、保存重开。

最新证据为 [`p0-p1-baseline.json`](p0-p1-baseline.json) 和独立 [`p0-p1-check.json`](p0-p1-check.json)。每个后端 5 个计时进程 + 5 个探针进程，6 项工作负载，每项 4 次预热、32 个计时样本。两种产物和 CPU 宿主均 Release、复制到独立路径并冻结哈希；探针不进入计时二进制。详细统计范围和阈值见计划第 2.2 节及 JSON，后续比较拒绝工作负载脚本/平台/Python 的漂移。

当前冻结基线摘要（CPU fixture，非 Editor）：

| 工作负载 | Local P50 / P95 (μs) | Connected P50 / P95 (μs) |
| --- | --- | --- |
| small_query | 14.1 / 21.6 | 1547.5 / 1592.8 |
| single_edit | 53.5 / 80.3 | 6277.5 / 6518.6 |
| batch_100 | 167.3 / 189.2 | 6712.3 / 8495.4 |
| large_array | 359.2 / 384.1 | 8370.0 / 9952.4 |
| image | 249.8 / 353.5 | 26069.7 / 29991.6 |
| plan | 71.3 / 85.2 | 9290.2 / 9622.4 |

最终客户端的独立复测 `comparison` 为空（通过）；基线与复测各自记录源码哈希，允许客户端修正而拒绝工作负载漂移。普通 Rust 定向门禁 49 项、计数探针 1 项、仓库工具 4 项通过；公共行为 36 项及严格正例/三条精确反例通过，正常 wheel 两种隔离安装通过。

当前指标有真实 Runtime JSON 编解码、Plan 准入序列化、Rust 分配次数/字节与请求堆存活/峰值、原生数值数组物化、显式负载复制、图片资源存活，以及 Python 调用和堆探针。Local transport 的 Rust JSON 编解码/Value 构造为零；Plan 准入的计数式序列化单独报告一次。复制范围包含原生 RGBA/PyBytes、图片 Vec→Arc 和网格快照数组，排除隐式领域 Clone/realloc 内部复制；Rust 堆是 Layout 请求字节，排除 Python/C、分配器元数据和 realloc 内部峰值。数组物化计数仅覆盖原生 Vec 提取，Connected 的 typed JSON 数组转换不在此计数内，零不表示没有物化。Connected 保留空控制窗口校准，不将其隐式扣掉，不把其 CPU fixture 延迟当作真实 Editor 延迟。

仓库工具负控制拒绝空/伪造探针、source/binary/workload 漂移、复制/codec/调用/堆/延迟增长与超阈值噪声。各轮原始 JSON/标准错误留在 `target/python2/p0-p1-final` 与 `target/python2/p0-p1-verified-check`；提交的摘要保存五轮探针及构建/源码身份。旧 schema-1 的 null 指标保持原样，不倒填历史证据。

## P0 首次测量历史


```sh
./tools/test-sdk.sh python2 --release
./tools/test-sdk.sh python2 --release --instrumented
uv run --locked python crates/zipzi-python2/tools/baseline.py --output target/python2/baseline-first
```

已测小查询、单项编辑、100 项批次、位置数组、256×256 RGBA 准备/创建/提交/释放，以及正式 Plan 创建/应用/提交/释放。CPU 采样留待 P5；Connected 性能仍待补测。每次 4 次预热、32 个计时样本，默认 5 次独立进程。计时与 Python profile/tracemalloc 探针分开；校验 Release 构建元数据与实际加载扩展哈希。

2026-10-25 补测记录为 [`p0-resources-baseline.json`](p0-resources-baseline.json)，对应 `ad868c8a` 之后的资源/Plan 工作区及文件内的 Release 扩展、客户端源码及测量脚本哈希；仍为 `partial`。在相同采样规则下，图片工作负载 P50/P95 约 266/385 μs，Plan 约 71/82 μs；单项编辑的进程间 P50 相对范围约 26%，未冻结阈值。新记录补充首次四项基线，不覆盖其历史证据。

探针报告 Python JSON 和原生边界调用，以及 Python 堆存活/峰值。Rust 编码次数、真实复制字节和 Rust 堆指标缺失时为 `null`，不能用逻辑输入大小或 Python 堆指标替代。阈值尚未冻结，结果只能标 `partial`。

## 复杂度记录

新包从零计数，不按其他 Python 实现的接口数量或行数设指标。P1 初始记录时全部为手写实现，尚无绑定生成器；后续 P2/P3 生成范围见对应阶段记录，P3A 再统一公开包装。分别检查 `client/python/zipzi` 的公共/生命周期和 backend/机械转换、`src/lib.rs` 的 PyO3、`tools/`、`tests/`、共享 Runtime/SDK 新增代码及构建元数据。许可副本与文档不计为生产逻辑。

P2 第一批提取了网格字段、操作、codec、查询和便利方法，生成与手写目录分开；资源/Plan 在当时仍手写，随后由 P2 补齐记录接入生成。基础设施（Core 中立形状、Runtime 注册、exporter/renderer）与生成产物分别计数，不把增加的共享 Rust 隐藏成“减少 Python”。独立行为预期不由生成器产生，详见 [P2 记录](p2-mesh-generator.md)。
