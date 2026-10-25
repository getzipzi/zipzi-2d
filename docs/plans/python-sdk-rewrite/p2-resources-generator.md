# P2 资源、Plan 与源码发行补齐

日期：2026-10-25。基于 `3d1b1eef` 后的工作区，接续[网格生成历史切片](p2-mesh-generator.md)。
最终功能、安装和性能结论统一见 [R1–R3 / P1–P2 整体验收](../sdk-runtime/r1-r3-python-p1-p2-acceptance.md)。

## 单一声明与生成范围

Core 的 `ApiShape` 继续只表达有限结构。SDK 的 `ImageCreateSpec`、`ConversionCost`、
`ModelingPlanReport` 与 Runtime 的图片快照、Plan 意图/快照、资源操作、资源统计和查询参数，
均由实际 Rust 类型声明导出。新增形状为 bool、u64/usize、拥有的 `Arc<[u8]>` 和
`BTreeMap<String, String>`；不接受任意映射或动态值。既有点、数组、Option、record、
带标签的 union 和不透明身份继续使用原有限机制。

`IMAGE_QUERY`、`PREPARED_PIXELS_QUERY`、`PLAN_QUERY` 是可执行的编译注册，
将实际输入/输出类型与 Runtime 处理器绑定。Local 与协议入口共同调用这些注册。
`ResourceOperation` 是统一 `EditOperation` 的资源分支；保持既有 wire 标签，
仍由同一个 Runtime 候选执行器处理混合批次、失败与提交。

| 生成产物 | 内容 |
| --- | --- |
| `resource_values.py` | ImagePixels、Snapshot、Usage 的不可变值和读取 |
| `plan_values.py`、`plan_intents.py` | Cost、Report、两种 RenameIntent、校验和 wire |
| `resource_ops.py`、`resource_conveniences.py` | CreateImage/ReplaceImage/ApplyPlan；创建/替换便利方法构造同一操作 |
| `resource_backends.py` | 接口、原生协议与两后端图片/像素/Plan 查询接线 |
| `resources.rs`、`resource_queries.rs` | 直接 PyO3 读写、拥有的 bytes、共享注册调用与 GIL 释放 |

加上既有网格输出共 **18 个受管文件**。`schema/mesh.json` 保留既有路径，增加资源子契约；
它是编译导出产物，不能手改。`generate.py` 在写入前检查所有目标的归属标记。
`--check` 只检查，固定来源重复生成必须一致。

公开导入仍为 `zipzi.meshes/resources/plans/ops`，包括 pickle 的类身份和
`typing.get_type_hints` 可解析性。公开图片构造保持 `CreateImage(name, image, id=None)`，
由明确的字段投影策略展开 SDK spec；不在生成器内复制这些字段声明。
Plan Report 的 Source 来自包围它的 Runtime PlanSnapshot，保留既有读取契约。

## 手写边界与修复

PreparedImage/Plan 所有权、预算预检、恢复归属、释放、Edit/Job 状态机及领域算法继续手写。
生成器不接管准备图片的内存准入，也不创建第二套事务入口。后续参数、绑定和 CPU 采样
不算入本阶段生成覆盖；没有为未实现领域生成空壳。

资源 Usage 原来依赖手写的嵌套诊断字段映射。本轮增加实际 `ResourceUsage` Rust 类型，
Local 直接转换，Connected 返回同一扁平字段；诊断统计保留原有内容。
新原生契约为 `zipzi-python2-7`，连接端要求 `typed_resource_values`，避免新客户端误连
缺少该返回契约的宿主。未知结果恢复仍消费原响应，没有另一条特例转换路径。

复验还修复了两处已存在问题：确认恢复到 Busy 后 Edit 未回到可重试状态；Runtime
接口清单未识别新增 SDK 模块。详细复现、回归及源码见整体记录。

## 源码发行

纯 Python 与原生为两个分发。纯包 sdist 携带 Python 来源和生成产物；原生 sdist
还携带 SDK 路径依赖闭包、相同 Python 来源、Rust 声明、编译导出器和生成器。
这一配对源码集合能离开 Git 工作树检查生成并独立重建两种 wheel。

实际验证发现 Maturin 虽收集了 SDK 依赖，仍保留根 Cargo 中指向未打包 Editor vendor
的 patch，导致解包构建失败。`build_backend.py` 仅在生成 sdist 时移除明确列出的
桌面 patch，离线裁剪锁文件并检查依赖 name/version/source 没有新增或变化；遇到其他
patch 拒绝打包，要求显式审查。wheel 构建直接交给 Maturin，不运行生成器。

`tools/distribution.py` 在新目录构建、解包并校验两个 sdist，从解包源码重建正常 wheel
及 CPU 宿主，然后在纯包连接环境、纯包加原生本地环境运行归档内的独立安装 recipe。
构建缓存隔离于仓库 target，避免解包源码覆盖日常测试产物；所有命令和产物 SHA-256
写入结果。测试期间也发现了共用 Cargo target 造成 doctest 找不到 SDK 的工具问题，
隔离后 Runtime 完整门禁重新通过。

## 独立验证

源码行为为 52 项，包含既有固定像素/几何/历史和恢复断言；新增用例经真实原生转换验证
非空有序映射、大整数、可选 cost/bool、bytes、公开身份和类型注解。
Pyright 正例无错误，反例精确检查 12 条实参类型诊断。
生成器工具测试实际改变 Rust 形状，检查字段传播至值、wire、原生和便利方法；
错误处理器签名、未支持映射和无效绑定在编译或生成时拒绝。
行为预期、故障注入、安装 recipe 和性能门槛始终独立于生成器。

验证入口：

```sh
./tools/test-sdk.sh python2 --release
./tools/test-sdk.sh python2 --release --instrumented
uv run --locked python tools/test_tools.py -p 'test_python2*.py' -v
uv run --locked python crates/zipzi-python2/tools/distribution.py --output target/python2/source-acceptance
```

复杂度按责任在整体验收时统计：手写公共/生命周期、手写适配、生成产物、
生成器/工具/测试以及共享 SDK/Runtime 分列。增加生成器和 Rust 声明的成本没有隐藏；
本阶段不承诺某个删行比例或把后续领域算入生成完成量。
