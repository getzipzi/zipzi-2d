# zipzi-python2 实施记录

日期：2026-10-25。对应[建设计划](../python-sdk-rewrite-plan.md)及[独立新包](../../../crates/zipzi-python2/README.md)。

## 项目边界

本项目从新包 `crates/zipzi-python2` 开始，Python 导入名为 `zipzi`。P0/P1 的公共类型、生命周期、两后端、薄 PyO3、包级测试、构建和测量工具均已移入新目录。旧 crate 中的临时特性接线和源码改动已撤回；旧接口去向清单和旧包性能采集工具不再属于本项目。

可复用的是共享 Rust SDK/Runtime 和独立 Rust 测试。`MeshCreateSpec`、`MeshOperation` 及默认值/ID 分配在 SDK；`LocalHost`、候选写入、来源、任务与预算在 Runtime。新包仅适配这些类型化处理器。共享层的新增代码单独计数。

## 阶段状态

| 阶段 | 已实现 | 尚未满足的退出门槛 |
| --- | --- | --- |
| P0 | 新包边界、两分发配置、选定输入/结果/失败契约、能力范围；新包自身的固定性能采集入口 | 全部工作负载接线、Rust 编码/实际复制/存活峰值指标、噪声调查与阈值冻结 |
| P1 | 纯 Python 公共类型/异常/包装、薄 PyO3、两后端；网格、图片准备/创建/替换/查询、正式/候选 Plan、混合批次、历史、保存重开和响应恢复 | 完整取消/worker 失败/请求过期、客户端过期和宿主重启的 Python 矩阵 |
| P2–P5/P7 | 未开始 | 按计划交付 |
| P6 | 纯包/原生包分别构建；初步隔离安装 recipe | sdist、完整版本/平台配对、宿主发现、真实 Editor 和全部领域安装验收 |

能力范围见 [`CAPABILITIES.md`](../../../crates/zipzi-python2/CAPABILITIES.md)。源码接线、独立验证与完整阶段验收分开记录；P0/P1 均未标为完成。

## P1 当前链路

- `local()` 显式拥有进程内 Runtime，`connect(endpoint=...)` 绑定现有宿主。两者进入同一 Rust 处理器。
- `MeshSpec` 创建与 `Rename/SetPositions` 修改意图分开；省略的持久 ID 由 SDK 生成。便利方法构造同一操作，显式批次按序执行并单独提交。
- Snapshot 拥有不可变 tuple 数据及确切 formal/edit 来源。候选序号、Runtime/Session、正式版本与候选身份都在测试中断言。
- 普通领域/转换失败使候选不可提交，即使用户捕获异常。执行前拒绝的 Busy/预算保留候选。只读查询失败独立处理。
- 创建会话、开始 Edit、查询、写入、commit、Job 启动/终态查询及清理丢响应都有恢复归属。`OutcomeUnknown.recover()` 或所属 Edit/Job 按原请求恢复；其他 Edit 不能消费其 pending 请求，也不被这种 Busy 污染。
- Job 完成由宿主主动消费；本地调用、等待、close 和常规原生析构释放 GIL。Job 结果转换到 Python 后才释放 Runtime 结果槽。
- 本地适配没有 JSON 字符串/Value 树往返。实际 Local 调用在禁止 Python `json.dumps/loads` 的测试中完成建模、查询及保存。

当前连接入口需要已有的绝对 socket 路径。尚无宿主发现/启动流程；没有 GPU 或其他未接线领域的空壳 API。

## 2026-10-25 资源与 Plan 切片

提交 `ad868c8a` 已保存独立新包基础。本节是该提交之后的工作区验收，不能用旧提交重现本节新增功能；尚未生成 P2 绑定。

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
# 19 项新包测试，其中多项对 Local/Connected 或多种故障分别验证；加严格正反类型检查。
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

## P0 自身测量

```sh
./tools/test-sdk.sh python2 --release
uv run --locked python crates/zipzi-python2/tools/baseline.py --output target/python2/baseline-first
```

已测小查询、单项编辑、100 项批次、位置数组、256×256 RGBA 准备/创建/提交/释放，以及正式 Plan 创建/应用/提交/释放。CPU 采样留待 P5；Connected 性能仍待补测。每次 4 次预热、32 个计时样本，默认 5 次独立进程。计时与 Python profile/tracemalloc 探针分开；校验 Release 构建元数据与实际加载扩展哈希。

2026-10-25 补测记录为 [`p0-resources-baseline.json`](p0-resources-baseline.json)，对应 `ad868c8a` 之后的资源/Plan 工作区及文件内的 Release 扩展、客户端源码及测量脚本哈希；仍为 `partial`。在相同采样规则下，图片工作负载 P50/P95 约 266/385 μs，Plan 约 71/82 μs；单项编辑的进程间 P50 相对范围约 26%，未冻结阈值。新记录补充首次四项基线，不覆盖其历史证据。

探针报告 Python JSON 和原生边界调用，以及 Python 堆存活/峰值。Rust 编码次数、真实复制字节和 Rust 堆指标缺失时为 `null`，不能用逻辑输入大小或 Python 堆指标替代。阈值尚未冻结，结果只能标 `partial`。

## 复杂度记录

新包从零计数，不按其他 Python 实现的接口数量或行数设指标。当前全部为手写实现；尚无绑定生成器或生成绑定。分别检查 `client/python/zipzi` 的公共/生命周期和 backend/机械转换、`src/lib.rs` 的 PyO3、`tools/`、`tests/`、共享 Runtime/SDK 新增代码及构建元数据。许可副本与文档不计为生产逻辑。

目前仍有 P1 手写字段/codec 的重复声明；P2 只能在这些行为测试保持独立的前提下提取生成，不先扩张 schema 来提高生成比例。
