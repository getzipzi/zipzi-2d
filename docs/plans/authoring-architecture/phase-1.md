# Authoring 阶段 1 不可变模型与资源基础

2026-11-06 完成，阶段提交为 `300e8a18`。[架构方案与阶段索引](../authoring-architecture.md#实施记录)

- Contract `kernel`：区分 ContentId、SessionKey、Version、DraftId、DraftSource 与
  ResourceKey；版本/候选计数溢出拒绝推进。新值类型尚未进入 Runtime wire 或 Python schema。
- Core `state`：DocumentState / DocumentBuilder 覆盖全部持久领域，提供类型化增删改、
  根替换与顺序修改；OwnedDocument 承接完整 owned 输入，经规范化和验证后冻结。
  目录、记录、顺序、元数据根和参数 runtime ID 索引分别共享；无净变化复用原 ContentId，
  写失败后不能冻结候选。场景几何和绑定目前采用阶段一允许的记录级共享表示。
- Core `model_validation`：抽出只借用持久记录的共享验证与规范化规则，由 Builder 和只读模型使用。冻结检查跨领域身份、引用、几何、组织和全部业务领域；
  新路径不构造旧 Document，也不依赖旧事务、保存或发布状态。
- WriteSet：由触及记录和根产生净变化、顺序变化和固定步骤成本估计；单步内去重共享块，
  未变曲线不重复计入，改变的粗粒度几何块保守计入两端。估计不承担资源准入或逐分配计费。
- Project `resource_store`：复用现有 PreparedImageContent 与图片预算，ResourceStore 接管
  图片，ResourceView 持有不可变版本并按模型引用检查存在性与尺寸。外部文件先捕获为自有
  内容；资源持有者可跨 Store 生命周期读取，同一图片对象的不同版本可独立存活。

第一阶段已可独立构造、修改、冻结和读取完整持久模型。第二阶段接通原生文件解码/保存及
SDK 根入口，正式状态改由新 AuthoringSession 直接持有 Snapshot。
细粒度几何通道分离及各领域 authoring 算法迁入仍按后续阶段推进。
领域与格式转换随后迁入，宿主和客户端在第四阶段切换，遗留测试与示例在第五阶段清理。

## 持久字段归属

范围覆盖旧 DocumentContent 的全部字段。

| 字段组 | 新归属 | 当前状态 |
| --- | --- | --- |
| id、export_settings、source_canvas | DocumentState 的文档身份与 ModelSetup | 已实现 |
| assets、asset_order | 共享图片记录/顺序；已解析内容只引用 ResourceKey | 已实现 |
| parameters、parameter_order | 共享参数记录/顺序及匹配的 runtime ID 索引 | 已实现，冻结检查绑定依赖 |
| parts/transforms/meshes/glues/offscreens 及各自 order | 场景共享记录与顺序根，记录内暂保留 owned 几何 | 已实现粗粒度表示 |
| bindings/scene_bindings/blend_key_tables/blend_constraints/blend_bindings 及各自 order | 绑定共享记录与顺序根，记录内暂保留 owned 通道 | 已实现粗粒度表示 |
| draw_order_groups、display_info、hierarchy_order、editor_state | 各自独立共享根，保留持久偏好 | 已实现 |
| expressions/expression_order、motions/motion_order、motion_groups、motion_roots | 业务记录及已有曲线/目录共享根 | 已实现 |
| pose、physics | 独立共享领域块 | 已实现 |
| missing_attachments、model3_settings、package_attachments | 附件与来源/opaque 元数据共享根 | 已实现 |

新增行为覆盖完整领域构造与共享、失败隔离、依赖变更后的重验证、顺序/根净变化、
曲线成本不随未变数据增长，以及共享附件去重、保活和回收。

## 本地验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --locked -p zipzi-core` | 单元、集成和文档测试通过；单元测试 79 项，含新状态行为 20 项 |
| `cargo test --locked -p zipzi-project --lib` | 66 项通过，含新资源行为 8 项 |
| `cargo test --locked -p zipzi-contract` | 18 项通过 |
| `uv run --locked python tools/test_tools.py -p test_python_codegen.py` | 64 项通过 |
| `./tools/test-sdk.sh fast` | 单元 34 项通过；集成 506 项通过、5 项失败、2 项忽略 |
| workspace package-domain、所改 Rust 文件格式、`git diff --check` | 通过 |

SDK 的 5 项失败均在独立的 `0262b010` 基线 checkout 原样复现，涉及
`image_memory::captured_disk_image_pins_encoded_bytes_and_pixels_after_source_is_replaced`、
`motion_library::roots_groups_and_pending_content_roundtrip_with_stable_ids`、
`physics_authoring::reopened_field_address_survives_first_reorder_and_is_retired_by_reload`、
`resource_authoring::empty_images_are_native_resources_and_save_without_fake_pixels` 和
`scene_content::deformer_original::native_save_reopen_duplicate_and_history_retain_independent_canvas_originals`。
这些旧测试按目录工程访问当前 ZIP 工程，报文件缺失、NotADirectory 或 UNSUPPORTED_PROJECT_PATH；
本批次未修改这些既有失败。未执行 Runtime/Editor 全链路或 GPU/Windows 验证。
