# R6：Editor 直写与人工接续

日期：2026-10-27。范围按用户确认的简化方案交付：agent 批量修改 → 提交到 Editor 当前 Session → 人工检查/精修 → agent 读取最新版本接续。Proposal/seal、专用对比和接受/拒绝 UI 后置，R5 性能余项与 R7 发行总验收分别跟踪。

## 实现与使用

正式写入复用已有 Session/Edit/Job、SDK 历史与 Editor 事件刷新。flush 指 `edit.commit().wait()`；正常 Edit 上下文退出也会提交并等待。一次有实际改动的成功 Edit 对应一个 Undo 步骤，保存磁盘继续使用原保存入口。

- 随纯 Python wheel 交付 `zipzi.docs.editor_handoff`：连接已运行的 Editor、列出 Session 身份、明确绑定目标 Session/mesh，再执行批次提交。`rename_mesh` 在新 Edit 中读取候选当前值，示例使用既有版本/Busy/恢复契约。
- 新增独立 `runtime_handoff_acceptance`，使用隔离安装的纯客户端、真实 Python 子进程和生产 Editor/IPC/Metal 链。合成素材必跑，不依赖授权模型；纳入 Editor acceptance/full 入口。
- CPU 集成覆盖两轮 agent 提交、人工属性/Timeline 精修、Undo/Redo、过期候选拒绝、脚本客户端释放及对象/key 身份连续。
- 定向回归发现执行槽满时，Editor 保存/加载拒绝的清理还会尝试占用 worker，导致排队 Job 和 Busy 状态残留。补 `Runtime::cancel_queued_job` 归还未执行任务的正式状态并释放预留；宿主在同一 Runtime 写锁内完成领取或取消，已进入执行器的任务继续占用执行槽。

运行随包示例：

```sh
python -I -m zipzi.docs.editor_handoff list
python -I -m zipzi.docs.editor_handoff rename \
  --runtime RUNTIME_UUID --session SESSION_SERIAL --mesh MESH_UUID --name "Refined face"
```

`--endpoint /absolute/path/to/profile.sock` 放在子命令前可选定宿主。示例使用明确的对象身份，提交后客户端退出仍保留正式文档与资源；人可直接在 Editor 中继续编辑。

## A19 与相关验证

原生验收在带实际 Metal 的测试窗口中执行，使用正式 UI 控件与键鼠事件：

1. 已安装的纯 Python 客户端在一个 Edit 中改 mesh 名称、替换内存纹理、修改 Motion/key，显式提交/等待；Python 退出后，Editor 列表、动画资源及实际画布更新，历史恰好增加一步。
2. 通过 Properties 文本输入/Enter 改名，走 Editor 命令 Undo/Redo，再通过 Timeline 数值拖动精修原 key。每次人工编辑独立形成历史步骤。
3. 新 Python 进程绑定同一 Session，读取人工精修后的版本、名称、key 和纹理。新批次保留人工终点 key，修改另一 key 和 mesh 名称；Editor 一次 Undo 可撤销该整批，同时保留人工精修。
4. 随包示例的 no-op 保留 redo；取消临时图片候选释放图片负载；实际安装的命令行示例再次提交。最后的原生动画帧仍包含人工结果，Session 和对象身份连续。

回收检查区分实际 RGBA/PNG、准备中的负载及底层资源条目，与协议恢复窗口内保留的二进制查询响应。后者计入有界 blob 预算，不将其误判为取消图片未回收。

已有 A07/A09 的完整调用恢复、手势 Busy 与旧版本拒绝继续复用；原有 `runtime_ipc_acceptance` 也在本轮复验。窗口/宿主关闭重开的既有证据见 [R2](r2-editor.md)及 [R5 调度/隔离记录](python2-r5-scheduling.md)，本轮没有另建窗口或会话镜像。

## 检查与边界

- `runtime_handoff_acceptance`：安装纯 wheel、四次真实 Python 进程、原生属性/Timeline 输入、共享历史与 Metal 通过。
- `runtime_ipc_acceptance`：原有 R4 原生链通过，包含人的手势占用、Busy/过期候选及内存资源投影。
- Editor-core `runtime::`：19 项通过，包含新增 R6 CPU 闭环及两个执行槽拒绝清理回归。
- `./tools/test-sdk.sh runtime`：115 项 CPU 回归通过（含 8 项调度回归），2 项 GPU 用例按该入口约定跳过；本轮原生 Metal 验收另行实际执行并通过。
- 严格 Pyright：0 errors / 0 warnings；生成一致性：40 文件通过。新增随包示例/指南更新了 catalog 与 reference 的客户端来源摘要，公共协议和生成处理器未改变。
- `tools/test_tools.py -p test_test_runner.py`：9 项通过；安装示例 `--help` 和实际 `rename` 命令通过。

本阶段交付的是简化后的共享文档交接。原生验收使用生产库的 `test-support` 测试窗口与实际 Metal；正常应用包位置/完整平台安装、最终全领域配对发行和 R5 Release 性能比较仍归原阶段。Native/IPC 配对标识不变：`zipzi-python2-18` / `zipzi-runtime-r5-observation-1`。

原始日志、隔离安装包和运行产物位于工作区 `target/r6-*` 及 `target/python2/connected/`，不把日志或 JSON 清单纳入计划目录。
