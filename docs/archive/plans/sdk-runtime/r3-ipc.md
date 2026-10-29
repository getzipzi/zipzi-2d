# R3：本机 IPC 与 Python 最小闭环

日期：2026-10-24。状态：**R3 最小切片已交付**。R4–R7 尚未开始。
验收结果见本页，逐入口覆盖见
[r3-coverage.json](../runtime/r3-coverage.json)。本阶段不声明完整 modeling、
animation、Preview、资源或 Proposal 覆盖。

## 配对与实现

基线为同仓 `b3487dabe861a47d9a586203d9f4ff002b65beb4`；开始时工作区干净，
交付改动尚未提交。Editor、Runtime、SDK、Python 共用本次工作区和根 Cargo.lock。
依赖增加限定为 Runtime 的 serde/serde_json、sha2、async-channel、Unix libc，
Editor 显式依赖 Runtime；SDK 没有反向依赖。基础目标仍无 GUI、PyO3、GPU 依赖。

协议契约为 `zipzi-runtime-r3-2`。控制帧使用 big-endian u32 字节长度和 UTF-8 JSON，
严格握手先于客户端登记/写入，返回 RuntimeInstanceId、能力、预算和 SDK 来源摘要。
SDK build 是 SDK/Core/Project/动画及相关格式 crate 源码、manifest 与根锁文件的
确定性 SHA-256 身份，可在没有 Git 的源码分发中计算；它不是二进制签名。
SDK 只补公共版本验证及批量读取适配；领域规则和提交校验仍由 SDK 执行。

请求按 ClientId + 单调 RequestId 去重，保存规范化输入摘要和原结果。
相同 ID 不同参数拒绝；被淘汰的旧 ID 由水位拒绝，不重新执行。
`request_status` 不推进水位，因而查询未执行 ID 不会阻止原 ID 恢复。
提交、Undo/Redo、保存和打开继续走 Runtime Job；终态查询原子地将 Job 结果交给
去重窗口并释放 Job 槽位，终态响应丢失仍可用同一查询 ID 恢复。

本机目录/profile、flock 唯一所有权、Python 启动时传递已锁 fd、驻留持锁、残留
socket 清理及桌面重开复用均已接入。超时不清理存活所有者，也不启动第二个宿主。
监听前启动失败释放所有权，后续调用可以重试。开发 PID 记录只表示进程归属；
另一个 readiness 文件只有真实严格握手完成后才发布。转发给现有宿主时报告复用，
不会把它写成当前开发启动器拥有的进程。

socket 线程只处理编解码与有界消息通道，所有 Runtime admission/publication 在
Editor 应用线程执行；昂贵正式操作移交 worker，完成先归还 Runtime 再通知视图。
无窗口宿主可通过显式 SessionId 显示已有会话。

Python 公共 facade 可在没有 `_native` 的环境导入；原生 wheel 保留原有类/异常身份。
同一 facade 还可打包成不含扩展的 `zipzi-0.1.0-py3-none-any.whl`。
两个 variant 使用同一 distribution 名，按环境选择安装。已验证 CPython 3.14.7，
支持范围仍为 `>=3.14,<3.15`。

公共 Edit 现在区分 editing/aborted/outcome_unknown/committed/cancelled。
普通领域或本地转换失败禁止后续提交；明确 Busy 保留候选并使用新 ID 重试。
准入预算被明确拒绝且 Runtime 保留候选时也不丢 Edit。
丢响应/断线保留原请求和 Job，阻止后续写入，`recover()` 只恢复原 ID。
上下文退出不会盲目取消未知提交；清理失败不替换原异常，也不伪造已确认释放。

## 预算与范围

保持 R0 已启用预算：16 MiB 控制帧、每客户端 1024 条或 8 MiB 去重结果、宿主
64 MiB 结果、300 秒保留窗口、30 秒断连宽限。结果缓存以序列化字节加条目
开销计量，不将其宣称为 allocator/RSS 字节。连接串行，每个连接至多一个在途
请求，最多八个 socket worker，宿主消息通道 16 条；这是比 R0 最大队列更严格的
首期准入策略，输入背压不会无限积压。Job/候选/事件沿用 Runtime 的有界表。

大数组/blob 通道留在 R4，此阶段握手不宣称其可用。资源像素、PNG 缓存和准备中
负载的总计费及回收仍是 R4；这里的 CPU 字节为文档估算，不是 RSS/GPU 硬限额。
未实现领域明确返回 `UNSUPPORTED_OPERATION`。独立 Preview/Observer 在 R5。

## 验收证据

| 验收 | 已观察到的结果 |
| --- | --- |
| A02/A03 | 同一路径 open 复用同一个驻留 Session；两个客户端共享历史；取消/no-op 保留 redo；原生 UI 持有 Save As 目标时真实 Python 客户端被拒绝且目标未被写入 |
| A04/A12 | 公共 Edit 读到前序 Mesh/Parameter 修改；捕获普通错误后仍不能提交；本地非有限值转换失败也使候选失败；清理断线不替换原异常 |
| A06 | 两个真实 Python 子进程同时冷启动，得到同一 PID/RuntimeInstanceId，绑定同一未保存 Session；覆盖协议/build 不匹配、宿主重启、慢启动超时、残留 socket、监听前失败重试、桌面重开复用 |
| A07 | 安装后的公共 Edit/with 经真实 socket 注入 apply 成功丢响应、commit 成功丢响应、Job 终态丢响应；恢复后恰好一个历史步骤；Busy 离开 with 也保留候选；不同参数重用 ID、结果过期和原 ID 状态查询分别测试 |
| A10 | 真实 Python 修改原生 Editor 未保存合成文档，图层及属性投影前进、历史只增加一步；UI Undo 可被另一个 Python 进程读到；之后原生 Metal 渲染成功 |
| A12 | 隔离 venv 以 `-I`、仓库外 cwd 运行已安装纯 wheel，确认 `_native` 不存在；同一 recipe 与安装后的 NativeBackend 比较几何、参数、错误字段、历史和保存状态相等 |
| A17 | Rust 测试验证结果数量/字节/TTL、过期 ID 水位、帧长度/UTF-8/非有限值/空数组及候选回收；另做 100 轮真实连接、绑定、创建候选、断线到期，回到 0 Edit、0 Job、1 正式 Session |
| A18 | Python 正常退出及被终止均不结束宿主；未提交修改不发布，候选到期释放；已提交内容与历史保留并可重连保存 |

100 轮实测及 RSS 的精确值记录于 results JSON。当前记录中正式文档计费始终为
2164 字节；结果窗口只保留当前客户端的少量结果。RSS 包含分配器与代码页驻留，
不把它等同于存活对象字节。此验收不使用关闭/重开正式 Session 清理候选。

## 重复执行

```sh
cargo test --locked -p zipzi-runtime
python3 tools/python/test_connected.py

# 可选：先安装/刷新原生扩展，再比较同一个公共 recipe。
python3 tools/python/test_connected.py \
  --native-python "$PWD/target/python-tests/develop-venv/bin/python"

# 当前源码编译的生产桌面宿主：双进程冷启动、开发握手、桌面重开、保存。
cargo build --locked -p zipzi-editor
ZIPZI_TEST_EDITOR_HOST="$PWD/target/debug/zipzi-editor" \
  target/python-tests/connected/bin/python -I \
  crates/zipzi-python/tests/test_editor_connection.py

# 原生 UI + 安装后 Python + 路径竞争 + Metal。
ZIPZI_CLIENT_PYTHON="$PWD/target/python-tests/connected/bin/python" \
  env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor \
  --features test-support --test runtime_ipc_acceptance
```

Editor acceptance/full shell 门禁自动准备隔离客户端，也可用
`ZIPZI_CLIENT_PYTHON` 指定既有安装。直接 Cargo 原生验收缺少 Python/Metal 前置条件时
明确失败。基础 Rust Runtime 是 36 个测试通过，安装后连接套件是 14 个测试通过，
共享 recipe 另行通过；原生生命周期聚焦 18 个测试通过；开发启动器 17 个、Editor
测试入口 9 个、inventory 3 个、依赖方向 1 个、纯 wheel 1 个工具测试通过。
环境为 Apple M4 arm64、macOS 27.0.1、Rust 1.99.0、CPython 3.14.7、Debug。

首次 `./tools/test-sdk.sh python -- -k history` 刷新了 Debug 扩展并通过 10 个 CPU
history 用例，但 inspection 过滤匹配 0 项，包装门禁返回 5；不将该命令记作整体通过。
随后直接执行公共生命周期聚焦测试并完成双后端同 recipe 验证。开发中还修复了
macOS accept 继承非阻塞标志导致分段控制帧失败的问题，并增加真实多帧/事件验证。

未运行完整 SDK/Editor/full、Release wheel/发行安装位置、R7 总性能验收；这些不作为
本阶段已通过的证据。R2 的交互性能结论继续适用，R3 不宣称新的固定 RPC 时延。
R4 从完整场景/资源/Plan/工程操作、无状态 CPU 求值及底层图片计费回收开始。

## Review 修复验证（2026-10-24）

本轮修复四个审查问题：待确认请求绑定原 Edit（包括 commit 的 Job 查询），同连接
其他 Edit 的未发送操作返回 Busy 且保持可编辑；参数替换在一次 apply 内保留候选的
runtime ID、kind 和精度，不再先向客户端读取；request_status 传输失败丢弃 socket，
保留原请求及其归属；Unix host fixture 的导入、实现与入口均受 cfg(unix) 保护，
非 Unix 入口不引用 Unix API。

新增参数保留字段后，严格握手契约从 `zipzi-runtime-r3-1` 升为
`zipzi-runtime-r3-2`。旧宿主需要重新构建并重启；旧契约在握手时被拒绝。
上文原始交付的测试记录对应修复前版本，本轮补充验证如下：

- `cargo test --locked -p zipzi-runtime --test protocol`：7/7 通过，覆盖候选字段保留、显式覆盖、同 ID 去重和旧契约拒绝。
- `python3 tools/python/test_connected.py --native-python "$PWD/target/python-tests/develop-venv/bin/python"`：18/18 通过，共享 Native/Connected recipe 一致；日志 `target/r3-review-connected-tests.log`。
- `cargo check --locked -p zipzi-runtime --all-targets`：macOS 通过；日志 `target/r3-review-check.log`。
- 新增连接用例覆盖两个 Session/Edit 间 commit 与终态 Job 响应丢失、被阻止的 apply/cancel、参数替换响应丢失，以及有/无待确认 Edit 时的状态查询超时。

本机只安装 aarch64-apple-darwin target，未实测 Windows 编译；未重跑完整 Editor
或 Metal 验收。此前审查中的 Clippy 运行因未修改的 zipzi-core 告警失败，不计为通过。
