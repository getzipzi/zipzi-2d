# P3B：剩余 Modeling 范围核销

2026-10-26。状态：**已完成原定范围核销和补验。** 前置提交为 R4 收尾
`2595acc5`；R4 主体实现早于 P3A/P3B 拆分。本轮没有新增或改写领域处理器，
没有增加公共 API、兼容包装或验收门禁；补齐 Python 公共调用的独立场景和类型证据。

## 范围与证据

| P3B 原定范围 | 现有交付与本轮补验 |
| --- | --- |
| 完整场景绑定 | R4 canonical `CreateSceneBinding`/binding authoring。新增 Rotation、Warp、Part 三种轨道的数值用例；分别验证普通网格轴及场景轨道/Glue/Offscreen 的参数范围映射；现有 BlendShape/带绑定拓扑用例继续通过 |
| 网格/绑定转换 | 显式 `Plan`、`plan_async`、成本报告；带反射/缩放/旋转的保参数域换父级、插入/移动/删除采样、稳定 ID 网格入口、增轴/重排/范围映射、最后一轴重采样烘焙及保存重开 |
| 集合关系 | 组织、变形与绘制集合使用 SDK 既有意图；验证改组织不改变形父级、显式换父级与同父级重排、绘制成员与顺序保留。绘制范围可被 SDK 规划正规化，不把它误记为成员关系变化；错误 anchor 返回 `INVALID_PLACEMENT` 且不改正式来源/历史 |
| 剩余 Ref/Address | 既有十类地址域的 Undo 失效回归全部通过；新增嵌套绑定样本删除/Undo 后旧 Ref 与地址均返回 `STALE_HANDLE`。对象自身在换父级时保持身份，旧标签地址按上下文失效 |
| 公共契约与两后端 | 使用现有 Snapshot/Plan/Job/恢复/所有权契约；增加 canonical records、转换 Job 和字段地址的静态类型示例。相同 recipe 在源码及隔离安装的 Local/Connected 上均通过 |

实现入口仍为 `modeling.ModelingCommand`、`session/edit.modeling.records`、
`session/edit.plan/plan_async`、`session.reference/address/capabilities` 及领域操作。
没有另造一套 Python 转换规则。CPU 建模采样属于本阶段已有能力，动画、独立
Preview 和 Observer 仍归 R5/P5。

## 本轮结果

新增公共 recipe：[p3b_recipe.py](../../../../crates/zipzi-python2/tests/p3b_recipe.py)，由
`test_api.py` 与隔离安装入口 `installed_recipe.py` 共用，只调用公开 API。
预期几何、历史步数、失效错误码与集合关系均由手写断言检查。

- 六项定向源码测试通过，每项均运行 Local/Connected；严格正类型检查、既有
  18 条公共类型反例和恢复类型反例检查均通过。
- 隔离环境从 `/tmp` 使用 `-I` 执行更新后的完整 installed recipe。
  Local 安装 `zipzi` + `zipzi-native`，Connected 仅安装纯包；均为 `0.1.0a1`。
  使用 R4 已独立构建的正常 wheel/CPU host；本轮产品源文件未改，未重新构建 wheel/host。
  当前随包文件与 wheel、Rust 源文件与 R4 构建源已逐文件确认一致。
- 当前 native sdist 已重新打包，新增 recipe、源码/安装调用入口和类型示例均包含在内，
  与工作区逐文件一致；不把本次打包记为重新编译 wheel。
- 两后端 P3B 报告一致：保形 sweep 的首顶点 X 为 `0, 2.5, 5, 7.5, 10`；
  扩轴后 6 个关键形，烘焙后 X 为 `5`，转换历史为 `(5, 0)`；Warp X 为
  `0, 2, 4`，Offscreen opacity 为 `0.2, 0.5, 0.8`。
- 同次安装流程中的既有 R4 recipe 也通过：65,536 顶点、130,050 三角形、
  9 类格式、驻留 100 轮取消和 100 轮替换、51 条历史图片/204 字节及空几何诊断。

定向命令（仓库根目录）：

```sh
./tools/test-sdk.sh python2 \
  PythonSdk.test_p3b_conversions_scene_bindings_and_independent_collections \
  PythonSdk.test_all_modeling_address_domains_keep_sdk_epochs_after_undo \
  PythonSdk.test_canonical_modeling_records_plans_and_bound_topology_recipe \
  PythonSdk.test_scene_organization_plan_and_metadata_recipe \
  PythonSdk.test_blendshape_bound_topology_and_candidate_cpu_sweep \
  PythonSdk.test_sdk_references_addresses_and_undo_invalidation
```

本轮日志位于 `target/python2/p3b-1026/`。R4 的大网格、资源、配对源码包/wheel、
Release 性能及真实 Editor/Metal 证据沿用[已提交记录](../sdk-runtime/python2-r4-complete.md#2026-10-26-r4-最终复验)，不声称本轮重跑了 Editor 或性能测试。

P3B 原定建模范围已核销；独立附件便利入口、动画/观察及平台/app 总验收不属于
本轮新增实现。下一步单独核对 P4 的资源/工程范围，再推进 R5/P5。
