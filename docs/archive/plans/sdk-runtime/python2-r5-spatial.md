# Runtime R5 / Python P5：CPU 空间查询

2026-10-26。状态：**CPU 空间查询切片已接通，源码/类型/正常分发验证通过；R5/P5 整体仍在实施。**
基于独立 CPU Preview 提交 `071aa0f8`，继续在 `python-p4-resources` worktree 开发。
本记录对应 CPU 空间查询切片提交，沿用既有 Source、Job、恢复和二进制传输契约。

## 实现与边界

- `session/edit.modeling.spatial(poses, queries)` 显式返回
  `Job[Snapshot[spatial.Batch]]`，对确切正式版本或候选序号求值。
  请求参数与有效参数分别保留，后者包含默认值和 clamp；姿态、查询的顺序和重复保留。
  后续提交、候选写入或取消不改变已经捕获的对象表、标签和几何。
- `spatial.Bounds` 使用 Mesh/Part UUID，Part 包含后代网格；结果给出解析后的稳定 ID、
  model bounds 或空结果原因。`spatial.Hits` 返回 SDK 画序降序的候选、完整总数和截断标志；
  可选详细三角形包含 buffer 索引、稳定顶点 ID、重心坐标和 UV，以及 Part 路径/标签。
  坐标为 Y-up 模型单位；这些是几何候选，未做纹理、mask、遮挡或可见像素判断。
- Preview 的 `sample/seek/advance_to(..., queries=...)` 在同一 SDK 动画状态求空间结果，
  `geometry=False` 时仍可返回空间结果。外层 Source 和每帧 Preview serial/sequence
  区分来源与采样请求。无效目标在 seek 之前拒绝，时钟保持原值。
- 共享 SDK 复用原空间算法，借用冻结文档对象表，不向 Python 复制所有网格/绑定再计算。
  含隐藏对象的求值帧支持 `include_hidden=True`；默认过滤 disabled/hidden/零 opacity。
  Part 祖先标签只在候选截断后复制；三角形扫描和层级遍历检查取消。
- Runtime 在克隆来源和展开结果之前，计入冻结文档、求值空间、详细三角形和 Part 路径
  的保守预算。最多 64 个姿态、每姿态 256 个显式参数、64 个查询、4096 个总选择器、
  每查询 4096 个保留命中候选；实际仍受字节预算限制。
  使用现有 worker/取消/释放机制，正式 Session 保持 Ready，读错误不使 Edit 失败。
- Preview 的已完成 seek 可以在后续空间查询被取消时保留；取消的输出被丢弃。
  SDK 单次 seek 内部的取消仍保留旧状态，整批已完成后的晚取消报告实际成功。
  本切片未改变此前声明的 Preview 取消和序号契约。

当前 native/IPC 配对为 `zipzi-python2-17` / `zipzi-runtime-r5-spatial-1`，需配对重建。
分发版本仍为 `zipzi` / `zipzi-native` `0.1.0a1`；39 个生成产物覆盖 typed 输入/输出、
binary codec、facade、目录和文档。Local 直接转换原生结构，Connected 返回分块二进制。

## 验证

- 十项 Python 定向行为通过：四项空间测试，以及 Preview 数值/恢复/大结果、动画创作、
  P4 和手工网格回归。适用用例运行 Local/Connected；公共
  [recipe](../../../../crates/zipzi-python2/tests/r5_spatial_recipe.py)
  独立检查位置 `0/5/10`、参数 clamp、Part 后代去重、命中画序/截断、退化三角形、
  空选择、隐藏筛选、稳定顶点 ID `(10,20,30)`、重心 `(.5,.25,.25)`、UV、冻结正式/候选
  标签，以及动画局部坐标对应的 bounds。
- 三阶段丢响应恢复覆盖提交、Job 结果和 binary 读取。8 姿态 × 2048 条详细三角形的
  结果从第二个二进制块丢响应恢复，每个末尾 triangle index/顶点 ID/重心均检查。
  已进入 worker 的取消和 Local 空间/Preview 查询零 Python JSON 编解码通过。
- Runtime 空间三项、Preview 四项、冻结读任务三项，共十项通过，包含无效目标在 seek
  前拒绝、候选仍可用、预算拒绝、100 次取消/释放后计费回到基线。
  SDK `spatial::` 选中五项通过（原空间一项及 alpha mesh 空间四项）。
- 正常纯 wheel 的离线目录/字段/注解/文档检查通过：853 个符号、1603 个可调用项、
  1997 个字段；修复 Python 3.14 同名方法遮蔽模块的注解解析问题。
- 严格正类型通过；公共反例预期 24 条、恢复反例 7 条，位置/规则匹配。
  19 项生成工具测试、共享索引检查及其一项退役来源保护回归通过。
- 两份配对源码包、两份正常 wheel 和归档源码构建的 Release CPU host 通过 12 个
  构建/安装步骤。解包源码生成检查通过，所有 Python 内容与当前源码一致；安装后
  使用隔离解释器和 `-I` 运行 Local/Connected recipe，覆盖空间查询、Preview、动画创作及
  既有 R4/P3B/P4 流程。两后端数值/业务报告相同，仅最终环境报告的 mode 字段不同。
  分发哈希复核通过，纯 wheel 与提前离线检查的产物逐字节相同；最终纯安装环境再次
  通过 D01/D02/D04/D07/D08。没有复用此前 Preview 配对的 adapter/host。

开发日志与产物位于 `target/python2/r5-spatial/`；失败迭代保留，不计作通过证据。

```sh
./tools/test-sdk.sh python2 PythonSdk.test_r5_spatial_frozen_sources_hidden_geometry_and_animated_numerics
cargo test --locked -p zipzi-runtime --test spatial --test previews --test read_jobs
cargo test --locked -p zipzi-sdk --lib spatial::
```

## 后续

sdk-observe 与 inspection、候选纹理和观察结果来源验证、持续批任务公平调度，
以及 Editor Timeline/双视口/人工时钟与 Physics 隔离、旧候选帧拒绝、Metal 和 UI
性能验收仍待交付。本轮 CPU 查询通过不代表 GPU、Editor 或整体 R5/P5 完成。
