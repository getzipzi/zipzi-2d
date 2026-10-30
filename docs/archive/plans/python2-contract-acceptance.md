# Python 契约拆分：阶段 6 验收

日期：2026-10-30。对应[拆分方案](python2-contract-extraction.md)。

## 环境与方法

本机为 Apple M4、16 GiB 内存、macOS 27.0.1；Rust/Cargo 1.99.0，
CPython 3.14.7。CI 固定使用项目最低工具链 Rust 1.95.0。
测量均使用同一机器和 dev 配置。性能测量与集成构建分开执行；各热场景预热后
取五次样本，保留中位数、最小值、最大值和各阶段耗时。
`CARGO_BUILD_JOBS=2`，沿用本机 sccache；冷构建指新的 Cargo target，未清空
全机编译器缓存。拆分前基线从提交 `4c485058` 的源码归档重建，属于本次重建
对照，不冒充实施前已经保存的计时；拆分主体位于 `a6e24cda`。

`tools/python/python2_codegen_acceptance.py` 复制当前源码及真实 workspace
依赖图，不复制 target、机器配置、Git 数据或模型。所有修改发生在副本中。
实际生成入口的导出、渲染／缓存和输出比较分别计时；Cargo 只增加 verbose
诊断以记录 Fresh／Compiling，不跳过导出、校验或输出归属检查。

```sh
uv run --locked python tools/python/python2_codegen_acceptance.py --output target/python2/contract-acceptance
```

## 正确性与隔离

- 从 Cargo 实际解析的 normal/build 依赖图检查所有 feature 和 target。
  仓库包仅包含 `zipzi-contract`、`zipzi-api-derive`，没有 PyO3 或 GPU 库。
- 分别改变 Core draw-order、SDK canonical-edge、Runtime sampling 的实际函数
  表达式。默认检查仍通过，没有 Cargo 包重编，所有生成文件字节不变。
- 修改真实 Rust 契约字段、默认值、枚举分支和操作输入／输出字段，检查报告漂移且
  不写生成文件；重新生成后检查通过，恢复源码后输出也恢复。
- 删除渲染缓存、强制不使用缓存仍能检查当前契约。已有成功缓存不能掩盖
  `compile_error!`；修复契约后默认检查恢复。
- 真实 Rust 编译负例覆盖 handler 输入／输出不匹配、遗漏绑定和重复操作。
  临时 checkout 路径包含空格和中文；外部 `CARGO_TARGET_DIR` 没有被使用。

迁移前后 schema 逐项比较只允许已登记的变化：321 个 Rust 类型路径映射
（出现 2,415 次）、9 个二进制签名、两处协议 v1 → v2，及从 sampling 数据投影
移出的 Runtime lease。其余字段、默认值、Serde 标签、顺序和结构完全相同。
比较没有忽略整个 schema 区块。

重建迁移前版本的默认值变更实验还确认了旧 derive 的一个问题：容器上的
`#[serde(default)]` 使用字段类型的 `Default`，没有读取容器自身 `Default`
实现的字段值。修改 `GeometryChecks::default()` 后旧 schema 不变，因此该场景
不能作为正确生成的速度对照。阶段 1–5 已修复这一点；当前真实 Rust 默认值
变更的漂移／重生成测试通过。原始默认值恰好相同，所以该修复不扩大上面的
基准 schema 差异清单。

原 schema SHA-256：
`46e509f1acc78586f5c50fa2553cef518a408247cd90f8d1a6479a5f3d1f8244`。
当前 tracked schema（含生成归属标记）SHA-256：
`f2403586a5e3a0429b2396ea775101f9d3031608d1e2afbc185cab93de4eba66`。
原始对照、321 项映射和比较脚本保存在 `target/python2/contract-baseline/`。

## 集成

验收日志、源码副本、构建命令和机器可读结果保存在 `target/python2/phase6/`。

| 门禁 | 结果 |
| --- | --- |
| `cargo test --locked -p zipzi-contract` | 9 个测试通过；覆盖默认值、标签、字段顺序、操作 shape、二进制布局／预算与 SharedVec 共享／写时复制 |
| codegen 工具测试 | 原有 26 个用例及新增真实 checkout 隔离用例通过 |
| distribution 工具测试 | 7 个通过；检查缺失／过期的契约 include、derive、模板及锁辅助源码 |
| package-domain、release version 与相关工具 | 18 SDK + 3 Editor，版本及打包检查通过 |
| Python Local／Connected 与类型 | 103 个 API、16 个 recovery 用例及正／负静态类型检查通过 |
| 安装客户端与进程连接 | 20 个通过 |
| GPU observation | 8 个通过 |
| Runtime GPU observation（`--include-ignored`） | 3 个通过，包含默认 ignored 的两个快照／图像用例 |
| Editor fast | 615 个通过 |
| mesh renderer 聚焦测试 | 4 个通过 |
| SDK regression | 93 个测试目标，1,167 个通过，0 失败，5 个既有 ignored |
| macOS Editor／renderer 全目标消费者编译 | `test-support` + `wgpu-backend`，通过 |
| 归档源码、sdist／wheel | 15 步全部通过；client/native 均从归档构建，并在隔离解释器运行 Local／Connected recipe |

归档检查既运行默认生成入口，也重放从归档 release 导出器得到的 schema；未引用
活跃 checkout 的源码或导出结果。wheel 构建与安装继续不运行生成器。

SDK regression 的 ignored 分别是需要私有 PSD 素材的一个用例、两个显式 GPU
用例及两个 release 性能测量。GPU 两例另行以 `--include-ignored` 运行通过；没有把 ignored
计入通过数。日常 `alpha_mesh_stress::` 仍按门禁既有规则过滤。

集成中修复两个漏迁的测试构造调用，使用实现层的 `ReferenceEntry::new` 分配身份。
另修复共享 mesh renderer 对应用 benchmark 模块的直接依赖：由应用初始化其
一次性诊断选项，使独立 render 测试和示例可以编译，默认绘制策略保持一致。

CI 的 tooling 门禁包含契约／codegen／打包检查并提供 rustfmt；专用 schema target
按固定版本 [rust-cache 配置实现](https://github.com/Swatinem/rust-cache/blob/6323deb102c322ba6fcbdcafc7e3dddab59af2b6/src/config.ts#L128-L138)
独立缓存。macOS／Windows consumer matrix 都执行契约测试，发布路径过滤已覆盖
`crates/**` 和 `tools/**`。本机执行的是 macOS 消费者，Windows 执行结果需由 CI
提供，不能把 matrix 配置视为已执行的 Windows 验收。

## 性能

单位均为秒。热场景每行五次，冷构建每行一次。

| 场景 | 拆分前中位数（范围） | 当前中位数（范围） |
| --- | --- | --- |
| 无改动 `--check` | 0.758（0.745–0.801） | 0.512（0.495–0.572） |
| Core 实现表达式改变后检查 | 未单独测量 | 0.503（0.499–0.528） |
| SDK 实现表达式改变后检查 | 5.363（3.489–7.815） | 0.491（0.485–0.508） |
| Runtime 实现表达式改变后检查 | 未单独测量 | 0.514（0.490–0.535） |
| 契约新增字段后编译并生成 | 未单独测量 | 5.518（4.942–6.351） |
| 契约容器默认值改变后编译并生成 | 旧版未正确发现变化，不作速度对照 | 4.587（4.542–4.671） |
| Cargo target 冷构建并检查 | 72.670 | 25.401 |

当前无改动及三个实现修改场景的 19 个 Cargo 包全部 Fresh，重编包为零；
`zipzi-contract` 库和导出二进制均复用。拆分前 SDK 修改后会重编 `zipzi-sdk`、
`zipzi-runtime`、`zipzi-observe`、`zipzi-python2`。当前契约字段或默认值改变只
重编 `zipzi-contract`；冷构建仅有该 crate、derive 和 17 个轻量通用依赖。
实现修改后的检查接近无改动基线，五次中位数均低于 5 秒目标。

阶段耗时单独取中位数，不应机械相加当成总耗时：

| 当前场景 | Cargo 导出与 JSON 读取 | Python 实际渲染 | 指纹／缓存与渲染合计 | 输出比较／写入 |
| --- | --- | --- | --- | --- |
| 无改动 | 0.339 | 0（命中缓存） | 0.100 | 0.022 |
| SDK 实现修改 | 0.335 | 0（命中缓存） | 0.096 | 0.021 |
| 新增契约字段 | 3.521 | 1.410 | 1.576 | 0.027 |
| 契约默认值修改 | 2.750 | 1.389 | 1.511 | 0.025 |
| 冷构建 | 23.271 | 1.571 | 1.716 | 0.038 |

另在已验收的 native 源码归档中，把 SDK `canonical_edge` 的 min/max 调用等价
改写为 `std::cmp::min/max`，测量实际完整循环。源码字节确实改变；随后恢复副本，
没有修改开发 checkout 的实现文件。

| 完整循环（一次，release native / Local） | 秒 |
| --- | --- |
| 默认生成检查 | 0.695 |
| 重建 native wheel | 72.934 |
| 新建隔离解释器并安装配对 wheels | 0.143 |
| 已安装包的 Local 聚焦 recipe | 2.742 |
| 总耗时，含少量编排开销 | 76.516 |

生成阶段的隔离收益没有消除执行实现的 native 构建成本。这里保持 Local 执行
路径；Connected 的归档安装验收另行通过，没有将宿主选择当成性能优化。

五次原始样本、每次 Cargo Fresh／Compiling 包和阶段数据分别见
`target/python2/phase6/before/results.json`、`performance/results.json`；完整循环
见 `full-cycle/results.json`。基线旧默认值实验的正确性失败也保留在报告中。
