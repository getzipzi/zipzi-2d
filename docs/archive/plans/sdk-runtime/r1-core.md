# R1：Runtime 进程内核心与 SDK 契约

日期：2026-10-24。状态：R1 进程内/CPU 部分已交付；R2–R7 未实现。
本阶段使用合仓后的单一工作区，起始 revision 为
`4cdd1738c79eee10d8c565516442d1b4bd45a48b`，开始时工作区干净。
本记录对应本阶段源码改动及根 `Cargo.lock`，不把起始 revision 表述为包含 R1 的提交。

## 实际交付

新增 MIT `crates/zipzi-runtime`，加入根 workspace 和 SDK 测试域；现在为
16 个 SDK package、3 个 Editor package。锁文件仅增加 Runtime package 和
Editor core 对 Runtime 的依赖，没有升级第三方版本。基础 Runtime 无 GPUI、
PyO3、GPU 或 transport 依赖，SDK 不反向依赖它。

Runtime 以 `&mut Runtime` 串行协调，持有唯一正式 `AuthoringSession`；不可变
`session`/`document` 借用直接读该实例。RuntimeInstanceId 隔离宿主，Session、
Client、Edit、Job 和 Operation 使用不复用的运行期身份。Edit/Job 表检查客户端
归属；detach/客户端释放不关闭正式会话。显式 close 检查其他使用者、候选、任务
和脏状态，发布 Closing/Closed 管理状态后释放。未完成 Open 只占用内部槽位与
预留，在成功登记前不出现在注册表订阅快照中。

事务托管直接使用 SDK owned candidate；事务内读取、候选快照、完整版本提交、
Undo/Redo 和唯一历史均由 SDK 执行。普通错误及被调用方捕获的 SDK 普通错误仍使
整个候选失败；候选超预算或 host apply closure panic 不会发布部分修改。Busy/
准入预算拒绝保留候选。最终 commit 始终调用 SDK `commit_to`，包括版本或 SDK
history budget 拒绝；显式诊断快照须在消费 Edit 前捕获。R1 没有第二套领域验证。

SDK 新增 `candidate_snapshot`、`EditSession::is_aborted/estimated_bytes` 和
`EditReceipt::estimated_bytes`。候选快照保留基准 Version 和已引用内存图像，拒绝
不同 session/generation；Runtime 补 EditId/候选序号，正式来源则带具体 Version。
快照读不改变正式历史或 preview。磁盘资源字节固定与底层资源回收留到 R4。

工程身份通过 `zipzi_project::project_identity` 复用项目层已有的原生路径解析原语。
Editor 原路径 helper 也转用该入口，保持目录/manifest、符号链接及尚未存在目标的
等价身份规则。Runtime 在派发前统一预留 Open/Save As 目标；Save As 成功前保留
旧身份，失败保留旧身份。取消仅标记请求，确认终态前不释放预留。同一 Runtime
的所有入口经过此表；不宣称协调外部进程或尚未在 R2 迁移的旧 Editor 注册表。

作业接口为 `Executor::submit(Work)`、worker `Work::run()` 和宿主
`Runtime::complete(Completion)`。作业携带正式会话所有权及 runtime/session/job/
base 身份，不借用视图。拒绝提交返还原 Work；接受后执行器必须投递 Completion，
不得在 submit 中 panic。Runtime 不安装桌面线程池。重型 Commit/Undo/Redo、
Open/Save 可走同一接口；短路径保留类型化同步调用，R2 再按交互测量接入。

普通 SDK 错误归还原会话与历史并恢复 Ready。`Work::run` 捕获 worker unwind，
以失败 Job 投递，正式会话进入 Faulted，保留带版本的冻结来源并拒绝写入/保存。
故障原因同时保留于 SessionInfo，不依赖过期客户端的 Job 结果。进程 abort/崩溃
仍不在内存恢复保证内。Completion 不可克隆/重定向，重复、错会话或晚到结果不
复活文档。取消发生在执行前返回 Cancelled；SDK 已开始执行则返回实际成功/失败，
不会把已保存/已提交的结果报作回滚。

Runtime 是托管 SDK receipt 的唯一 drain。订阅原子获取快照/游标，事件携带
ClientId/OperationId，后台事件还带 JobId；正式回执保留 SDK 前后 Version，保存
成功单独发事件。每 Session 与注册表流独立有序；数量/字节超限、单条过大、外部
宿主或未来游标明确返回 `ResyncRequired`。事件回调不在可变 SDK 借用内执行。

`zipzi-editor-core::HostedDocument` 是 R1 最小类型桥接，仅持 SessionId。
测试检查其读取与 Runtime 中原 SDK 实例指针相同，并观察同一提交/Undo 历史。
现有 Editor Document/Workspace、发布和窗口生命周期主迁移属于 R2，未在本阶段
提前声称完成。

## 预算边界

`RuntimeLimits` 使用 R0 已启用对象的默认值：8 客户端、32 会话；每客户端/会话
1 候选、每会话 4、宿主 16、候选估计总计 512 MiB；冻结文档估计 256 MiB、历史
总估计 512 MiB、可计量文档 CPU 总估计 2 GiB；Job 槽位 32/每客户端 8、同时执行
2；Session 事件 512 条/8 MiB、注册表 256 条/1 MiB。

候选估计计入文档、Edit 容器、身份集合、标签、控制对象与对象 ID；共享文档/身份
负载按 holder 保守重复计费。移交 worker 的候选仍计费，且在途正式提交预留历史/
publication 增长，取消请求不提前退还。冻结来源与事件也纳入文档 CPU 估计；事件
受独立上限及总预算淘汰，淘汰后游标要求重同步。SDK 每会话自己的 history limits
始终继续执行。

R1 为完成结果保留有界 Job 槽位，显式 `release_job` 释放；客户端显式过期清理其
终态结果，已提交作业仍执行并归还会话。R3 再实现协议请求结果的字节/时间窗口、
去重水位和重连宽限。R4 的底层 RGBA/PNG 真实负载计费/回收、磁盘资源固定，以及
Plan/Preview/Proposal/blob 预算尚未启用。此处的文档估计不能替代真实 heap/RSS
测量，也不计作 A17 通过；类型化调用方持有的按值/Arc 快照不是远端 token 表。

## 验收与实际验证

| 编号 | R1 的进程内/CPU 证据 |
| --- | --- |
| A02 | 目录/manifest/符号链接复用；相同模型 UUID 的副本独立；并发 Save As 与 Open 预留冲突；被拒绝者不写文件；取消终态前仍占用；加载/保存失败后重试；保存失败保留旧身份，成功才切换 |
| A03 | 两客户端交替提交共用一套 SDK 历史；no-op、取消、失败保持 redo；Undo/Redo 内容对照 |
| A04 | candidate 读到前序写入、正式内容不提前变化；捕获 SDK 错误也无法提交；SDK 最终 history budget 拒绝没有部分发布；显式候选诊断快照保留 |
| A05 | 改名保持句柄；删除/Undo 不复活旧句柄；完整 Version 冲突、旧 Plan 与旧字段地址沿用 SDK 错误 |
| A08 | 可控 CPU executor；真实线程/文件系统 panic；Ready/Busy/Faulted、冻结来源、普通保存失败归还、取消、排队拒绝、并发上限；错投/重复/晚到结果拒绝；发起客户端/视图 detach 后完成照常入宿主 |
| A10 | 快照/游标原子订阅、receipt 唯一消费、Operation 来源、独立保存事件；有界/超大事件断档显式 resync；无订阅者的同一写入/历史效果 |

从仓库根目录实际运行：

| 命令 | 结果 |
| --- | --- |
| `./tools/test-sdk.sh runtime` | 26 项：1 Runtime 内部迟到/重复完成测试 + 25 进程内行为测试，全部通过；不包含 UI/IPC |
| `cargo test --locked -p zipzi-sdk --test integration -- candidate_snapshot:: vertical_slice:: handles:: history_budget::` | 13 项通过；其中 2 项为候选快照的新契约测试 |
| `env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration -- runtime:: workspaces:: editor::` | 25 项通过；1 最小 Runtime 对接、8 工作区基线、16 Editor IO/路径/协调基线 |
| `python3 tools/test_tools.py -p 'test_runtime*.py'` | 4 项通过，包括实际 Cargo normal 依赖图的 CPU 边界和 SDK 无反向依赖检查 |
| `python3 tools/test_tools.py -p test_cargo_domain.py` | 5 项通过；19 个 workspace member 完整且互斥分域 |
| `cargo clippy --locked -p zipzi-runtime --all-targets --no-deps -- -D warnings` | Runtime 本身所有 target 通过 |
| 两份 Runtime 源清单 `--check`、所改 crate 的 `cargo fmt -- --check`、`git diff --check` | 通过 |

SDK 源清单更新为 958 条 API/记录，Editor 保守迁移索引为 77 文件/737 个引用。
API 的 `not-connected` 状态继续保留；新增 Runtime 与 CPU 测试不是 Python 连接
验收。Editor 索引仍记录 R2 待迁移入口，不将最小读取桥接视为完整迁移。

曾尝试对依赖也启用 `-D warnings` 的 Clippy，先在未修改的 `zipzi-core` 代码中被
21 项已有 lint 阻止（如 cloned-ref-to-slice、chunks-exact、type-complexity）。
未改动这些无关领域代码；上表明确记录 `--no-deps` 的聚焦检查范围。未运行昂贵
全 SDK/Editor/GPU 套件；R1 无 UI 行为变更，性能和窗口原生对比仍按计划在 R2 进行。
