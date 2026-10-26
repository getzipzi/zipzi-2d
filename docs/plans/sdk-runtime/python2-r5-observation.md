# Runtime R5 / Python P5：GPU 观察与基础 inspection

2026-10-26。状态：**GPU 图像 Job、冻结纹理和 PNG/JSON inspection 导出已接通，源码/类型/正常分发验证通过；R5/P5 整体仍在实施。**
基于 CPU 空间查询提交 `0b826880`，在 `python-p4-resources` worktree 继续开发；本轮改动尚未提交。

## 实现

- `session/edit.modeling.observe(poses, observation.View(...))` 显式返回
  `Job[Snapshot[observation.Batch]]`。每批捕获确切正式版本或候选序号，借用既有冻结读任务、
  准入、取消、恢复和二进制机制；不转移正式 Session，也不新增快照或后台状态机。
- Preview 的 `sample/seek/advance_to(..., view=view)` 可同时请求图像、CPU geometry 和空间
  结果。图像来自该帧同一个 SDK 动画状态，应用 Model opacity；外层 Source、Preview
  serial/sequence、MotionSnapshot 的实际时间和参数继续区分来源。无 view 的调用保持 CPU
  路径，不探测 GPU。GPU/捕获阶段失败可能保留已完成的 seek；取消输出会丢弃。
- 修复 SDK 原先把候选的文档修订号与正式基准版本比较而误拒绝的问题。GPU 捕获现在
  比较 Core 已有的不可变 snapshot identity；不同候选即使局部修订计数相同也不能混用。
  该检查为常数时间，不展开或重新 hash 整个文档。
- Runtime 接入 `zipzi-sdk-observe`，逐批解析纹理并固定实际解码内容，再共享给该批全部帧。
  候选 PreparedImage 在调用方释放、后续替换或取消后仍由冻结来源保留。
  磁盘资源在 worker 执行时解析并校验；缺失、被修改或无效的资源明确失败，不用新内容冒充
  旧来源。结果带实际纹理哈希和 capture/scene/render/input digest。
- 图像返回 Rust 生成的 RGBA8、PNG、实际 adapter/backend、参数、模型到图像的映射及
  drawable 几何 bounds。显式 ROI 使用 Y-up 模型坐标，图像为左上原点；映射算法复用 SDK。
  bounds 不是遮挡后的可见像素证明。原始 alpha 约定保留 renderer 语义，不在 Python 重算。
- 每个观察 Job 按需创建 Observer，同一批复用设备、renderer 和纹理；终结后释放。
  本轮未提供跨 Job 驻留的 GPU Observer lease，也未承诺全进程/GPU 内存上界。
  创建 Runtime、导入纯包、能力读取和 CPU 操作不初始化设备。
- `capabilities().gpu` 在未探测时为 None，成功渲染后为 True，adapter/device 失败后为 False，
  并提供编译的 backend 家族和最后错误码；这是缓存的实际结果，不保证下一次调用成功。
  无设备路径通过注入真实错误类型的 Runtime completion 单元用例验证，未靠隐藏 skip 冒充 GPU 成功。
- `inspection.save(result, new_directory)` 仅组织客户端 PNG 和 JSON，保留 Source、视图、
  实际参数、纹理哈希、图像摘要及 Preview 帧身份。文件写入无需 Runtime/设备；已有目录
  拒绝覆盖，文件 IO 错误可以留下部分目录。它不是可重新求值的完整 SDK scene bundle。

最多 64 个姿态/时间、每姿态 256 个显式参数；显式图像边长最多 4096，仍受设备限制。
整批 RGBA 与 PNG 的输出预估上限 128 MiB，纹理 RGBA 预估上限 256 MiB，并在来源克隆/
展开之前保守计入 Runtime 总预算。SDK 已有的 encoded+decoded 纹理上限继续适用。
纹理解码和摘要之间、每帧渲染之前均检查取消；已经进入的单次解码/GPU 提交可完成。
默认两 worker 的现有发布保留策略继续适用，完整持续批次公平调度另行交付。

Native/IPC 配对更新为 `zipzi-python2-18` / `zipzi-runtime-r5-observation-1`；需配对重建。
分发版本仍为 `zipzi` / `zipzi-native` `0.1.0a1`，共 40 个生成产物。目录的协议标识改为由
编译导出的 Rust 常量生成；Runtime build 身份覆盖 observe/render crate 与 Metal/WGSL shader。
旧记录保留其历史协议身份。

## 验证

- 六项显式 Python GPU 测试通过，适用用例分别运行 Local/Connected。公共
  [recipe](../../../crates/zipzi-python2/tests/r5_observation_recipe.py) 实际检查红/绿/蓝像素、
  正式提交和候选取消后的旧纹理、参数移位、重复姿态、动画局部时间、Model opacity=0、
  同批 capture/hash、一致映射、PNG，以及本地检查报告。
  候选 Preview 在后续纹理替换及取消后仍能输出原蓝色内容。
- 提交/Job/二进制读取丢响应恢复通过；Preview 图像对应三阶段也通过。
  两帧 512×512 RGBA+PNG 大结果从第二个二进制块丢响应恢复，末帧像素和尺寸正确。
  已进入 worker 的取消、Local 零 Python JSON 编解码、无纹理候选失败后仍可编辑通过。
  保存后的磁盘纹理在排队后遭外部修改时，整批明确失败；恢复文件后渲染正确，历史不变。
- 八项 CPU 定向回归通过，覆盖空间、Preview/其他客户端、动画创作、P4 和手工网格。
  严格正类型通过；公共反例预期 28 条、恢复反例 7 条，位置/规则匹配。
- Runtime 观察三项、Preview 四项、冻结读任务三项和空间三项，共十三项通过；另一个
  无设备能力/CPU 隔离单元用例通过。两个实际 GPU 用例在默认 CPU 模式显式 ignore，
  本轮使用 `--include-ignored` 执行通过。100 次排队取消/释放后计费返回基线。
- SDK view 三项、捕获取消及候选身份回归通过；身份回归验证同正式基准、同局部修订的
  两个分支仍被区分，既有正式 Preview 的过期拒绝回归也通过。观察算法、Metal renderer
  和视图映射复用现有 SDK。
- 正常纯 wheel 的离线目录、字段、注解和文档检查通过：865 个符号、1628 个可调用项、
  2066 个字段；纯包检查未安装原生扩展，也未初始化设备。

- 19 项生成工具回归、40 个生成产物检查、共享索引检查及一项退役来源保护回归通过。
  分发工具原有两项测试和新增 Cargo 缓存回归通过。新增回归先重现标准化零 mtime 的
  归档错误复用旧产物，再验证解包时刷新文件时间可使共享缓存编译当前依赖；归档内容
  仍保持确定性。早期缓存复验失败不计为通过。
- 最终配对源码包、正常纯/native wheel、归档源码构建的 Release host 通过 12 个构建/
  安装步骤。实际 Metal 安装 recipe 验证候选 Preview 修复、纹理隔离、透明度、PNG/
  RGBA 摘要及检查导出，Local/Connected 数值和业务报告相同，环境报告仅 mode 不同。
  24 份变更 Rust 源码与归档逐字节一致；产物哈希复核通过，纯 wheel 与提前审计产物
  逐字节相同，最终纯安装环境从 `/tmp` 使用 `-I` 再次通过全部离线检查。

最终分发位于 `target/python2/r5-observation/distribution-accepted/`。其他日志与失败迭代
保留于 `target/python2/r5-observation/`，不替代最终配对证据。本轮改动尚未提交。

```sh
./tools/test-sdk.sh python2 --observation
cargo test --locked -p zipzi-runtime --test observation -- --include-ignored
cargo test --locked -p zipzi-sdk-observe --lib view::
cargo test --locked -p zipzi-sdk-observe --test capture cancelled_capture
```

GPU 测试及分发 recipe 均使用显式 `--observation`，默认 CPU 入口保持独立。

## 剩余范围

完整持续批次公平调度、Editor Timeline/双视口/人工时钟与 Physics 隔离、旧候选帧拒绝、
原生 UI 性能验收仍待交付。可重新渲染的 SDK scene bundle 等更完整 inspection 工作流，
以及非 macOS/WGPU 设备验收另列；本轮 Metal 图像通过不代表整体 R5/P5 或 Editor 已完成。

## 后续收尾进展

客户端公平轮转、批次续执行和 Editor 播放/Physics/双视口隔离现已通过定向回归，
保存中关闭标签页的生命周期冲突已修复；具体证据及仍待完成的原生性能/发行复验见
[调度与隔离记录](python2-r5-scheduling.md)。本切片原有数字描述当时的验证范围。
