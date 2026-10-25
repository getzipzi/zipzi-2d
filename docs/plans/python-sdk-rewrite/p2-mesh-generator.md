# P2 第一批：网格声明与机械接线

基础提交为 `f2ef33c`（P1 生命周期与 P0 门禁）；本记录对应其后的工作区。P2 已开始，当前只验收网格切片，不把资源/Plan 或完整生成器标为完成。

## 唯一字段来源

Core 的中立 `api_shape` 宏从同一声明同时产生真实 Rust 类型和有限形状描述。它不依赖 Python、Runtime 或第三方 IDL。当前选中的类型：

| Rust 声明 | Python 形状 |
| --- | --- |
| Core `Vec2` | `meshes.Point` 二元 tuple |
| SDK `MeshGeometry` | `meshes.Geometry` 不可变数组记录 |
| SDK `MeshCreateSpec` | `meshes.MeshSpec`；SDK serde 决定空纹理与可选 ID 的默认值 |
| SDK `MeshOperation` | `ops.Create/Rename/SetPositions`，带固定 kind |
| Runtime `MeshSnapshot` | `meshes.Snapshot` |
| Runtime `MeshQuery` | 两后端及原生查询参数 |

`SourceRef` 和 Session/Edit token 使用明确的手写适配边界，未将其整个 Rust 内部结构发布为 Python 类型。现有公开导入路径、构造参数顺序与 class module 保持一致；不增加一份 `.pyi`。

Runtime 的 `Registered<MeshQuery, MeshSnapshot>` 要求真实的类型化处理器函数，`MESH_QUERY` 同时被 wire 与原生查询使用。缺少处理器、处理器输入/输出不匹配、字段类型没有形状实现、未支持的 serde 改名/自定义属性，均由 Rust 编译器拒绝。注册不是一张只保存处理器名字的字符串表。

## 生成范围

`examples/authoring_schema.rs` 只选择已审查的类型、公共名称、投影及查询注册。`tools/generate.py` 编译并运行这个 exporter，导出的 `schema/mesh.json` 是中间产物，不是第二份手写规范。Cargo example 不依赖本包的 cdylib，已有原生生成代码过期也能先导出新声明。

生成并纳入仓库的 10 个文件：

- `schema/mesh.json`：带 ownership/contract 摘要的中间结构。
- `client/python/zipzi/_generated/`：网格值、操作、wire/表达转换、后端与 native protocol 签名、查询接线、便利方法。
- `src/generated/`：直接 PyO3 结构转换和查询入口；依然通过 LocalHost 进入同一 Runtime。

公开 `meshes.py` 和 `ops.py` 是薄导出与资源/Plan 手写部分。`_lifecycle.py` 仅将网格便利方法交给生成 mixin，状态机、恢复、清理、Job、Plan/PreparedImage 持有、GIL/线程调度、预算和领域算法继续手写。

生成器使用固定目录与目标清单，先验证完整修改集再写文件。非法标识符、未知形状、未选类型、错误默认值、缺失处理器元数据、非受管文件、手写内容和越界/symlink 目标均拒绝。`--check` 比较产物但不修复或写入源文件。普通安装与运行不执行生成器。

当前支持的是实际试点所需的 string/u32/f32、显式 optional、记录、带标签的 Mesh 联合、u32/Point 序列和三元 u32 triangle。不是任意 Rust 语法/泛型的翻译器；bool/u64、受限映射、bytes/资源引用和更多 Plan 输出形状仍待下一批评审与接线。

## 验证

```sh
uv run --locked python crates/zipzi-python2/tools/generate.py
uv run --locked python crates/zipzi-python2/tools/generate.py --check
uv run --locked python tools/test_tools.py -p test_python2_codegen.py -v
./tools/test-sdk.sh python2 --release
./tools/test-sdk.sh python2 --release --instrumented
cargo test --locked -p zipzi-sdk --test integration mesh_editing::
cargo test --locked -p zipzi-runtime --test authoring --test protocol
```

源码 runner 已自动执行生成漂移检查。6 项仓库工具用例独立验证确定性、只检查不写入、先检查再写入、保护手写文件、拒绝文件及父目录 symlink、拒绝非法声明、同一字段变化传播到 Python/wire/原生，以及真实 Rust 正/反编译（含结构/字段/枚举 serde 改名拒绝）；没有生成行为预期。原有 36 项两后端行为与故障用例、严格正例和三条精确类型反例继续通过。SDK 8 项网格聚焦测试及 Runtime authoring/protocol 11 项通过。

[P0 独立比较](p0-p2-mesh-check.json)使用原冻结基线与原工作负载脚本，5 个计时和 5 个探针进程分别覆盖两后端；`comparison` 为空，噪声未超阈值。Rust JSON 文本编解码/Value 构造在 Local 路径仍为零。复制、物化和堆指标继续沿用 P0 的明确统计范围，不以代码行数或逻辑输入字节代替。

正常 wheel 在纯包连接和纯包+原生的隔离环境运行同一安装 recipe；生成的 Python 模块直接打入纯 wheel，原生 wheel 编入生成 Rust。发布平台、完整 sdist/源归档与真实 Editor 安装矩阵仍归 P6，不以源码验证冒充发布验收。

## 下一步与复杂度

本批删除了网格字段、操作、转换、查询签名及便利方法的多处手写副本，同时增加了中立 Rust 形状设施、编译注册、exporter、Python renderer 和负控制测试。基础设施增加是实际成本，不宣称第一批总代码行数减少。

P2 仍需覆盖已验收的资源/Plan 形状，补受限 map、bytes 和资源 token 适配，扩展类型/codec 独立用例及 source/sdist 配对证据。达到计划列出的完整 P2 门槛前，保持“第一批网格链路已验收、P2 进行中”。
