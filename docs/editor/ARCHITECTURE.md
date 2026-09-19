# 进程内脚本建模架构

本文记录现有原型实现，不是下一阶段的目标架构。工程目标、重构范围与验收以 [工程路线图](ROADMAP.md) 及独立里程碑为准；现有接口允许重构。

2026-09-19：根据 Agent 自主建模、Agent 自检、人辅助检查的目标调整 Stage 04。

## 数据与操作分离

```text
进程内 GDScript ──直接访问──→ KasaneMeshData / Document ─→ 预览
       └──可选 Action ─────→ 同一数据接口
                └────────→ Godot UndoRedo
```

- `kasane-core::Document` 保存源数据和稳定 ID，不依赖 Godot。它不再维护 Undo/Redo 栈。普通写入直接生效，不要求事务，不自动记录或清空历史。
- `KasaneDocumentBridge` 管理当前 Document、纹理与派生预览，提供保存、重开和数据访问。它目前仍是 Node2D；Document 本身不是场景树。
- `KasaneMeshData` 是 RefCounted 对象句柄，绑定拥有者实例 ID、文档代次和 Mesh ID。属性 `name`、`positions` 可以直接写；`set_vertex_positions` 支持稳定 ID 批量更新，`replace_geometry` 支持替换顶点 ID、位置、UV 和拓扑。它不持有另一份权威数据。
- `KasaneDeformerData` 提供 Rotation 的中心/角度与 Warp 控制点属性。源数据经最近变形父节点到祖先逐级求值，只有求值输出送到渲染器。组织关系不参与求值，实现见 [deformers.cpp](../../modules/kasane-core/src/deformers.cpp)。
- `script_host.gd` 在当前 Godot 进程加载并执行 GDScript。脚本接收当前 `document` 和可选 `actions`；执行本身不隐式创建事务，也不自动回滚。
- `actions.gd` 提供显式的 `perform(label, callable)`，使用 Godot 核心 `UndoRedo` 存放调用。也可直接使用 `UndoRedo.add_do_property` 等原生能力，无须经过辅助类。

## 数据访问边界

所有操作在 Godot 主线程进行。Packed 数组读取是副本，批量修改后显式赋回；不是 C++ 裸指针或零复制视图。普通属性 setter 仍校验长度、索引和有限值，保证渲染安全；当前尚不支持让无效拓扑在 Document 内暂存。Agent 可以直接替换一整组有效几何，不必调用高层建模命令。

属性赋值失败由 Godot 错误日志报告；需要程序化判断时使用返回 Dictionary 的方法（如 `set_vertex_positions`、`replace_geometry`），检查 `ok/code/message`。不把任意 setter 错误冒充成可捕获的 Python 异常。

直接写入会推进 revision、更新预览和未保存状态，不生成历史。直接脚本返回失败或运行中断时，先前的直接写入保留。脚本是可信的同步应用代码，不是隔离沙箱；无限循环会阻塞主线程。

## 可选 Action、事务和原生 UndoRedo

`actions.perform` 捕获执行前后的源数据状态，只有回调显式返回 `{"ok": true}` 才登记一个原生 UndoRedo 步骤；返回失败或无成功结果则恢复执行前状态。恢复范围仅为 Document 和它引用的纹理，不包括文件、网络等外部副作用。回调不得更换文档；若更换，会返回 `DOCUMENT_REPLACED`，不把旧数据恢复到新会话。

首版 Action 是**整个文档的快照操作**，不是属性级合并。纹理共享引用，不复制 GPU 数据。撤销会恢复整个文档，因此可能覆盖该 Action 之后的直接写入；混用时应明确选择撤销或 `history.clear_history()`。这里复用的是 Godot 的历史栈、顺序、redo 分支与步数管理；源数据快照是本应用的状态载荷。也可直接用原生属性 Action 获得属性级撤销。

`Document.begin_transaction/stage_vertex_positions/commit_transaction/cancel_transaction` 保留为可选的原子位置批次工具，提交不自动生成 Undo。需要时在 Action 回调中调用。Godot UndoRedo 本身不保证任意多步方法失败后自动回滚，应用不能将其宣传为数据库事务。

快照只用于显式 Action，不围绕每次脚本执行自动创建。历史被清理时释放快照及纹理引用；Document 中没有第二套历史栈。

## 保存、重开与生命周期

Stage 05 将项目格式升级为 JSON v2，保存变形器和父链接；兼容读取 v1，仍安全替换保存。Bridge 根据当前持久化内容与最后成功保存内容比较 `modified`，因此 Action 撤销回保存内容后变为未修改。

同一 Bridge 的 revision 在重开、恢复时也保持递增。成功重开推进文档代次；旧网格句柄和旧快照均不能修改新文档。Action 服务下次调用时清理上一会话历史。拥有者销毁后的句柄也安全失效。

## Agent 自检

脚本可读取摘要、对象快照和实际预览数组。`script_host.capture(viewport)` 等待实际渲染帧，返回图像与 revision；等待期间数据改变则拒绝，headless 无渲染环境明确返回 `RENDERER_REQUIRED`。演示的 Run/F5 将图片写到 `artifacts/script-preview.png` 并输出对应 revision。

当前已实现执行、数据读取、预览与历史基础；自动化几何质量诊断、变形参数扫描、自主判断与迭代策略仍需后续阶段提供。Agent 可以生成和修订脚本，人不需要逐次批准数据修改。

## 范围

当前使用 GDScript；不据此断言 CPython 无法嵌入 Godot。以后可以给同一数据语义增加 Python 绑定，语言选择不再决定数据模型。旧 socket 服务和 Python RPC SDK 已移除，不再作为编辑入口或验收依据。未实现通用 RNA、脚本沙箱、完整求值依赖图或通用插件系统。
