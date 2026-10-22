# C5 完整领域覆盖核对

2026 年 10 月 22 日。C5 覆盖表的 **7 项均已落地**：前五批完成对象、组织、内容和引用集合，资源操作与覆盖核对在本轮一并收尾。对应[核心方案](unified-object-authoring-core.md)第 4、5、9.2 节；实际接口和验证命令见[实施记录](unified-object-authoring-core-implementation.md)。

本表核对原生模型、core／SDK 规则、Editor 必要适配和 Python 入口。C6 负责核心总验收、性能与预算复核、公共契约定稿；之后按 UI 方案统一菜单、拖拽、快捷键和表单交互。

## 对象和操作

“生命周期”包括创建、适用字段／内容编辑、复制、删除、锁、持久化及格式校验。“集合”包括原生顺序、稳定锚点、合法归属移动与对应的解散策略。下表逐项列出实际入口和领域限制，未将当前条件不满足改写为类型不支持。

| 对象／内容 | 规则入口与已覆盖能力 | 领域边界与验证位置 |
| --- | --- | --- |
| Document | `AuthoringSession::new`；`DocumentAuthoringEdit`、`DocumentFieldAddress`；Layout 和全部 ExportSettings 的类型化补丁 | 文档关闭和标签顺序属于工作区。SourceCanvas 是来源信息，通过读取接口检查；通用字段补丁返回 `SOURCE_METADATA_READ_ONLY`。导入边界保留显式 `replace_source_canvas`。`resource_authoring`、`project_io`、Editor `properties` |
| Parameter、ParameterGroup、Part | `OrganizationEdit`／`OrganizationAddress`；生命周期、嵌套归属、同级顺序、复制闭包、删除和解散 | Part 解散校验绘制保持；有引用的目标默认拒删。未解析 CDI 行通过显式修复处理。`organization_authoring`、`cdi_authoring`、`parameter_management` |
| Mesh、Warp、Rotation、Offscreen、Glue、DrawOrderGroup | `SceneEdit`／`SceneFieldAddress`／`DrawingGroupAddress`；生命周期及三种关系的集合操作 | Part 归属、变形父级和绘制归属分别规划；改父级保持矩阵沿用 C4；Offscreen 跨 Part 使用联合映射转换。`scene_lifecycle`、`binding_authoring` |
| Mesh VertexId、位置、UV、三角形 | `SceneContentEdit`／`SceneContentAddress`；插入、修改、重排、删除及依赖拓扑转换 | 顶点身份与三角形端点保持对应；基础／普通／Blend 形态联合变更。UV 属于基础 Mesh。`scene_content`、`mesh_editing`、core `keyform_tests` |
| Warp lattice、Bezier anchor／handle | 同一内容入口；位置修改、控制器编辑、网格重采样 | 控制器残差保留；改变网格的保证与损失报告明确。`scene_content`、core `transform_tests` |
| GlueVertexPair | PairId、独立句柄、创建／配置／重排／复制／删除及显式端点转移 | 未指定顶点不参与执行；原始槽位映射保留。`scene_content`、core `blendshape_glue_tests` |
| Offscreen Keyform 与 Part 映射 | 专用增删、复制、排列和映射命令 | 使用 Part sample ID 地址；删除被引用形态必须选择 neutral／remap 策略。`scene_content`、`effects_authoring`、core `offscreen_tests` |
| MotionClip、MotionGroup、注册 | `MotionEdit`／注册地址；Clip 与注册分离、生命周期、空组、集合移动和解散 | 同 Clip 多注册；移除最后注册时保留可见入口；管理操作保持活动预览。`motion_library`、Editor `animation` |
| MotionTrack、MotionKey、MotionEvent、Bezier 柄 | 类型化轨道转移／展示顺序、`MotionCurveEdit`、Clip candidate | 时间顺序和展示顺序分开；控制柄通过关键帧身份和角色定位。`motion_library`、既有 Timeline／曲线回归 |
| Expression、ExpressionEntry | `ExpressionEdit`、Entry address／candidate；生命周期与条目集合 | 未指定和未解析目标分别保存；跨 Expression 转移校验目标，复制拥有条目而共享 Parameter。C1 记录、`expression_editing` |
| Physics、Group、Input、Output、Node | `PhysicsEdit`、稳定条目／节点地址、原生投影和 physics3 转换 | 根资源是单例；节点是拓扑，输出按 NodeId 引用，非法链不能当草稿跳过。C3 记录、`physics_editing` |
| Pose、Group、Entry、Link | `PoseEdit`、稳定身份、生命周期和集合操作 | 主 Part 缺失禁用整项，Link 缺失只禁用该 Link；解散显式指定接收组。`pose_authoring`、project `pose_tests` |
| Mesh／Scene／Glue Binding、Axis、Sample、Keyform | `BindingAuthoringEdit`；零轴、待配置轴、网格修改与转换、参数范围联合修改 | 必须有目标拥有者；形态使用 sample ID 组合，不是普通列表。容量、取消及保持策略见 C4 已验收支持矩阵。`binding_authoring` |
| BlendBinding、KeyTable、Constraint、DeltaKeyform | 同一 binding authoring 入口；待配置驱动、共享表／约束联合转换、基准点重基准化与各类 delta | 约束不可用时禁用使用者，不作为无约束执行；更换共享表必须覆盖全部使用者。`binding_authoring`、`effects_authoring` |
| ModelParameterGroup、ModelHitArea、CdiCombinedSet 及成员 | `ReferenceEdit`／`ReferenceFieldAddress`；稳定根／成员 UUID、待配置、生命周期、集合和显式解散 | CombinedSet 没有虚构名称；共享目标不随成员删除；重复业务目标整体拒绝。`reference_authoring` |
| Mesh masks、Offscreen masks、constraint_ids | `ReplaceSelection`／`ReferenceSelectionAddress`；整组原子选择、顺序及方向明确的 ChangeSet | 批量选择字段没有人为行 UUID；目标和拥有上下文变化使地址过期。缺失遮罩沿依赖传播，禁止无掩码回落。`reference_authoring`、`scene_lifecycle`、`resource_authoring` |
| Texture／ImageAsset | `ResourceEdit`／`ResourceNameAddress`；无内容创建、改名、内容设置／清空、复制、排序、依赖删除与显式替换 | `content: Option<ImageContent>`；有内容时才保存尺寸／哈希。导入不可用来源另存 `unresolved_source`，可修复且不改变 ID。普通删除列出使用者；替换引用校验每个使用者的锁。`resource_authoring`、project resource tests、CMO3／MOC3 定向回归 |
| Attachment、sound、UserData | `AttachmentEdit`、`AttachmentPathAddress`、`AttachmentLoadToken`；空内容创建、路径编辑、内容替换／清空、复制、排序、删除及替换引用 | 空路径创建生成唯一默认导出路径；`None` 和零字节文件不同。复制共享不可变 bytes，后续替换独立；声音和 UserData 关联 Attachment ID。C2、`resource_authoring`、project Motion／attachment tests |

## 异步、引用修复与来源边界

- `image_load_token`／`attachment_load_token` 绑定 SDK 会话、generation、资源 incarnation、单调内容 epoch 及可取消的请求。新请求取消同资源旧请求；改名和附件路径变化保持内容请求有效，内容修改／Undo／删除重建／文档替换使旧请求失效。错误类型请求先验证句柄，不取消合法请求。
- `prepare_image_load` 在会话锁之外读取、验证、解码和哈希 PNG，返回不可变 `PreparedImageContent`。`replace_prepared_image_content` 成功后交接 Arc，提交阶段使用已准备的像素。取消、过期、锁或预算失败不注册新内容；发布前再次检查取消。图像源文件在准备后删除仍可提交、保存和观察。普通描述符导入／替换入口继续可用。
- 准备图像采用已有 RGBA 资源边界：编码数据最多 64 MiB，宽高不超过 16,384，RGBA 最多 256 MiB。历史与只读快照可共同持有不可变内容；这些上限不表示进程 RSS 上限。
- `AuthoringSnapshot` 和 `ObservationInput` 持有相应的内存图像引用；脱离原会话后仍可解析纹理。保存转为真实项目文件。观察包采用 schema 6，校验空内容、尺寸、哈希与容量，损坏描述符返回诊断。
- Expression、Motion、Physics、Pose、Binding、场景和行为成员的引用修复使用各自字段／网格／内容规划入口。地址按其实体、拥有者和语义上下文校验；不能拿压缩执行槽位修改原生数据。
- `DocumentAuthoringEdit::RepairMissingAttachment` 将已知 sound／UserData 的未解析路径显式绑定到 Attachment ID，或显式移除；检查受影响注册的锁。无法解析的必需外部结构仍在导入边界报错，由修复来源文件后重新导入处理。
- `RepairCdiParameter`／`RepairCdiPart` 将未解析 CDI 行绑定到已有目标，或显式移除。保留可保留的 group／extensions；显示名称改用目标的权威名称时，计划报告损失。目标 UUID 保持不变，冲突整体拒绝。
- 不透明扩展、来源 namespace、SourceCanvas 保留独立的读取与导入边界。通用 Properties 不提供任意 JSON 修改；严格导出继续使用既有来源基线检查。未配置内容不会用随机目标 ID、伪造纹理尺寸或隐式解除引用代替。

## 发布与 Editor 状态

`ChangeSet` 记录资源集合顺序、资源内容身份、`document_fields`、实体及引用变化。普通命令、裸 setter、导入和 Undo/Redo 共用原生校验与 SDK 历史。资源删除入口委托同一资源规划，持久化锁覆盖 ImageAsset 和 Attachment。

Layout、导出设置、来源信息、附件内容／路径、资源名称与顺序按其实际影响发布。参数组改名继续复用原关系投影；文档内嵌属性同步刷新。纹理内容缺失保留几何，并沿 masks／Offscreen 依赖禁用绘制；Glue、HitArea 按几何用途独立判断。纹理快照跳过无内容记录。

Editor 附加活动 Document、候选手势和 Timeline 条件；异步完成在另一活动文档中被拒绝。展开、滚动、搜索、选择、预览属于会话；持久化隐藏与锁定继续使用单独的 EditorState。操作结果的建议选择不进入模型历史，也不自动激活另一个动画。

## 收尾验证

本轮新增 SDK 资源／文档回归、Editor 资源／文档回归、Python 资源回归，以及缺失纹理、严格导出、内存图像观察的定向用例。补齐基准目标和 MOC3 到 PSD 的旧引用／图像字段调用；真实图层导出回归通过。最终计数和命令见实施记录末节。原生项目为目录格式 **20**、core schema **11**，按未发布项目约定拒绝旧格式，不提供迁移。

本核对关闭 C5 的资源与全领域覆盖两项。后续 C6 核心总验收与真实模型性能复核已完成，见 [C6 交付记录](unified-object-authoring-c6-acceptance.md)；全面 UI 统一尚未开始。
