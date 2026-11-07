# Authoring 阶段 5 旧实现清理与正确性回归

2026-11-07 完成，阶段提交为 `c7ca7fe9`。[架构方案与阶段索引](../authoring-architecture.md#实施记录)

各批次的迁移边界与验证结果按当时状态记录，阶段最终结论见收尾与验收。

## 第一批 执行源码与工具入口

- 删除未接线的 SDK 事务、checkpoint 历史、旧保存路径，以及 Runtime 的 Session 搬运、
  字段注册和旧准入模块。清理 Core 的旧事务返回值、Contract 的旧地址与 wire 操作、
  Python 的旧手写代理和未编译的原生桥接。
- 保留正在使用的观察输入和纹理报告，放入 Contract 的 observation 模块并重新生成 Python
  schema。Project 移除目录版/CBOR 归档原型，原生 v22 路径继续使用同一 ZIP 校验、预算与
  文件发布流程。
- SDK Runtime 门禁使用常驻宿主、CPU 观察拒绝、传输和来源身份用例。Python 源码及隔离
  wheel 门禁切换为 v4 用例，覆盖分页来源、detach/close、批次和丢失回执的恢复。
- 迁移空图片原生往返测试与周期参数求值测试。空图片重开后保留空记录并报告
  `MISSING_IMAGE_CONTENT`，没有为它创建 PNG。

2026-11-06 首批验证（macOS）：

| Gate | 结果 |
| --- | --- |
| Workspace 产品库和可执行入口检查、本批四个迁移示例编译 | 通过 |
| Contract 全 crate | 16 项通过 |
| Core lib、周期参数、PreviewState | 50 / 8 / 4 项通过 |
| Project lib、原生和格式往返 | 70 / 4 项通过 |
| SDK lib、authoring、alpha mesh | 72 / 32 / 39 项通过 |
| Runtime CPU 库与指定集成目标、Metal 观察库 | 50 / 44 项通过 |
| EditorCore `authoring_kernel` | 6 项通过 |
| Python 源码门禁、生成和类型、隔离 wheel Connected | 23 / 9 / 13 项通过 |
| 包域、改动文件 Rust 格式、`git diff --check` | 通过 |

迁移范围：本批先清理执行源码与当前工具入口。旧聚合测试及若干基准仍引用已删除的接口，
主要涉及 Core 领域夹具、SDK 与 Runtime 集成、Project/格式、Editor 和旧 Python 用例。
全工作区 `--all-targets` 检查仍失败，聚合目标继续保留供后续逐项迁移。Windows、WGPU
运行与完整桌面验收尚未执行。第五阶段尚未结束。

## 第二批 Core、Animation 与 Model3

- Core 领域及求值夹具直接使用 DocumentBuilder 和冻结模型。保留 Mesh/顶点、普通绑定、
  BlendShape、Glue、Offscreen、Warp/Rotation、CDI 和求值失败边界。旧事务测试改为
  检查候选失败隔离、净 WriteSet、稳定身份和快照保活。
- 预览缓存用例以 ContentId 为边界，继续检查同一快照的缓存复用和热求值的分配行为。
  持久 appearance 保留大于 1 的有限 opacity 值，以支持格式往返。
- Animation 集成套件使用冻结模型与格式导入值，保留官方 CPU 轨迹、循环、淡入淡出、
  Physics/Pose 组合、seek cache、原始曲线采样及轨道共享断言。Core 和 Animation
  的现有基准入口改用快照构造，集中对照仍属于第六阶段。
- Model3 包装与附件回归改用 ProjectImporter、ProjectIo 与 TransferSnapshot。
  MOC3 的 v6 损坏输入检查使用带真实纹理捕获的导入夹具。

2026-11-06 第二批验证（macOS）：

| Gate | 结果 |
| --- | --- |
| `cargo test --locked -p zipzi-core -p zipzi-animation --no-fail-fast` | Core 112 项、Animation 42 项通过 |
| 两个 crate 的 `cargo check --locked --all-targets` | 通过，包括 Core 两个基准目标和 Animation 示例 |
| Animation `storage_benchmark` | 冒烟运行通过 |
| MOC3 `--no-default-features --test safety_tests`，强制关闭原生 Core 验证 | 6 项通过 |
| Project `--test model3_tests` | 4 项通过，包括源文件删除后的附件导出 |
| 包域、改动文件 Rust 格式、`git diff --check` | 通过 |

剩余工作主要涉及 Project/格式的其他旧用例、SDK/Runtime 聚合目标、Editor 用例及旧客户端用例。
本批验证范围为上述目标，Workspace 聚合门禁和平台验收仍待收尾。

## 第三批 CMO3 与 Project 格式回归

- CMO3/CAN3 全部集成用例和两个公开示例改用 DocumentState/Builder；保留独立 CAFF
  校验、无界画布、纹理 padding/原始 UV、普通/BlendShape 插值、Rotation 单位、
  曲线量化/烘焙、Expression 语义和格式损失断言。动画导入显式接收返回的冻结状态。
- 低层 CMO3 解码的图片保持未解析描述；重新导出的测试先解码并验证其自有 PNG，
  建立解析后的资源描述，再交给 AssetResolver。没有放宽缺图或严格导出检查。
- Project 的 CDI、Expression、Motion、Pose、Physics、CMO3/Cubism 发布测试移除
  DocumentSession/DocumentStore 和旧公开 JSON codec 调用，改走 ProjectImporter、
  ProjectIo、ResourceView 与 TransferSnapshot。持久化断言实际保存/重开 v22 ZIP；
  损坏文档测试重新计算容器 hash，确保检查到文档准入而非提前失败于 ZIP 完整性。
- 保留引用保护、opaque 命名空间/内容限制、关键帧稳定身份、损坏附件导入不返回半成品、
  成组发布保留无关文件等断言。旧 checkpoint/modified 检查改为冻结来源身份和内容检查；
  CDI 的已退役 CBOR 原型用例随实现删除。更新两个 crate README 的门禁和迁移边界。
- 本地 Mao 用例直接使用捕获资源，实际验证 260 Mesh、175 Transform、124 BlendShape
  binding、7 Motion、8 Expression 和 15 个求值姿态；保存后 CMO3/CAN3 与直接导出一致。

2026-11-06 第三批验证（macOS）：

| Gate | 结果 |
| --- | --- |
| CMO3 lib 与全部集成目标 | 47 项通过；首轮失败的 export/import 目标修正夹具后分别 11 项通过 |
| `cargo check --locked -p zipzi-cmo3 --all-targets --features test-support` | 通过，含两个公开示例 |
| Project `cdi_tests` / `expression_tests` / `motion_tests` / `pose_tests` / `physics_tests` | 9 / 3 / 6 / 3 / 3 项通过 |
| Project `cmo3_tests` / `cubism_tests` | 3 / 3 项通过，含三个入库模型的原生保存与导出一致性 |
| Project `mao_cubism -- --nocapture` | 本地素材实际执行 1 项通过；不是缺素材跳过 |
| 包域、改动文件 Rust 格式、`git diff --check` | 通过 |

本批验证 78 项格式/Project 回归。第五阶段仍未完成：Project 的 `project_tests`、
`sequence_tests`，其余 MOC3/SDK/Runtime/Editor 遗留聚合目标及旧客户端用例继续迁移。
本批开始时的 workspace `--all-targets` 检查仍失败；不将 focused 结果表述为工作区通过。
未运行 Windows、WGPU、外部 Cubism Editor 或完整桌面交互验收。第六阶段集中性能对照尚未开始。

## 第四批 聚合测试与桌面迁移

- Project 和 MOC3 的集成测试、示例及基准改用冻结文档、捕获的资源版本和
  原生 ZIP 接口。两 crate 的 `--all-targets` 编译通过，集成及库测试共 212 项通过。
- SDK 的场景、绑定、动画、资源、历史和删除回归迁到 Draft 与稳定 ID。
  删除评审保留转换完成后发现的类型化锁阻塞项。SDK 全目标编译通过，
  库与集成测试共 610 项通过、2 项忽略。
- 手工网格拖动重新从固定手势快照采样。每次预览仍检查当前候选来源，
  使用上一有效位置检查移动路径，拒绝后可继续拖动。
- Runtime 的默认全目标测试通过 143 项，启用 GPU 观察并执行忽略用例后通过 147 项。
  回归覆盖 v4 读取和预览、客户端归属、迟到结果、任务取消、真实归档的跨会话路径锁、
  指纹冲突和解压预算。超长命令在占用 Draft 前拒绝，动画事件和发布事件在展开前检查输出预算。
- EditorCore 的聚合测试与真实工程验收目标已迁到冻结快照、捕获资源和 Runtime Draft，
  库测试 104 项通过。迁移期间修复了属性命令在 Runtime 写锁中重新读宿主导致的卡死，
  以及结束失败手势时读取已终止 Draft 的问题。聚合行为、真实工程与桌面验收仍在验证。
- 阶段完成需重跑工作区、客户端及相关行为门禁。阶段六基线已从固定提交
  `0262b010` 在独立工作树构建 Release 探针，并完成本地 Mao 工程的基线冒烟
  和保存后重开检查。

## 收尾与验收

- Editor 用发布的净 WriteSet 驱动投影。已知原始快照时只更新受影响的对象或关系；
  遇到结构变化时重建相关投影。Undo/Redo 同样使用准确的变更前快照。
- 桌面原生测试、Metal 夹具、跨进程 Python 脚本和工作区基准入口已迁移。
  IPC 用例覆盖候选采样、真实 UI 更新、路径占用、人工手势与外部发布竞争、
  两客户端接力、共享历史，以及保存后重开和动画音频导出。
- 旧 Python 门面和协议夹具退出当前测试入口。领域数值与故障回归保留在 Core、
  Project、SDK、Runtime 和 Editor。Python 覆盖生成类型、Local/Connected 源值、
  mesh/资源版本、客户端启动、历史、回执恢复、真实 Editor 和原生扩展版本切换。
- 当前 API inventory 和源归档使用 v4 生成契约。R0/P3A 等历史证据仍按原始提交和输入复核。

| Gate | macOS 实测 |
| --- | --- |
| 工作区 `--all-targets`，以及 `--all-targets --all-features` | 通过 |
| SDK 行为与压力回归 | 610 项通过，2 项 opt-in 性能测试忽略 |
| Runtime 启用 Metal | 147 项通过 |
| EditorCore `--lib` / `authoring_kernel` / `integration` | 104 / 6 / 640 项通过 |
| EditorCore 真实工程 `acceptance` | 默认 19 项通过，另跑 heijiao PSD 导入、裁剪及保存重开 1 项通过 |
| 桌面 UI 与 EditorRender 库 | 桌面 704 项、EditorRender 10 项通过，4 项 opt-in 忽略 |
| 桌面 Metal `native_render` 与 WGPU | Metal 28 项通过、2 项忽略，WGPU 20 项通过 |
| 桌面截图、动画、Timeline、Python IPC 与接力 | 默认截图集、三个本地动画模型、真实 Mao Timeline 和两个桌面 IPC 入口通过 |
| Python 源码、扩展版本切换、codegen/类型 | 29 / 1 / 10 项通过 |
| 隔离 wheel、端点启动、生产宿主、宿主网格服务 | 17 / 15 / 11 / 1 项通过 |
| 从源归档构建的 Local/Connected wheel | 源码生成重放、安装及 Metal 观察配方均通过 |
| 编译契约隔离 | 源码副本中更改 Core/SDK/Runtime 实现，生成产物逐字节一致；更改字段、枚举与操作输入可检测漂移 |
| 工具回归、包域、Editor 所有权检查、`git diff --check` | 283 项及检查通过 |

D1–D13 已核对。生产源码没有旧事务、checkpoint、活文档 Project 会话、
worker Session 搬运、字段句柄注册和 v3 门面。保留历史报告及其原始输入，
格式编解码和领域验证继续通过新内核调用。Windows 与外部 Cubism 应用未运行。
额外的 Mao 多路线五轮审计运行了工程种子的原生 ZIP、Cubism 和 Runtime 单格式路线，
其中五次原生 ZIP 往返均精确稳定。组合路线和第二种输入未跑完，结果未计入上表。
全仓 Rust 格式检查仅剩原有的桌面 `settings.rs` 和 `headless_tests/mod.rs` 差异。
两项已有图标删除未计入阶段提交。
