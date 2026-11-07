# Authoring 阶段 4 宿主与客户端切换

2026-11-06 完成，阶段提交为 `3395e4a2`。[架构方案与阶段索引](../authoring-architecture.md#实施记录)

各批次的迁移边界与验证结果按当时状态记录，阶段最终结论见收尾与验收。

## 第一批 Observe

- Observe 参数/动画捕获、纹理规划、CPU 准备、GPU 渲染、拓扑/质量与工程预览直接消费
  SourcedSnapshot、DocumentState 和 ResourceView，删除旧 AuthoringSnapshot、图片路径、
  asset_root 和 ProjectResourceLease 依赖。工作输入只持有不可变值与实际资源版本。
- 动画预览绑定完整 QuerySource；批次/动画匹配同时检查来源与 ContentId。不同 Draft、
  sequence，以及 Undo 恢复相同内容后的新 publication 均不能复用旧来源。
- 原图准备共享已捕获像素，无外部文件重读、PNG 编码或整图复制。保留缩放、透明度采样、
  元数据、输出/GPU 限制及取消检查。scene digest 升至 v4，按语义资产描述与已验证像素
  计算，不纳入随机 ResourceKey；相同像素的不同资源版本摘要相同。
- 迁移观察行为测试和公开动画渲染示例；新增来源竞争、Undo 旧来源拒绝、资源版本保活及
  内容摘要稳定性测试。Metal 捕获测试验证关闭 Session/删除全部源文件后仍渲染。

本批验证：Observe 默认构建 38 项单元、11 项捕获/Metal 集成测试通过；CPU-only 22 项通过。
公开动画渲染示例编译通过；Animation 6 项、SDK 66 项、ResourceStore 8 项回归通过；
所改 crate 格式、`git diff --check` 与 package-domain 检查通过。
本批未验证完整 Runtime/Editor/Python 调用链、WGPU 或 Windows。

## 第二批 Runtime 常驻核心与异步保存

- Runtime 根模块直接编译新宿主核心；SessionEntry 始终持有 AuthoringSession，派发不再
  take/归还 Session 或保留恢复副本。Work/Completion 只持有来源、Draft/快照、准备结果、
  取消信号和任务资源责任。WorkerPool 使用该协议，拒绝派发时完整归还未执行 Work。
- 支持多个隔离 Draft；独占 worker 只阻塞自己的 Draft。普通/忽略的命令错误及 panic
  结束候选；所有权、busy 和任务额度准入失败保留候选。prepare/publish 消耗尝试。
  宿主检查 Job、client、Session/generation/revision、Draft/sequence 后安装结果。
- Undo/Redo 直接恢复 SDK 历史；迟到 Worker 不能覆盖新版本或 Undo 后相同 ContentId。
  只读任务保持冻结 QuerySource；Worker panic 不丢失正式 Session 或历史。通知采用有界
  publication 队列，订阅同时捕获内容与 cursor，间隙要求重新同步，不互相消费事件。
- 保留 client/session/draft/job 数量、并发、任务工作区和只读结果限额，不加入文档/history
  倍率准入。取消与客户端释放不提前释放进入执行的任务/租约；ReadResult 外部副本继续
  持有资源责任。误投的 Completion 返回给宿主，避免丢弃已发布保存结果。
- SDK 新增 PreparedSave/CompletedSave 及保存许可；Worker 不持有 Session，保存期间仍
  可读但拒绝发布、Undo/Redo、替换和其他保存。Runtime 同时阻止 Draft 写入，保留原 Draft。
  文件发布成功后迟到取消/断开仍安装 ProjectContext/SavedMarker；失败或未执行任务释放
  许可；成功完成结果遗失则保持阻塞，不假装文件发布已撤销。
- 原生 open/save 走上述协议；保留规范化路径占用/预留、指纹冲突检查及 dirty 语义。
  打开结果检查最初来源再替换代次/清空历史；保存不与未完成的修改任务并发。

本批验证：Runtime 核心 17 项单元测试通过，含真实 WorkerPool、临时原生工程读写、
取消/panic、隔离候选、过期来源、通知间隙、任务/结果保活、误投完成与断开后保存。
SDK authoring 集成 31 项通过，其中新增 3 项保存许可测试；SDK 单元回归 66 项通过。
Runtime `--all-features --lib` 编译通过；所改 Rust 入口格式、`git diff --check` 与
package-domain 检查通过。all-features 仅为编译检查，不代表 WGPU 运行或 Windows 验收。

迁移边界：旧 Runtime domain/host/local/protocol 模块及集成目标保留为迁移输入，未通过
旧 API facade 接入；原有调用恢复、wire major/schema、领域导入/导出/观察任务、分步只读
调度、Editor 和 Python Local/Connected/生成契约仍需迁移。D9 已从编译执行路径移除，
D10 与旧 Runtime 部分 D12 源码待后续清理；不宣称全 Runtime/Editor/Python gates 通过。

## 第三批 v4 契约、Python 与协作调度

- 共享 Contract 定义带 `DraftSource` 的命令、预览与发布，以及带 `QuerySource` 的
  对象/集合查询；稳定 `ObjectKind + ID` 取代新路径上的对象/字段注册句柄。
- wire major 改为 `zipzi-runtime-v4`。私有端点沿用原传输权限检查；恢复使用原请求 ID
  和参数摘要，过期结果禁止重放。释放客户端后仍可恢复最后回执，但不允许新操作。
- Runtime 查询只序列化选定的记录/分页并限制结果大小。Local PyO3 接收 owned 值，
  在复制前计量并申请实际输入额度，不使用 JSON 字符串往返调用。
- Python `Session`、`Draft`、`Job`、不可变值和两种 backend 已切到 v4；异步命令更新
  精确候选序号，超时不取消任务，回执丢失的恢复保留原调用的对象投影和生命周期。
- `PreparedPreview` 在独立 Builder 中准备，成功后核对来源再安装；预览失败保留
  有效候选，普通 batch 仍失败即中止。读取任务支持分片让出，快照与租约随续体保留，
  取消/断连后的大对象在 Worker 退出时释放。
- 官方 `generate.py` 与 Contract exporter 切到 v4；生成 manifest 退役旧代理产物，
  新值由编译得到的 shape 生成。生成覆盖漂移、文件保护及正反类型检查。

本批 focused 验证：Contract 全 crate 测试、Runtime 28 项 lib 测试、3 项 owner/worker
集成测试、SDK 原 66 项 lib 测试，以及加入预览后的 43 项 authoring 过滤测试通过。
Python Local/Connected 的 10 项端到端测试通过，覆盖创建/竞争候选、预览拒绝、异步修改、
带图片保存/重开，以及创建/提交/关闭回执丢失恢复。新 Python 入口与生成值严格类型检查
通过；7 个错误用法得到预期诊断，生成与类型检查共 9 项通过。

尚未完成：Editor 当前仍引用旧 EditSession、领域 Plan、字段地址及旧发布分类，
`zipzi-editor-core` 整体尚未编译通过；生产 Editor host 未验收。Runtime 的二进制 bulk
传输、格式转换/观察/空间/动画任务接线，以及完整 Python gate/旧示例迁移仍未完成。
该批次以中间提交 `28eedeb0` 保存，随后继续 Editor 与其余接线。

## 第四批 Editor 宿主与资源读取

- Runtime 增加原生宿主的初始 Session 接管、空闲 Session 转移、定向派发、
  状态通知戳和即时保存派发；转移保留原身份、历史与保存点。即时保存先检查并发
  名额，拒绝时不留下排队任务、路径预留或保存许可。
- Editor 的 HostedDraft 使用 Runtime batch/preview/publish；手势协调保存在 Editor，
  不阻止其他客户端发布。ModelRead 持有不可变 Snapshot，读取不再长期持有 Runtime 锁。
  busy 拒绝提交时保留待提交的手势候选；外部发布使旧来源失效。
- ResourceView 按 AssetRecord 的资源版本直接读取已捕获纹理。Editor 纹理缓存改用
  Snapshot/ResourceView，处理资源替换和重排；画布缓存同时捕获完整来源和内容，
  跨来源复用前检查结构，保留相等的几何与索引共享。

本批已通过 Runtime 32 项 lib 测试、Project 9 项资源测试、SDK 的纹理快照/Undo/Redo
专项测试，以及 workspace domain 检查。Editor 的宿主、画布和授权读取测试已迁移或补充，
但仍受整个 crate 的旧领域 Plan、地址、发布分类与任务接口阻塞，尚未运行通过。
本批不代表 Editor 整体编译或第四阶段验收通过；剩余任务沿用上一批清单。

## 收尾与验收

- Editor 的领域确认、换父级、普通绑定、删除、Palette 多步操作和网格生成直接准备
  Runtime Draft；确认发布同一个候选，克隆对话框共享其一次性生命周期，不重放命令。
  字段捕获只保存稳定 ID、期望值和完整 Version，旧字段注册表/跨版本代理退出产品路径。
- Core 提供不可变能力/集合/检查查询与资源元数据编辑；SDK 补齐受锁保护的记录替换。
  删除审查在私有 Builder 中完成转换，转换阶段新增阻塞时保留完整报告并丢弃部分迁移。
- Draft 同一 sequence 的读取、PreparedPreview 和正式发布使用同一不可变快照及 ContentId；
  修复确认视图和发布内容身份不一致的问题。宿主写锁串行化后台完成与 UI 读取；Runtime
  排队的 Work 用独占锁持有 Send 闭包，使共享宿主不要求 Worker 回调实现 Sync。
- Runtime 接通实际资源捕获的格式导入/导出、空间与动画采样、GPU 观察及 Editor context。
  采样按姿态让出，结果保留原始来源；导出预留输出路径，文件发布后的取消不伪报撤销。
  发布事件记录实际 client 来源，宿主目录暴露真实 dirty/saving 状态。
- v4 二进制上传/读取使用相同认证与 request ID 恢复，校验输入摘要；缓存读取只保留请求的
  分片和实际租约。封存 RGBA 在 Draft 批次中替换图片，Job 输出通过 Blob 读取，不塞入
  控制帧。Python 两端共享生成 Operation、分片生命周期及恢复后的原始结果投影。
- 桌面应用与矩阵渲染工具直接使用新接口；更新 Runtime、SDK、EditorCore、Python 文档，
  记录明确的原生回归目标和生产 Editor 宿主验证入口。

本批最终验证（macOS）：

| Gate | 结果 |
| --- | --- |
| `cargo check --locked --workspace --lib --bins` | 全部工作区产品库/可执行入口通过 |
| `cargo build --locked -p zipzi-editor --bin zipzi-editor` | 生产桌面宿主构建通过 |
| Runtime `--features gpu-observation --lib` | 42 项通过；含实际 Metal PNG、来源保持、二进制恢复、导入/导出和保存 |
| SDK `--lib` | 72 项通过；含候选身份、阻塞删除审查与空间算法 |
| EditorCore `--test authoring_kernel` | 6 项通过；含确认一次发布、Undo 后过期来源、保存忙时重试、删除取消、图片保存及并发读写 |
| Python `tests/test_kernel.py` | Local/Connected 共 19 项通过，含故意丢失二进制回执后的恢复 |
| 同一 Python `ConnectedLifecycle`，`ZIPZI_KERNEL_EDITOR_HOST=.../target/debug/zipzi-editor` | 生产宿主 7 项通过，含异步编辑、保存/重开、图片 bulk、空间/动画、观察和格式导入/导出 |
| Python codegen/类型检查 | 9 项通过 |
| Core linked cancellation | 1 项通过；同批前序 Core state 28 项通过 |
| Contract 全 crate、变更文件格式、`git diff --check`、package-domain | 通过 |

范围说明：阶段四的产品调用链已切换；没有以旧协议 facade 保留旧实现。
旧 Editor 聚合 lib/integration tests 仍引用删除的 API，本批试编译未通过；旧 Runtime/SDK
集成目标、旧 Python 聚合用例、示例和未编译旧源统一在第五阶段迁移或删除。这些不计入上表，
也不宣称 full workspace tests / 完整桌面交互验收通过。Windows 与 WGPU 运行验收未执行；
Runtime all-features 编译检查通过不等于其运行验收。两项预先存在的应用图标删除不纳入阶段提交。
