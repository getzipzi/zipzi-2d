# Runtime R5 / Python P5：首批动画创作

2026-10-26。状态：**R5/P5 实施中，首批动画创作已接通；整体阶段未完成。**
P4 已提交为 `e1401632`，本轮在同一独立 worktree `python-p4-resources`
继续开发。本记录对应首批动画创作提交；手工网格重构由 main 独立推进。

## 实现范围

| 范围 | 本轮结果 |
| --- | --- |
| 四领域创作 | Motion/注册组/Track/Key/Event、Expression、Physics、Pose 的现有 SDK edits 与整资产命令进入同一 Runtime candidate/batch，普通失败终止整批，发布仍为一个历史步骤 |
| 读取与来源 | `ModelingQuery` 增加选取式动画 records；保留请求顺序、重复与缺失槽。Physics/Pose 用显式布尔值请求单例，空选择只列身份。Snapshot 固定正式版本或候选序号，不保留 Python 全模型镜像 |
| Plan/地址/能力 | 同步及异步 Plan 调用现有 SDK/core planner，保存确切来源；注册、Expression entry、Physics channel、Pose fields 使用 SDK 地址。四个对象能力领域和对应集合能力经共享 Runtime 查询 |
| 生成与二进制 | 共享 Rust 声明生成有限类型、原生转换、二进制 codec 和随包目录；`SharedVec<T>` 使用相同 Vec wire layout，先计费再分配。Physics 扩展仅允许有限的 `BTreeMap<String, Value>` flatten，拒绝覆盖已声明字段 |
| SDK 地址修复 | 新增动画字段 epoch，字段变更及 Undo/Redo 均退休旧地址，避免值恢复后旧地址复活。Physics change summary 按稳定 ID 比较，避免同级重排误判为字段修改；Pose 单独记录实际字段变化，子项变化不会退休根 FadeIn 地址。没有全模型克隆或另一套领域规则 |
| 公共契约 | 沿用 Snapshot/Job/Plan、资源所有权、二进制与完整调用恢复；Local 动画调用没有 Python JSON 编解码。Runtime 仅新增 `animation_authoring` 能力声明 |

当前配对为 native `zipzi-python2-15`、IPC `zipzi-runtime-r5-authoring-1`，
纯包和可选原生分发仍为 `zipzi` / `zipzi-native` `0.1.0a1`。
旧协议组合不兼容，需重新构建 adapter/host；不增加迁移层。
历史 R4/P3A 配对与验收数据保留原身份，不改记为本轮通过。

## 验证

- 八项定向 Python 公共测试通过，覆盖新增四领域流程、4096 段曲线、完整调用恢复、
  Local 无 Python JSON 往返、Physics 扩展冲突回滚，以及受影响的 R4/P3B/P4 场景。
  适用案例运行 Local/Connected，传输丢响应案例运行 Connected。
- 共用 [r5_authoring_recipe.py](../../../../crates/zipzi-python2/tests/r5_authoring_recipe.py)
  检查混合事务、候选读隔离、重复/缺失读取、能力、Plan、字段地址改动后 Undo
  仍失效、失败回滚，以及 Motion3/Expression3/Physics3/Pose3 实际导出和工程重开。
  独立核对曲线段 `[0,-1,0,1,1]`、事件时间/文本和 Expression JSON。
- Runtime 动画/类型建模/引用七项测试、SharedVec 两项测试通过。
  SDK 四个动画领域共 73 项通过，另一个既有用例忽略；新增字段地址回归一项通过，
  分别检查四领域的字段变更后 Undo，不把忽略用例计为通过。
- 18 项生成工具测试通过，包含真实 Rust 负编译、确定性、检查模式、手写文件保护、
  Physics flatten 扩展拥有性与冲突检查、单变体类型别名的字段/方法目录；
  37 个生成产物检查一致。
- 共享 Runtime 源码目录已刷新，`--shared-only --check` 与工具回归通过；
  保留历史 Python 索引，没有读取已退役客户端实现。
- 严格正类型检查通过；公共误用反例为预期的 20 条诊断，恢复反例为 7 条，
  位置及规则与标注一致。
- 正常纯 wheel 在不安装原生扩展的隔离环境、从 `/tmp` 使用 `-I` 运行离线目录检查，
  D01/D02/D04/D07/D08 均通过；覆盖 789 个符号、1482 个可调用项、1830 个字段。
  首次检查发现单变体类型别名未继承字段/方法目录，修复生成器后复验通过，
  首次失败与最终结果分别保存在临时报告中。
- 新配对的两份正常源码包、两份 wheel 均构建成功；解包后在无 Git 的源码树中
  重新检查生成产物，并从归档源码构建正常原生扩展和 Release CPU host。
  两套隔离安装环境使用 `-I` 运行共享 installed recipe，Local 为纯包加原生包，
  Connected 仅装纯包，完整报告一致。R5 四领域及 Pose 地址修正、R4/P3B/P4
  现有流程均通过；本轮未复用旧 R4 adapter/host。

临时日志与分发结果仅放在 `target/python2/r5-authoring/`；最终配对产物为
`distribution-current/`，中间构建与失败报告保留原范围。定向入口：

```sh
./tools/test-sdk.sh python2 PythonSdk.test_r5_animation_authoring_plans_addresses_and_sidecars
cargo test --locked -p zipzi-runtime --test animation_authoring
cargo test --locked -p zipzi-sdk --test integration animation_field_epochs::
uv run --locked python tools/test_tools.py -p test_python2_codegen.py
```

没有运行完整 SDK/Editor 套件，没有重跑固定 agent 矩阵，也没有新增模型/服务身份门禁。
本轮仅验证 Local 桥接没有新增 Python JSON 往返；未建立动画吞吐、堆峰值或 UI 性能结论。

## R5/P5 仍待交付

1. 每客户端/每视图独立 Preview，动画批量采样、seek、播放与空间诊断；
   人的播放钟、参数滑块、Physics 状态和正式历史隔离。
2. sdk-observe、候选新增/替换资源的观察与 inspection；结果携带准确来源、时间和参数，
   拒绝旧候选帧及混合资源来源。
3. 持续采样/观察的取消、并发预算与公平调度，及后台批次与人工精修并行时的测量。
4. Editor Timeline/资源/检查面板及播放来源刷新，双视口/动画 worker/Metal 验收。

下一步从独立 Preview 的来源和生命周期接通 Runtime/Python，复用本轮类型化创作、
Snapshot/Job/恢复能力。R5 和 P5 在上述范围完成前保持“实施中”。

## 2026-10-26：提交后 rebase 到 main

首批动画创作先提交为 `8ad1d30f`，随后按用户要求 rebase 到本地 main。
先接入 `8efb0288` 的手工网格结构、旧 Python 适配器删除和格式整理，再接入
验证期间新增的 `2d85c2b0` 手工网格纯编辑操作。P4 的提交由 `e1401632`
改写为 `d0d401c2`；本记录前文及原始产物中的旧 SHA 保留其历史含义。

冲突处理保留主线网格结构及动画创作，合并两组公共测试，重新生成 37 个 Python
产物和共享 Runtime 索引。索引为新 `manual_mesh` 模块登记几何领域分类，
不将新增纯 Rust helper 的索引视作已实现 Python 连接。

`8efb0288` 组合上的十项公共 Python 定向回归、严格正反类型、19 项生成工具测试、
七项 Runtime 测试和一项 SDK 动画地址回归通过。接入 `2d85c2b0` 后，
动画创作、手工网格、P4 三项公共用例及类型复验通过，共享索引检查与工具回归也通过；结果见
`target/python2/r5-rebase-main/reports/`。

前文源码包/wheel/安装证据对应 rebase 前的树。本次重新构建匹配的 Debug adapter
和 CPU host 做源码回归，未重新执行发行打包或 Editor 验收；旧配对产物只保留为
历史证据，不能作为 rebase 后的构建使用。R5/P5 整体仍为实施中。

随后已接通[独立 CPU Preview 切片](python2-r5-preview.md)，覆盖冻结来源、
多个实例/客户端、采样/seek/advance Job、取消和二进制恢复。该记录更新前文的
Preview 待办；Observer、空间查询、完整调度与 Editor 播放隔离继续待交付。
