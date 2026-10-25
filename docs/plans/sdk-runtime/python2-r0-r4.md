# 新 Python 包接回 Runtime R0–R4

最新收尾见 [R4 实现与验收](python2-r4-complete.md)：主体连接、安装和 UI 性能已验证；最后 Box 布局修复后的 Release P0/最终发行复验仍待完成，R4 未标完成。以下保留此前切片的历史范围。

2026-10-25 最新：[P3 场景/身份切片](python2-p3-scene-identity.md)接通选定场景/组织、
网格 BlendShape/带绑定拓扑、正式 SDK Ref 与场景标签地址；行为、安装和原生 Editor
证据独立记录。R4 仍在实施，完整退出项见该记录末尾与逐能力清单。

下一阶段按独立的 [Python P3A 重构方案](../python-sdk-rewrite/p3a-api-agent-contract.md)先统一公开契约、恢复、生成/手写边界和 agent 文档/验收，再继续 P3B/P4 的剩余公共能力。P3A 尚未实施，本记录保留原接口及原验收事实，不作为新目标接口的使用指南。

日期：2026-10-25。基线：`1e0043f`（网格生成器），本记录对应其后的工作区。
宿主/会话切片现已保存于 `acc54907`；其后工作区的
[事件、参数与批量查询接回](python2-r3-events.md)已通过最小链路验收。以下首批验证
保留其原始范围，R4 尚未验收。其后[网格绑定与 CPU 采样](python2-r4-bindings.md)已接回选定数值链路，
并将跨领域 Modeling 调度移到 Runtime；完整领域与预算仍待推进。
执行主线为 [Runtime 实施计划](../sdk-runtime-implementation-plan.md) R0–R4。
`zipzi-python2` 是本轮唯一 Python 实现；旧 R0–R4 记录保留为历史，不读取、搬回或
继续扩展旧 Python 包。共享 Rust SDK/Runtime/Editor 能力与独立领域测试可复用。

目标是让 Agent 与人在同一个未保存工程和同一套历史上交替制作。Local 经直接
PyO3 调用 LocalHost；Connected 进入 Editor 应用级 Runtime。窗口、脚本和会话
具有各自生命周期，保存文件不承担人机交接同步。

## 覆盖与缺口

以下是接回顺序和逐能力差距清单，尚不是完整 SDK 方法级 R0 清单或 R4 验收。
旧客户端通过的案例不直接换名计入新包。机器可读状态见
[python2-coverage.json](../../runtime/python2-coverage.json)。

| 阶段/能力 | 可复用的共享层 | 新包已有证据 | 剩余工作 |
| --- | --- | --- | --- |
| R0 清单、预算、基线 | Runtime 预算/路径边界；SDK 独立测试；R2 原交互基线 | 新包 P0 试点冻结比较，当前差距清单与产物身份 | 扩至 R4 方法/意图/结果级清单；新增领域、大负载与真实宿主测量 |
| R1 会话、候选、Job、历史 | 单一正式 Session/历史、路径预留、完成消费、事件缓冲 | P1 生命周期故障矩阵；本批 Local/Connected 会话列表、身份及绑定 | 新领域继续补 Job/来源/预算验收；公共完整故障与事件视图 |
| R2 Editor 所有权与投影 | 应用级托管、窗口 attach/detach、统一发布 | 安装新包外部改名→图层/属性更新→UI Undo→客户端读取；跨会话 Save As 竞争；Metal 渲染 | 全领域投影/选择/关系/纹理/表单；本批未重跑完整窗口/IO 生命周期及交互性能矩阵 |
| R3 发现/启动/绑定 | 私有 endpoint、继承 flock、严格协议/构建握手、桌面重开 | 安装纯包并发冷启动；慢启动不重复拉起；失败重试；残留 socket；协议/build 拒绝；真实桌面重开复用 | 发行安装位置验收；更多真实宿主生命周期扩展 |
| R3 事件与参数最小闭环 | 注册表/会话订阅、事件 cursor/resync、SDK 参数编辑 | 新包类型化事件、参数及网格批量读取、安装故障与真实 Editor 同历史 recipe 已通过 | 全领域增量事件、集合与大型读取随 R4 扩展；详见新记录 |
| R4 场景/组织/拓扑/绑定/BlendShape | 首批 Modeling 处理器及 SDK 规则 | 新包已有选定网格/图片/参数/网格绑定关键形与改名 Plan | 逐域类型化值、操作、查询与同 recipe；大网格、有绑定拓扑、限额负例 |
| R4 身份/地址/能力 | SDK Handle/incarnation/epoch；Runtime scoped token | 本批仅公开运行期 SessionId 和握手能力 | 对象 Handle、稳定字段/内容地址、删除/移动/Undo/generation 失效 |
| R4 Plan 与 CPU 求值 | 来源绑定 Plan、正式/候选 CPU 采样与诊断 | 选定改名 Plan；正式/候选网格绑定 CPU 参数 sweep 与人工预览隔离已验证 | 完整 Plan 报告、成本/损失与释放；scene-binding/BlendShape 数值 recipe 和大负载 |
| R4 资源与工程 IO | 图片实际负载计费与回收、首批 blob、路径预留和 IO Job | 新包 1 MiB 图片/PreparedImage、替换、历史、保存重开 | 真正二进制 blob；文件图片/附件、磁盘资源固定、导入/替换/generation/各格式导出；准备与编码峰值准入、取消 |
| R4 调度与总预算 | 共享资源池、候选/Plan/Job 子预算 | P1 scoped heap/copy 探针与选定释放断言 | 昂贵准备/Plan/采样后台 Job、队列与结果总预算、公平调度及资源/RSS 报告 |

本批保持 R0–R3 原共享层交付事实；其后 **新包 R3 最小链路已贯通，R4 未验收**。建模 CPU
采样在 R4 内；Python 自身 P5 的原排序不能成为延后它的理由。二进制 blob 与
Modeling 调度定义从 SDK 总枚举移到适配边界的既定调整继续列入 R4。
动画/独立 Preview/Observer、Review 分别按 Runtime R5/R6 后续推进。

## 本批已实现：宿主与驻留会话

```python
from pathlib import Path
import zipzi

with zipzi.connect(start=True, host=Path("/absolute/path/to/zipzi-editor")) as runtime:
    metadata = runtime.connection  # 不含恢复 secret 的不可变握手记录。
    for info in runtime.sessions.list():
        session = runtime.sessions.attach(info.id)
        session.show_window()
```

省略 endpoint 时使用与 Rust 相同的 `ZIPZI_RUNTIME_DIR` / `ZIPZI_RUNTIME_PROFILE`，
默认 `/tmp/zipzi-runtime-<uid>/default.sock`。`start=False` 为默认值。
`start=True` 在取得该 endpoint 的私有 flock 后传递已锁 fd，使用 `--no-window`
启动宿主；先尝试已有宿主，不在协议/build 不匹配或握手超时后另起实例。
启动失败允许后续重试，慢启动超时保留存活宿主。恢复已有请求永远不触发启动。
客户端不删除 socket/lock；只有持有唯一所有权的 Rust 宿主清理残留 socket。

可执行文件按显式 `host`、`ZIPZI_EDITOR_HOST`、macOS 用户/系统 Applications 中
`Zipzi Editor.app` 的顺序选择；本批实际验证仓库编译的生产可执行文件，尚未验证
发行安装位置。目录/锁/socket 的所有者、权限和 symlink 检查与 Rust 边界一致。
`timeout` 控制请求，`startup_timeout` 限制启动等待；可通过 `sdk_build` 显式固定
SDK 来源身份，不宣称客户端自动携带或验证尚未完成的发行构建配对。

`SessionId(runtime, serial)` 是纯 Python 不可变值，不以文档 UUID/路径代替；
`session.id` 不发查询。`sessions.list()` 返回不可变 SessionInfo，包含路径、状态、
版本与历史；`sessions.attach(id)` 只绑定现有正式会话。两者丢响应仍由原异常
恢复同一请求。Local 使用直接结构转换，未引入 JSON 路径。
`show_window()` 由桌面宿主执行；无窗口能力的 CPU/Local 宿主明确报不支持。
原生契约更新为 `zipzi-python2-4`，IPC 契约继续是 `zipzi-runtime-r4-1`。

## 验证

可复核的命令、产物哈希、环境与未运行项见
[本批结果](python2-r3-results.json)。

- 原有 36 项源码行为和生命周期用例通过；新增 1 项 Local/Connected 身份/绑定用例通过；严格正例与三条精确反类型用例通过。
- 安装纯 wheel 的 11 项连接验收通过；使用真实进程/socket/flock，覆盖并发冷启动、慢启动、失败重试、权限/symlink、残留 socket、错误 build/protocol、握手超时，以及 list/attach 响应恢复。
- 当前生产 Editor 可执行文件验收通过：两个安装后 Python 进程同时冷启动，得到同一 PID/Runtime，绑定同一未保存 Session，累积两个历史步骤；显示窗口、桌面重开复用、保存后重启并从磁盘打开内容一致。
- 原生 `runtime_ipc_acceptance` 已改用安装后的新纯包并通过：真实外部写入、图层/属性投影、UI Undo、新客户端读回、在途 Save As 竞争以及 Metal 渲染。Editor acceptance/full 默认准备新包的隔离解释器。
- Editor 测试入口工具回归 9 项通过。源码构建和生成漂移检查通过。

正常与独立指标 Release 产物分别重建，通过会话绑定及本地无 JSON 的聚焦用例。
[本批 P0 比较](python2-p0-check.json)沿用原冻结工作负载/阈值，每后端 5 个计时与
5 个探针进程；`comparison` 与 `noise_rejections` 均为空。范围仍为六项 CPU
fixture 工作负载与已声明的复制/请求堆指标，不以此声称启动耗时或完整 R4 达标。
正常原生 wheel 也重新构建，与纯 wheel 安装后运行图片/Plan/历史/保存重开 recipe。

```sh
./tools/test-sdk.sh python2
./tools/test-sdk.sh python2-connected
cargo build --locked -p zipzi-editor
./tools/test-sdk.sh python2-connected --editor-host "$PWD/target/debug/zipzi-editor"

# 聚焦的真实 Editor + 安装新客户端 + Metal 验收，不跑昂贵全套。
ZIPZI_CLIENT_PYTHON=$(python3 crates/zipzi-python2/tools/connected.py --install-only) \
  env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor \
  --features test-support --test runtime_ipc_acceptance
```

本批未运行 SDK/Editor full、完整 R2 性能配对、完整 R4 领域或发行 app 验收。
现有 P0 CPU fixture 基线不冒充真实 Editor 延迟或 R4 性能完成。
