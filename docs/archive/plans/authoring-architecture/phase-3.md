# Authoring 阶段 3 领域算法迁入

2026-11-06 完成，阶段提交为 `ed8de895`。[架构方案与阶段索引](../authoring-architecture.md#实施记录)

- 参数、普通绑定、BlendShape、网格/范围转换、场景/组织、换父级与分组、依赖删除、
  Mesh 拓扑、Motion/Expression/Pose/Physics、资源/附件、文档字段与 EditorState 全部
  通过 Builder 的记录/根写入口。只读变换返回受影响值；发布不扫描全领域重建差异。
- 领域锁覆盖所有者、子对象及稳定采样身份；移动保留身份/偏好，复制重建自有身份，
  删除清理孤立元数据。typed patch 检查预期值，任何普通命令错误中止整个 Draft，
  包括错误被调用者忽略的情况。失败、取消和无净变化保持正式状态与 redo。
- 普通/BlendShape 插值、网格重建、range mapping、reparent/grouping 与 manual mesh
  通道转移使用现有数值算法。转换额度仅描述采样组合与实际临时/输出载荷，独立于历史。
  Motion 曲线插点、同时移动、删除、插值、Bezier 手柄及时间缩放也已接入 Draft；
  SDK 私有曲线工作区只持有 clip 值，结果经 Builder 安装，不保留第二套提交协议。
- 冻结模型提供领域投影、引用/删除分析和求值；预览与动画按 ContentId 失效。
  复用原始形状捕获和骨骼显示规则，导入数值轴在初始化前完成稳定身份规范化。
- MOC3、CMO3/CAN3、PSD 和 sidecar 适配器直接读写 DocumentState/Builder；ProjectImporter
  返回冻结模型、捕获的 ResourceView 和来源保护路径。导入不修改正式 Session。
  原生工程编解码、ZIP 验证、预算、目标指纹、staging/原子发布及失败语义继续复用。
- D1–D6 已删除：Core 可变 DocumentContent/Document、transactions、checkpoint、
  TrackingState/allowlist、publication 全量比较及 SDK changes；Project DocumentSession /
  DocumentStore 活文档所有权及其旧保存路径已移除。document 模块仅保留值类型与纯规则。
  D12 的 Core 通用 retained/history 倍率准入已移除，保留实际大分配与图片/ZIP 限制。

## 最终验证

| 检查 | 结果 |
| --- | --- |
| `./tools/test-sdk.sh fast` | 单元 66 项、authoring 集成 28 项；包含领域锁、失败隔离、历史、来源、资源版本、原生保存及三个入库工程 |
| 八个受影响库 `cargo test --locked -p … --lib` | Core 47、Project 69、SDK 66、Animation 6、CMO3 8、MOC3 2、PSD 5、MOC3-PSD 3 项 |
| Core `--test preview_state_tests` | 4 项；共享帧、拒绝保留、内容/代际失效与数值失败缓存 |
| SDK `--test alpha_mesh` / `--doc` | 39 项 / 保存排他借用 compile-fail 1 项 |
| Project `--test authoring_formats` | 4 项；model3/CMO3/PSD 捕获与几何往返、所有动画 sidecar 的原生往返及稳定 key 身份 |
| CMO3 `--test rotation_units` | 1 项，涵盖混合 Rotation/Warp、两种坐标方向、普通/BlendShape 多姿态与两次往返 |
| PSD `--tests` | 单元 5、导入 14、遮罩 15、macOS Metal 裁剪 6 项；1 项私有 PSD 素材测试按既有标记忽略 |
| MOC3-PSD `--test export_tests` | 2 项，包含真实入库模型的图层、隐藏图像及合成预览 |
| 所改 crate Rust 格式、package-domain、`git diff --check` | 通过 |

迁移边界：上述 focused gates 已通过；不宣称整个 workspace 或旧聚合套件通过。
Observe、Runtime、Editor、Python native/生成客户端仍消费已移除接口，第四阶段接线；
旧 SDK/Core/Project/格式集成 targets、示例及 archive prototype 中尚未迁移的源保留为
阶段四、五的行为迁移/删除输入，不能通过旧签名编译，也不是兼容执行路径。
本阶段迁入了对应领域数值和失败断言；旧 checkpoint/事务内部保证的测试随机制删除，
新 snapshot/WriteSet/Draft/资源与发布测试覆盖替代契约。未执行 Windows 与完整桌面验收。
本地 Metal 验证仅指上面的 PSD 裁剪六项，不代表 Editor/Observe 全链路完成。
