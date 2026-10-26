# Runtime R5 / Python P5：独立 CPU Preview

2026-10-26。状态：**本轮 CPU Preview 切片已接通并通过定向验证；R5/P5 整体仍在实施。**
基于 rebase 后的 `0a208fba`，在 `python-p4-resources` worktree 中继续开发，
本记录对应独立 CPU Preview 切片提交。沿用[动画创作](python2-r5-authoring.md)和既有 Job/恢复/二进制契约。

## 实现与边界

- `session.preview()` / `edit.preview()` 捕获确切正式版本或候选序号。每个 client-owned
  Preview 都有独立的 SDK `MotionPreview`、参数、Motion/Expression schedule、时钟、
  Physics/Pose 状态和 seek cache。原模型、历史和 SDK 默认 Preview 均不被修改。
  后续提交、候选写入或取消不刷新已有来源；需要新内容时显式创建新 Preview。
- `animation` 模块提供生成的关键词参数控制值：SetParameter、ScheduleMotion、
  ScheduleRegistration、ScheduleExpression、ScheduleParameter、SetPhysics、Reset。
  控制直接转发 SDK。Reset 回到零时刻并保留 schedule，不能安排早于当前 SDK 时钟的激活。
- `sample(times)` 返回 1–64 帧的 `Job[Snapshot[animation.Batch]]`；`seek` 和 `advance_to`
  返回单帧 batch Job。顺序、重复和倒退目标保留，可选 CPU geometry。帧携带 Preview
  serial、操作序号、实际时刻、参数、SDK 局部采样坐标、事件/覆盖说明和 geometry。
  serial 与外层 Source.runtime 一起区分视图，Source 继续区分旧正式/候选内容。
- 工作线程只持有独立 Preview 和冻结来源，正式会话保持 Ready。相同 Preview 的并行
  控制/读取/采样返回 Busy；其他 Preview 可继续操作。沿用现有读任务队列，默认两个
  worker 中最多一个执行读任务，为正式发布保留另一个；完整公平调度仍待后续实现。
- seek/advance 调用 SDK 可取消入口。中断中的单次 seek 保留原 SDK 状态；取消 batch
  可以保留已完成目标，incremental advance 可以保留已完成步。取消的输出被丢弃。
  SDK 已完成整批后到达的取消报告实际成功，避免时钟已经推进却声称回滚。
- 关闭 Preview 请求取消其任务并释放表项；仍由 Job 持有的来源/工作区待任务终结及
  Job 释放后回收。客户端过期、Session detach/close 清理对应 Preview；晚到结果不会
  复活表项或修改正式文档。结果是独立拥有的 Python 值。

默认每客户端最多 8 个 Preview，Preview 总估算上限 128 MiB；每实例 SDK checkpoint
cache 为 2 MiB，最多保留 4096 次激活。创建和采样在展开前预留来源、模拟、cache、
几何和输出空间，并计入 Runtime 总量/冻结来源预算。`resources.usage().previews`
报告实际表项数；这些估算和表项计数不是 RSS、GPU 或全进程内存结论。

Motion 在首个规范更新步激活：本轮线性曲线中 Preview 时钟 `.25s` 对应 SDK clip
局部时间 `14/60s`，`.5s` 对应 `29/60s`。数值检查使用返回的实际局部坐标，再按
线性插值独立核对 geometry，未把时钟直接当作 clip 局部时间。

当前 native/IPC 配对为 `zipzi-python2-16` / `zipzi-runtime-r5-preview-1`。
需同时重建 adapter/host；旧配对保留历史身份，不做兼容层。分发版本仍为
`zipzi` / `zipzi-native` `0.1.0a1`。38 个生成产物覆盖类型、codec、facade、目录和文档；
Local 使用直接原生转换，Connected 数值结果使用既有分块二进制与原 RequestId 恢复。

## 验证

- 九项 Python 定向测试通过：五项新增 Preview 测试，加上动画创作、P4、手工网格
  和原生准入回归；适用案例运行 Local/Connected。共享
  [r5_preview_recipe.py](../../../crates/zipzi-python2/tests/r5_preview_recipe.py)
  验证线性 Motion/Expression 的参数和网格位移、Pose opacity、多个视图、候选来源、
  正式变更后的冻结内容和一个驻留会话内 100 次创建/释放。
- Connected 另验证两个客户端同会话预览、断开一个客户端后另一个仍可用；Local 和
  Connected 都通过受控已进入 worker 的取消、Busy 及正式历史不变检查。
- 创建、控制、采样提交、Job 结果、二进制块读取和释放六阶段丢响应恢复通过；序号
  保持唯一。大负载为 32 帧 × 1024 顶点，从第二个二进制读取响应丢失处恢复，
  每帧内容、序号与顺序完整。Local Preview 的 Python JSON 编解码为零。
- Runtime Preview 四项、现有动画创作两项和冻结读任务三项，共九项通过。新增覆盖
  queued cancel、晚取消成功、错误所有者、预算拒绝、候选取消、客户端过期和
  100 次取消/释放后 Runtime 计费返回基线。
- SDK 现有 `basic_playback` 五项通过，包括实际 Motion/Expression/Physics 数值语义；
  本轮仅添加公开值类型的 ApiShape/BinaryWire 及字段说明，没有重写动画算法。
- 严格正类型通过；公共反例为预期 22 条，恢复反例为 7 条，位置/规则匹配。
  19 项生成工具测试通过，生成一致性与共享索引检查通过。
- 正常纯 wheel 在无原生扩展的隔离环境、从 `/tmp` 使用 `-I` 通过
  D01/D02/D04/D07/D08：823 个符号、1553 个可调用项、1921 个字段。
- 两份配对源码包、两份正常 wheel 与从归档源码构建的 Release CPU host 验证通过。
  解包后的源码生成检查通过，Local 安装纯包和原生包，Connected 仅装纯包，
  隔离解释器使用 `-I` 运行当前 installed recipe。两后端完整报告相同，包含本轮
  Preview recipe、此前 R4/P3B/P4 和动画创作流程；未复用旧配对 adapter/host。

临时日志、失败迭代和最终产物位于 `target/python2/r5-preview/`；早期开发迭代日志
保留在 `target/python2/r5-authoring/reports/preview-*`。定向入口：

```sh
./tools/test-sdk.sh python2 PythonSdk.test_r5_independent_previews_numeric_geometry_and_candidate_sources
cargo test --locked -p zipzi-runtime --test previews --test read_jobs
cargo test --locked -p zipzi-animation --test motion_preview basic_playback::
```

## R5/P5 仍待交付

空间查询/诊断、sdk-observe 与 inspection、候选纹理和观察结果的完整来源验证、
持续批任务公平调度，以及 Editor Timeline/双视口/人工播放钟与 Physics 的视图隔离、
旧候选帧拒绝、Metal 和 UI 性能验收。本轮没有重跑 Editor、GPU、固定 agent 矩阵或
性能基准，没有新增模型/服务身份门禁；CPU 配对通过不能替代这些后续范围。

## 2026-10-26 Review 修复

重复 Motion 激活会复制每次触发的事件和未解析 Track 诊断，不能仅按激活条数收取固定字节。
SDK 现在提供规范采样的 Motion 输出上界（含短周期循环）；Runtime 在启动 Job 前同时
预留输出、事务回放及观察捕获的副本，并在 Job 释放后继续计入 Preview 保留的状态。
预算拒绝不推进时钟或操作序号、不入队。新增回归覆盖 20 MiB 上限下的重复大事件拒绝、
正常事件采样/释放，以及短循环和大诊断字段。GPU 观察沿用相同的采样准入。

定向验证：Runtime Preview/Observation 九项、SDK 动画 basic_playback 六项、
sdk-observe capture 十一项，以及 Python Local/Connected 六项 GPU/inspection
用例和正反静态类型检查通过。预算回归同时覆盖无 view 与 GPU view 的拒绝路径。
