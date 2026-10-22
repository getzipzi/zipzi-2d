# C6 核心验收与 UI 交付

2026 年 10 月 22 日。对应[核心方案](unified-object-authoring-core.md)第 10、11 节。
状态：**C6 核心验收与交付完成**。
对象全清单仍由 [C5 覆盖矩阵](unified-object-authoring-c5-coverage.md)维护；本文固定验收
入口与交付边界，不把后续菜单、焦点、快捷键和拖拽统一计入核心完成条件。

## 公共契约

SDK 的[对象创作契约](../../../kasane-2d/modules/kasane-sdk/AUTHORING.md)列出各领域
规划／执行／事务入口、稳定身份、字段地址、锁、单一历史、通知消费、支持矩阵、
有限预算与诊断处理。SDK README 中旧版“忽略锁”、项目版本及已合并的测试入口已修正。

- 所有写入和 Undo/Redo 发布同一 `ChangeSet`；同步回执和事件队列是同一个通知。
  多领域事务必须消费全部变化，不能根据第一个领域提前忽略其他字段。
- SDK 领域条件和 Editor 会话条件保持来源分离。建议选择／定位不进入历史，不隐式
  激活 Motion／Expression。普通整理操作保持活动注册、播放时间与预览实例。
- 核心类型不支持、动态条件不满足、转换保证不足分别报告。C4 支持矩阵保持原语义；
  不通过改用较弱策略掩盖拒绝，也不将位置保证表述为全部外观保证。
- 原生项目目录格式 20、core schema 11、观察包 schema 6；不新增迁移。
  原生保存保留待配置内容，严格外部导出继续拒绝无法表达的数据。
- 默认 Binding 预算为 65,536 组合、64 MiB 临时估算、256 MiB 历史估算；SDK 历史
  为 50 步／256 MiB。图像编码 64 MiB、单轴 16,384 像素、RGBA 256 MiB。
  这些上限及估算不等同于进程 RSS。性能复核见本轮[测量记录](../benchmarks/2026-10-22-core-authoring-c6.md)。

## 第 10 节验收映射

SDK 测试模块均注册在相邻仓库 `modules/kasane-sdk/tests/integration.rs`。
下表中的 Editor 指本仓库 `crates/editor-core/tests/cases`；模块名是可直接筛选的入口。
每行覆盖核心方案同序场景；完整支持范围与各领域例外仍见覆盖矩阵和公共契约。

| # | 场景 | 回归依据 |
| --- | --- | --- |
| 1 | 空项目 Mesh／Glue／Texture／Attachment | `authoring_acceptance`、`scene_lifecycle`、`resource_authoring`；Editor 资源保存与对象投影 |
| 2 | 空 ParameterGroup／MotionGroup／PoseGroup | `organization_authoring`、`motion_library`、`pose_authoring`；跨领域保存往返 |
| 3 | Physics 根与组一次创建／撤销 | `physics_authoring::root_and_group_are_one_transaction_and_ids_survive_native_save` |
| 4 | Motion 组改名、移组与根注册 | `motion_library`；Editor `animation::motion_authoring::library_organization_keeps_registration_selection_clock_and_playback_run` |
| 5 | 共享 Clip 注册／组删除与最后注册 | `motion_library`；Editor `animation::motion_authoring` 的删除与共享注册回归 |
| 6 | Pose／Physics／Expression 重排后的地址 | `pose_authoring`、`physics_authoring`、`expression_entries` 的 reorder／owner roundtrip 回归 |
| 7 | ParameterGroup 嵌套、Part 组织移动 | `organization_authoring`；Editor `organization_authoring`、`sorting` |
| 8 | 非线性父级拒绝／原子性 | `binding_authoring::conversions::reparent::singular_driven_and_nonlinear_parents_reject_without_mutation` |
| 9 | 轴重排、Sample 与 Keyform 映射 | `binding_authoring::stable_domains::identity`、`shared_tables`、`target_grids` |
| 10 | Physics 节点增删与逻辑输出 | `physics_authoring::node_insertion_and_joint_replacement_do_not_retarget_output_by_index` |
| 11 | 删除、跨文档与异步旧结果 | `handles`、`expression_entries`、`resource_authoring`；Editor 跨活动 Document 完成拒绝 |
| 12 | 非法属性、no-op、取消保留 redo | `expression_entries`、`motion_editing`、`physics_authoring`、`resource_authoring`；Editor `properties` |
| 13 | 候选、多选、文件夹单历史 | `authoring_acceptance` 的全领域单提交和末尾失败回滚；各域候选／组织多选回归 |
| 14 | 原生保存重开完整身份与内容 | `authoring_acceptance`、`project_io`、project 测试、Editor 保存及纹理回归 |
| 15 | 外部格式行为与 ID 边界 | `expression_entries`、`motion_library`、`physics_authoring`、`pose_authoring`；project／CMO3／MOC3 测试及授权模型往返 |
| 16 | 不透明扩展与未解析导入 | `cdi_authoring`、`resource_authoring`、各格式保护与修复回归 |
| 17 | 无内容纹理、遮罩和 Offscreen 传播 | `resource_authoring::missing_image_content_propagates_masks_and_composites_without_losing_geometry` |
| 18 | 条目压缩映射与修复后语义顺序 | `reference_authoring`、`physics_authoring`、`pose_authoring`、`motion_library` 的 projection 回归 |
| 19 | 预览与可用内容导出同源 | `expression_entries`、`motion_library`、`physics_authoring`、`pose_authoring` 的 available export／omission 回归 |
| 20 | 零轴最新基础字段及末轴写回 | `binding_authoring::conversions::mesh_grid::zero_axis_uses_latest_base_and_last_axis_writes_selected_shape_back`；Editor 零轴回归 |
| 21 | 三形态新增轴保持旧域 | `binding_authoring::conversions::mesh_grid::expand_three_distinct_shapes_and_reorder_preserve_old_domain` |
| 22 | 插点／减轴／共享表重基准与所有使用者 | `binding_authoring::stable_domains::{shared_tables,shared_rebase,shared_validation,offscreen_blend}` |
| 23 | 容量、溢出、协作取消 | `binding_authoring::conversions::validation`；C4 Release 探针验证真实分配峰值和取消释放 |
| 24 | 参数范围与所有依赖联合映射 | `binding_authoring::conversions::parameter_range`；Editor 联合范围 receipt／Undo |
| 25 | 静态可逆父级的必需成功组合 | `binding_authoring::conversions::reparent`、`stable_domains::shared_tables::affine_reparent_transforms_shared_mesh_deltas_without_translation_or_identity_loss` |
| 26 | 新驱动、奇异／非线性与保证范围 | `binding_authoring::conversions::reparent` 的 captured snapshot／domain／support matrix 回归 |
| 27 | 异步加载、删除、Undo 后旧 incarnation | `resource_authoring::newer_cancelled_and_undone_image_loads_cannot_publish_old_results`；Attachment 生命周期回归 |
| 28 | 内容 epoch／子项删除 Undo 不复活 | `resource_authoring`、`expression_entries`、`physics_authoring`、`scene_content`、`handles` |
| 29 | 直接 SDK／规划／Editor／Undo/Redo 发布 | 新增 `authoring_acceptance::planned_and_direct_domain_writes_publish_identical_results` 对照 12 类入口；Editor publication 混合事务与缓存工作量回归 |
| 30 | 旧计划与扩大的影响范围 | 各域 stale plan／锁／基线回归，`expression_entries::available_export_lists_loss_and_rejects_a_stale_acceptance`、资源旧计划回滚及转换旧接受结果拒绝 |

## 本轮补齐

1. 新增 SDK 跨领域验收：一条事务创建所有领域的合法待配置内容，核对实体发布、
   保存往返、单条 Undo/Redo、引用方向与全部旧句柄退休；末尾失败整体回滚并保留 redo；
   12 类规划执行与直接事务调用产生相同模型和 `ChangeSet`，只读规划不发通知。
2. 修复 Editor 混合文档字段／场景名称事务遗漏画布名称，以及混合组织移动遗漏父级缓存。
   画布结构和名称先按权威变化处理，再选投影局部更新路径。标签保持几何共享，
   Physics 的拓扑变化不重建画布几何；新回归覆盖提交与 Undo/Redo 和旧快照不变。
3. 修复可见性 Undo/Redo 的预览代际：Editor 可见性虽不是运行输入，也要刷新 host
   预览并沿用暂停规则；仅锁变化保持运行输入。既有 lifecycle 回归先失败再通过。
4. 观察包从稳定 Sample 重建普通／Scene／Glue 轴、Keyform 坐标及 Blend 表／约束的
   数值缓存，再做完整往返比较。旧实现把合法 Mao 场景误报 `NONFINITE_SCENE`。
   新回归覆盖全部派生字段、缺失样本、非有限缓存、保存重开和相同 scene digest。
5. PSD 测试适配 `ImageAsset.content`；MOC3 成功导出／数值对照测试提供实际 PNG
   资源，真实模型使用 model3 纹理映射；序列化 fixture 只将实际描述符改为项目相对路径。
   缺失／损坏资源仍由原有专用测试检查，不放宽严格导出或捏造纹理内容。
6. 开发脚本原有迟到 PID 测试的固定 2.5 秒延迟与轮询开销竞争，曾错误地未进入超时。
   测试桩改为文件信号，在首轮明确超时后才允许写就绪 PID；生产 `dev.sh` 未改。

## 复现入口

```sh
# Editor：工具、所有 Rust 目标、无界面 UI、原生和真实模型验收
./scripts/test.sh full

# 相邻 kasane-2d：普通工作区门禁，排除 alpha mesh 随机压力余项
./tools/test.sh regression --no-fail-fast
./tools/test.sh python

# 新增专项；总门禁已覆盖这些用例，无需把计数重复相加
cargo test --locked -p kasane-sdk --test integration authoring_acceptance::
cargo test --locked -p kasane-sdk-observe --lib --test capture
```

## 验证结果

| 门禁 | 本轮结果 |
| --- | --- |
| Editor `scripts/test.sh full` | 51 项 Python 工具测试、773 项 Rust 测试通过，5 项按原设置忽略；包含文档测试及所有默认目标 |
| Editor 原生主程序式验收 | Mao／Shirousagi／Heijiao 动画，真实 Metal 双视口截图、Timeline 导入边界／候选取消／单历史／导航／SDK 数值与保存往返均通过 |
| Properties／Physics 专项截图 | 英中表单及面板验收通过，代表画面已目视复核 |
| SDK `tools/test.sh regression --no-fail-fast` | 788 通过，2 项按原设置忽略；脚本排除 alpha mesh 随机压力余项，未将其计为通过 |
| MOC3 最后一次纹理映射修正 | `import_tests model_roundtrip::` 11 通过；与总门禁覆盖重叠，不相加 |
| 当前 Debug Python extension | CPU 72、inspection 4 通过；未将此记为 Release wheel 或 GPU Python 验收 |
| Release 性能复核 | Mao retained 属性链三轮、Mao／Shirousagi SDK 查询／结构及 C4 分配预算、120 请求 Timeline 均完成；Dense 原生拖动延迟等边界单独记录 |
| 两仓格式与空白检查 | `cargo fmt --all -- --check`、`git diff --check` 通过 |

首次总门禁暴露本文记录的遗漏，均修复后重跑；最后 Editor 完整门禁成功。
开发脚本超时测试的竞态在无并发构建时也复现，采用确定性测试信号修复后 51 项通过。
不合并前轮专项计数，不将忽略、跳过的压力余项或未执行的 wheel 验收计入成功。
最终门禁日志保留在本地 `target/c6-acceptance/`；源代码、diff 与模型哈希保留在属性链
测量归档。截图位于 `target/headless-screenshots/`。

UI 交互统一按[UI 方案](unified-object-authoring-ui.md)单独推进；本阶段不声称新增菜单或
控件已上线。性能采样与边界见[测量记录](../benchmarks/2026-10-22-core-authoring-c6.md)。
