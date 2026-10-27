# R5/P5：批次调度与 Editor 隔离

日期：2026-10-26–27。接续 GPU observation 切片；本记录不代表 R5/R7 整体验收完成。

## 交付范围

- Runtime 按客户端轮转读取任务，同一客户端的 Job 与续执行单元按队尾轮转。
  正式发布优先，并继续为多 worker host 保留发布槽；单 worker host 在批次让出后也可先发布。
- Modeling/空间查询每个姿态让出；独立 Preview 每个时间目标让出；图像捕获、共享纹理解析、
  逐帧 GPU 渲染之间让出。继续执行保留同一 Job、冻结 Source、序号、输出次序和预算预留。
- 同批 Observer、已解析纹理和 capture identity 跨调度保留，正式或候选资源在间隙替换不会混入批次。
  取消已排队的续执行单元立即终结 Job、释放持有并恢复 Preview 可用性；已完成时间目标继续保留。
- LocalHost、Connected fixture 和桌面 IPC 使用 `Work::run_quantum` / `run_on_thread`；
  `Runtime::complete` 接收续执行所有权并重新排队。桌面只在终结时通知视图，避免逐样本唤醒 UI。
  显式 `take_work` 遵守同一客户端轮转和并发上限，执行器拒收保留原轮次。
- Editor 已有视图持有的参数预览和独立 AnimationEvaluator，本轮补实证而未重写原生展示链：
  agent 独立 Preview 的参数、反复 seek 和 Physics 设置不改变人的播放钟、滑块、Physics 试听和双视口状态。
  外部 Motion 提交刷新资源目录及 Timeline 数据，并拒绝此前的 CPU/GPU 结果。

这不是固定毫秒抢占。单次 SDK seek/plan、图片解码、整批共享纹理解析和一次 GPU 提交仍是协作取消单元；
单次 seek 保留 SDK 的 1,000,000 步上限。持续批次之间已有公平轮转，原子单元的耗时须由原生性能复验评估。
同一 Timeline 编辑内按序接收在途帧的现有交互策略继续保留；新的编辑、提交、取消和更新后的已呈现结果形成拒绝边界。

## 验证记录

- Runtime 新增 7 项确定性调度回归：三客户端与突发队列、单 worker 发布插入、取消续执行 Preview、
  执行器拒收及显式调度绕过拒绝、客户端过期释放、视图关闭取消试听、已进入任务取消后仍占用执行槽，均通过。
- Preview 6 项、冻结读取 3 项、空间查询 3 项、Runtime lib 8 项通过。
- Runtime observation 3 项在实际 Metal 上通过；正式/候选测试在第一个采样单元后替换纹理，仍保持冻结像素与来源。
- Editor 新增 2 项集成回归：动画播放和实际 Physics 试听分别与 agent 采样交错，覆盖两个视口；
  外部 Motion 更改刷新目录/Timeline 并拒绝旧帧。全部 50 项 Timeline 回归通过。
- Python GPU/inspection/取消/恢复/大二进制 6 项在 Local、Connected 通过，正反静态类型通过；
  生成一致性检查仍为 40 个文件，公共 Python/IPC 契约未改变。
- `./tools/test-sdk.sh runtime`：113 项 CPU 回归通过，2 项 GPU 用例默认跳过；随后额外加入的执行槽回归与其余调度用例合计 7 项通过。
  GPU 用例另行显式运行，Metal 3 项全部通过。

完整 Runtime CPU 回归及桌面回归暴露现有 detach 接线冲突：持有未完成 Job 的客户端 Session
应拒绝 detach，但关闭 Editor 标签页需要允许后台保存继续。新增仅供宿主的 `detach_editor_view`，
Editor 关闭标签页走此入口；客户端 detach 仍返回 Busy。A08 同时验证两条路径及客户端过期后
已完成保存仍可发布，桌面保存失败提示用例保持原有断言并通过。

桌面 Animation、UI 工作量、Runtime 窗口/保存生命周期定向回归 70 项通过；Physics 目录/编辑定向回归 15 项通过。
Python Preview/空间/取消/大二进制定向回归另 4 项在两后端通过。
Python Session 别名/detach/关闭、断线响应恢复及其他客户端保护另 3 项通过，正反静态类型通过。

本轮[发行与总验收](python2-r7-closeout.md)已完成正常配对 sdist/wheel 和两后端 GPU recipe 复验，持续后台批次与人工交互的 Release 延迟/工作量比较继续收尾。
随后 R6 已按用户 2026-10-27 的决定收缩并完成[Editor 直写与人工接续](python2-r6-handoff.md)交付，Proposal/专用 Review 后置。本轮五对原生 Release 比较 122 项通过，实际窗口性能余项与 R7 总验收继续单列。
