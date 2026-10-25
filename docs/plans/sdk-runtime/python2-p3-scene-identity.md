# Python P3：场景、拓扑、BlendShape 与 SDK 身份

日期：2026-10-25。状态：**P3 已开始，首批完整调用链通过验收；Runtime R4 仍在实施，尚未满足全部退出门槛。**

基线为同仓 `e8c2849ea6b90dc9164bde18750c2a73b69eaa0a`，开始时工作区干净。
SDK/Runtime/Python2/Editor 使用本次工作区差异；未改依赖或 Cargo.lock。
锁文件 SHA-256 为 `55701976f5ed5fa228d0a91fc2db7024ad2dd7c4d2ceb808e0eb7a2650c0a278`。
本批不读取、搬回或修改旧 Python 包。共享源码清单新增 `--shared-only`，保留历史
Python 索引值，更新共享 Rust 方法/意图/值类型；独立测试禁止它读取旧包路径。

## 交付范围

- SDK 新增选定 `SceneOperation`，将 28 个类型化操作映射到现有 scene、organization、
  content、binding 意图。Part/参数组、Warp/Rotation、Glue/Offscreen、移动/删除、
  本地坐标 reparent、网格 BlendShape、顶点/三角形、Glue pair、Offscreen 关键形及
  静态映射进入同一候选批次。默认值、UUID 分配、规划、转换、数值规则均在 Rust。
- 从实际 Rust 声明导出 9 个场景记录和操作联合，生成 Python dataclass、编码/表达检查
  与 PyO3 字段转换。新增 i32、有限长度 f32 数组、嵌套三角形和记录数组；固定数组
  长度/整数表示由生成器检查，领域合法性仍由 SDK 判定。生成产物由 18 个增至 20 个。
- `session/edit.scene.get/get_many` 返回带确切 Source 的不可变元数据；不返回完整工程。
  最多 256 个选择，以借用字段计算字符串转义上界，先检查 1 MiB 响应和宿主剩余
  CPU 预算，再复制。重复选择也计入总额；查询失败不污染 Edit。
- `scene.plan(operation)` 复用正式/候选来源绑定的 Runtime Plan，返回成本、损失、
  affected/copied IDs；执行复用 `ops.ApplyPlan`。原有 Plan 只能放在批次开头的规则不变。
  `ReparentLocal` 明确选择 SDK `KeepLocalData`；其他 preservation 策略未在本批发布。
- `session.reference(kind, id)` 捕获 SDK incarnation；`resolve` 返回当前正式来源。
  `scene_capabilities` 读取实际 SDK 场景能力。`reference.label()` 捕获 SDK 标签地址，
  包含实体、owner/context/field epochs；地址以来源绑定 Plan 应用修改。
- ObjectRef/LabelAddress 分别拥有创建、读取、规划、释放请求的恢复对象，支持 close/
  context。地址独立持有 SDK 身份，关闭其父 Ref 不破坏它。每客户端两表合计最多
  1024 项，保守字节加入宿主 CPU 预算；显式释放、客户端到期和会话关闭清理表项。
- 原 SDK `edit_scene` 不因新返回值而复制 affected IDs；只有新入口需要结果时提取。
  共享规划、变化记录、身份跟踪和控制检查仍只执行一次。

本地转换不经 JSON/Value 树，所有输入在释放 GIL 前取得所有权；连接端进入相同
`apply_authoring`/SDK 处理器。原生契约为 `zipzi-python2-8`；IPC 保持
`zipzi-runtime-r4-1`，增加并要求 `typed_scene_authoring`、`typed_object_references`
能力。旧宿主须重建，否则在创作前因能力不匹配失败。

## 行为证据

| 验收范围 | 已观察结果 | 证据 |
| --- | --- | --- |
| A04 / A12 | 场景、参数、网格、Glue/Offscreen 在同一 Edit 组合，候选读到前序操作；一个正式历史步骤；无效父级/三角形失败无部分发布 | Python2 两后端 `test_scene_organization_plan_and_metadata_recipe`、`test_scene_failure_and_query_admission_preserve_formal_history` |
| A05 | 移动保持 Ref；删除再 Undo 不复活 Ref；修改、移动及 Undo 不复活旧地址；重新查找可捕获新 Ref | `test_sdk_references_addresses_and_undo_invalidation`、Runtime `tests/references.rs` |
| A07 | Ref/地址/Plan 创建及 Ref 释放丢响应，原 owner 按原请求恢复；最终恰好两步历史 | `test_reference_creation_and_label_plan_response_recovery` |
| A15 | 普通绑定与 mesh BlendShape 叠加；在已有绑定上插入插值顶点、替换三角形、重排顶点和 local reparent；候选/正式 sweep 不改来源、版本、历史 | `test_blendshape_bound_topology_and_candidate_cpu_sweep` |
| A10 / A15 | 安装纯客户端向实际 Editor 发布 Part/参数组/组织关系；Part tree、参数组和属性 revision 更新；人工 Preview 的值/时钟 revision 与历史不被正式/候选 CPU 采样改变；UI Undo 与外部事件连续，Metal 捕获成功 | `crates/zipzi-editor/acceptance/runtime_ipc.rs` |
| A05 / A17 | foreign client 拒绝；引用与地址计费；超额捕获保留正式状态；100 轮捕获/释放回到条目/字节基线；客户端到期清理；Ref 关闭后地址仍可规划 | Runtime `tests/references.rs` |
| 分发 | 两份 sdist 提取后生成检查；重建纯/native wheel；无 native 环境 Connected 与有 native 环境 Local 使用同一安装 recipe，含场景/BlendShape/带绑定拓扑/Ref/地址、保存和新宿主重开 | `tools/distribution.py`、`tests/installed_recipe.py` |

数值 fixture 使用顶点 ID `42/7/99`，在 `42/7` 中点插入 `50` 并把它排在首位。
普通参数产生 `0..10` 的 X 位移，BlendShape 产生 `0..2` 的 X 增量，参数共同取
`0/.5/1` 时首顶点 X 为 `.5/6.5/12.5`。两后端得到相同几何与诊断；固定 fixture
数值可精确表示，Editor 既有 fixture 沿用 `1e-5` 数值容差。

## 执行的聚焦门禁

- `./tools/test-sdk.sh python2`：57 项行为通过；正例零类型错误，16 条实际类型反例匹配。
  收尾消除 SDK 原入口多余 ID 复制后，再跑本批 5 项行为及完整类型检查通过。
- `cargo test --locked -p zipzi-runtime --test references --test authoring --test modeling --test protocol`：
  26 项通过（2 / 7 / 7 / 10），无忽略。
- `cargo test --locked -p zipzi-sdk --test integration scene_`：59 项通过，无忽略；
  覆盖原 scene/组织/content/lifecycle 行为和标签修改工作量回归。
- `uv run --locked python tools/test_tools.py -p test_python2_codegen.py -v`：9 项通过，含
  确定性、检查不写入、保护手写文件、真实 Rust 编译反例、嵌套数组与场景形状拒绝。
- `uv run --locked python tools/test_tools.py -p test_runtime_shared_inventory.py -v`：1 项通过。
  `runtime_inventory.py --shared-only --check`、生成漂移及 diff whitespace 检查通过。
- `./tools/test-sdk.sh python2-connected --install-only` 后运行
  `env -u ZIPZI_DEV_SESSION_DIR ZIPZI_CLIENT_PYTHON=<isolated-python> cargo test --locked -p zipzi-editor --features test-support --test runtime_ipc_acceptance`：
  实际原生 Editor/Metal 验收退出码 0。
- `tools/distribution.py --output target/python2/p3-final-source --cargo-target target/python2/source-cargo-target`：
  配对 source/wheel 与两种隔离安装 recipe；命令、产物哈希及结果在对应目录。

验收结果保留在本节。
初轮 Offscreen 测试遗漏显式静态映射，曾读到中性 opacity；修正为插入、修改关键形并
显式设置 Part 静态映射后通过。初轮 Rust 测试的 Box 字符串比较及 Python Kind 表达
已修复。上述失败日志保留，不将其混入最终通过计数。

## R4 仍未完成的退出项

本批不能标记完整 R4 已完成，也不标记 P3 整体退出：

1. Scene binding、完整 grid/range/reparent 转换、全部集合/CDI、引用和文档意图仍需
   类型化接线和完整同 recipe；mesh BlendShape 不等于所有 BlendShape target 已交付。
2. Ref 当前捕获正式对象；地址当前仅 scene label。候选引用、其他字段/内容/集合地址、
   各领域能力和项目替换 generation 的公共连接验收仍缺失。
3. 独立二进制 blob、大资源/附件、磁盘内容固定、完整导入/替换/已有格式导出与解码峰值
   准入尚未补齐。1 MiB PreparedImage 试点不能替代这些门槛。
4. 昂贵准备/规划/采样的可取消后台 Job、全部队列/结果/临时来源总预算、公平调度仍缺失。
5. 本轮未重跑 P0 成对计时/heap/copy、完整 R2 交互性能、R4 大网格/限额/RSS/缓存矩阵；
   引用表字节测试不是整个宿主 RSS 测量。未跑 SDK/Editor full 或授权真实模型总验收。
6. 原生 Editor 已验收本批 Part/group 等投影；全部选择/关系/纹理/属性表单的完整外部写
   矩阵，以及无窗口素材导入→建模→保存重开→格式导出闭环仍待完成。

逐能力状态继续以 [python2-coverage.json](../../runtime/python2-coverage.json) 为准；
R4 完成需逐项消除以上缺口并达到主计划 A04/A05/A09/A10/A12/A14/A15/A17 门槛。
