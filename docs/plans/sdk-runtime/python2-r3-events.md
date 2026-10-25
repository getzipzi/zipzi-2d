# 新包 R3：事件与参数最小链路

日期：2026-10-25。基线 `acc54907` 已保存宿主/会话接回；本记录对应之后的工作区。
执行范围仍是 [Runtime R0–R4](../sdk-runtime-implementation-plan.md)。本批接回新包的
事件、参数和网格批量读取；R4 全领域及最终发行验收继续单列。

## 调用与共享边界

```python
from zipzi import events, ops, parameters

stream = session.subscribe()
with session.edit("Mesh and angle") as edit:
    angle = edit.parameters.create(parameters.ParameterSpec("Angle", -30., 30., 0.))
    edit.apply_many((ops.Rename(face, "Face"),
        ops.PatchParameter(angle, parameters.Patch(name="Angle X"))))
    candidate = edit.parameters.get_many((angle,))
    meshes = edit.meshes.get_many((face,))
for event in stream.poll():
    if isinstance(event, events.Published):
        assert event.receipt.changed
```

SDK 新增 `ParameterCreateSpec` / `ParameterPatch` 和对应方法；创建从 Rust 默认值
取得精度、kind、repeat 等未指定字段，身份由 SDK 分配。补丁在 SDK 内读取当前
候选并调用既有 `replace_parameter`，避免客户端先读再写覆盖 runtime ID、kind
或精度。范围、依赖、不变量及提交仍由 core/SDK 决定。Python 只校验文本、有限
f32、i32、bool 和枚举等可编码形状，不复制参数范围或精度领域规则。

`CreateParameter/PatchParameter` 加入同一个 `Edit.apply_many`，经
`Runtime::apply_authoring` 与网格/图片/Plan 组合。每次批次只推进一次候选序号，
最终提交只有一个历史步骤；失败、Busy、准入拒绝与结果未知沿用共用生命周期。
未增加独立参数事务或发布路径。

`session/edit.parameters.get/get_many/list` 返回带正式/候选来源的不可变快照。
`list` 使用 SDK 参数存储顺序；参数组/CDI 展示排序仍属 R4。
`meshes.get_many` 保持选择顺序及重复项，整批在同一宿主调用中读取。
两种批量查询都限制为 256 条、1 MiB 保守响应估算，在复制记录前核对整批与
剩余宿主预算。缺失记录或读取拒绝不毒化候选。大型/分页集合仍待 R4。

参数、批量查询及事件适配本批手写，复用已有生成的网格转换；网格生成器本身
保持不变并通过漂移检查。P2 的 bool/i32/枚举与资源/Plan 生成继续独立跟踪。

## 事件契约

- `session.subscribe()` 捕获 SessionInfo 与游标；`runtime.sessions.subscribe()` 捕获注册表信息与游标。初始信息与游标在同一 Runtime 读取中确定，不复制整份模型。完整投影须显式读取并核对 SourceRef。
- `initial` 是缓存的不可变信息，只有 `resync()` 显式更新；纯值 `cursor` 区分 Runtime、会话/注册表流及序号。不同流没有全局顺序。
- `poll(limit=128)` 返回只读事件页，最大 256 项；Runtime 在构造 Python 对象或 JSON 树前检查保守负载及临时展开预算。完整解码原响应后才推进游标。
- Published/Saved/StateChanged/Created/Closed 具有明确类型及操作、客户端、Job 来源。发布回执当前公开版本、标签、对象 ID 和 changed，完整 SDK ChangeSet 尚未成为 Python 公共类型。
- Runtime 仍是唯一 SDK 回执消费者；订阅者互不消费对方的事件。`runtime.events_since(cursor)` 可从另一连接接续。
- 断档、未来游标或旧 Runtime 明确要求 resync，不自动跳过。未决 poll 不能被 resync 消费；订阅创建丢响应恢复原对象，poll 丢响应恢复原批次，之后才读取后来的事件。

Runtime 新增无模型复制的 `subscribe_info` 与有界 `events_page`；两个 Python
后端使用同一入口。Editor 原有完整冻结投影的 `subscribe` 及普通 Rust 读取的
`events_since` 保留原职责。

IPC 保持 `zipzi-runtime-r4-1`，新增并检查 `typed_parameters`、`typed_event_pages`
和 `typed_mesh_batch` 能力；缺少能力的宿主直接拒绝，不自动替换运行中的实例。
原生契约为 `zipzi-python2-5`；本地继续经直接 PyO3/LocalHost，无 JSON 往返。

## 验收与范围

本页保留本批构建、测试与验收结论；前一批宿主/会话结果仍保留在对应 Markdown 记录中。

P0 独立比较沿用冻结脚本与原六项 CPU fixture
工作负载，计时/探针分开构建，两个后端各 5 个计时与 5 个探针进程。
`comparison`、`noise_rejections` 均为空；Local 的 JSON 编解码/Value 构造仍为零。
新参数/事件专项负载、真实 Editor 延迟及完整 R4 预算仍须独立扩展，不将它们填零。

- 源码 45 项行为/故障用例通过，覆盖两后端混合参数批次、字段保留、读取预算、捕获错误/no-op/redo、事件分页/resync与独立读取、各新增入口丢响应恢复。
- 严格类型正例通过；6 条精确反例为零基行号 5/6/7/8/9/12，新增参数补丁/批量 ID 和事件游标反例。
- SDK 参数 2 项、Runtime authoring 5 项、protocol 10 项、原事件 a10 2 项聚焦测试通过；生成器 6 项继续通过。
- 安装纯 wheel 的连接测试为 13 项；公共 Edit/with 注入批次、commit、Job、订阅/poll/参数读取丢响应，以及 Busy、捕获领域错误和丢清理响应，验证不重复发布或推进来源。
- 生产桌面双 Python 冷启动加入共享参数写入及事件；强制终止带未提交修改的客户端，等待真实宽限期，通过四个编辑槽位重新可用证明回收，正式内容/历史不变。保存重启并重开验证参数字段，旧会话游标明确返回 `RESYNC_REQUIRED`。
- 原生 Editor + 安装新包 + Metal：网格/参数在同一历史步骤发布，图层/属性/参数投影更新；UI Undo 后另一进程从旧游标读到两次连续发布并确认参数撤销；路径预留与渲染通过。
- 正常纯/原生 wheel 的安装 recipe 扩展参数、批量查询、事件、纹理/Plan、Undo/Redo 和保存重开；源码 extension 与正常 wheel 分开验收。

```sh
./tools/test-sdk.sh python2
./tools/test-sdk.sh python2-connected --editor-host "$PWD/target/debug/zipzi-editor"
cargo test --locked -p zipzi-sdk --test integration parameter_editing::
cargo test --locked -p zipzi-runtime --test authoring --test protocol
cargo test --locked -p zipzi-runtime --test runtime a10_
```

新包 R3 公开最小链路已贯通，后续进入 R4 场景/绑定/拓扑、地址/句柄、CPU 求值和
完整资源/工程 IO。完整方法级 R0 清单、R4 总预算/大负载、发行 app 位置/平台矩阵
与完整 R2 性能配对仍未验收。现有六项 P0 CPU fixture 也不代表新增参数/事件负载
和真实 Editor 的完整性能验收。
