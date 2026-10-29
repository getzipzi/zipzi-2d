# Mesh Edit 保留绑定与变形依赖实施方案

状态：已实施。更新日期：2026-10-29。定向验收、性能测量和既有门禁限制见文末“实施记录”。

目标是在当前模型姿态中编辑单个 Mesh，将操作映射回源图／UV（可显式切换源图视图），同时保留父变形器、普通参数绑定、BlendShape 和 Glue。将目前“存在依赖就禁止进入”的策略改为逐操作判定：合法操作更新完整依赖并原子提交；无法推导的外部采样、删除 Glue 端点、锁定依赖或超预算等情况，只阻止对应操作并给出具体原因。

本方案继承[手动网格编辑方案](../archive/plans/manual-mesh-editing.md)的稳定 VertexId、UV 拓扑校验、单次手势提交和唯一文档历史。以下保留本轮实施契约；示意类型与最终 API 的对应关系见实施记录。项目未正式发布，允许直接调整内部接口和生成协议，不引入旧版本迁移层。

## 目标与交付边界

首轮交付必须同时支持四类依赖及其组合，不能只放开入口或只支持没有动画数据的父变形器。

| 项目 | 实施决策 |
| --- | --- |
| 编辑平面 | 默认当前模型姿态；点选／插点／拖动映射回 UV，拓扑合法性仍在 UV 中判断；可切换完整源图视图 |
| 编辑含义 | 调整图片上的采样点与连接，并将其传播到基础形态及全部依赖形态 |
| 父变形器 | 保留 Mesh 的父级引用；所有形态继续存储在原 authoring space |
| 普通绑定 | 保留 binding、轴、sample 身份及非位置通道；同步更新所有位置数组 |
| BlendShape | 保留绑定、共享关键值表和约束；同步更新所有逐顶点增量 |
| Glue | 保留 Glue 对象、pair 身份、端点引用、权重及强度绑定；首轮阻止删除任何被 pair 引用的顶点 |
| 非仿射基础形态 | 可以进入；旧 UV 域内部采用分片仿射采样，不要求全局 AffineUvMap |
| 旧覆盖范围外的点 | 有统一仿射基础映射且无上述依赖时延续现有能力；其他情况首轮拒绝该次操作，后续扩展显式外推策略 |
| 当前姿态 | 作为独立结果预览；不会成为全部关键形的写入基准 |
| 历史 | 一次点击、一次拖动或一次生成应用一条历史；取消手势恢复手势前完整数据 |
| 视觉保证 | 保留依赖数据和明确的重采样语义；任意拓扑变化不承诺所有姿态下逐像素相同 |

保留原有单 Mesh、独立完整图片、单个无孔洞闭合 UV 区域、有限且可辨认的 UV 几何等条件。空网格、孔洞、多区域及多 Mesh 联合拓扑编辑不在本轮范围。

直接改写指定关键形的位置、通过 Glue 联动写入另一 Mesh、自动新增 Glue 配对、完整重建后的自动接缝恢复也不属于首轮交付。普通形态编辑工具继续负责指定关键形的变形操作。

## 当前实现与可复用基础

| 位置 | 当前行为 | 本轮调整 |
| --- | --- | --- |
| [Editor 目标检查](../../crates/zipzi-editor-core/src/document/manual_mesh.rs) | 拒绝父变形器、普通绑定、BlendShape、Glue，并要求全局仿射映射 | 改为 UV 入口检查和按动作能力查询 |
| [SDK 手动编辑](../../crates/zipzi-sdk/src/manual_mesh.rs) | 纯几何操作通过仿射映射生成位置；会话入口拒绝依赖，提交空依赖，space 固定为 Model | 分离 UV 拓扑编辑与形态转移，构造完整替换 |
| [完整拓扑替换](../../crates/zipzi-core/src/document/meshes.rs) | 原子校验并替换 Mesh、普通绑定、全部 BlendShape 和 Glue | 复用最终联合提交，审核锁与变化追踪 |
| [内容编辑重映射](../../crates/zipzi-core/src/document/scene_content/mesh.rs) | 插入、重排、删除同步各形态；插入支持权重来源 | 提取共享形态采样原语，避免复制两套算法 |
| [矩形网格细分](../../crates/zipzi-sdk/src/geometry.rs) | 已保留普通关键形、BlendShape 和角点 Glue | 复用依赖收集思路；其四角采样不能直接处理任意网格 |
| [手动交互](../../crates/zipzi-editor-core/src/tools/manual_mesh.rs) | 命中依赖求值几何，拖动和加点依赖基础位置与仿射映射 | 模型姿态命中后回映 UV；拖动冻结来源并逆算父级和 Glue |
| [参考图绘制](../../crates/zipzi-editor-core/src/scene.rs) | 用基础仿射映射绘制四角参考图 | 为 UV 编辑平面构建独立展示，不混入模型空间 drawable |
| [生成预览和应用](../../crates/zipzi-editor-core/src/mesh_generation.rs) | 手动／Alpha 路径提交空依赖 | 与手动编辑共享同一依赖转移计划 |
| [普通点编辑逆算](../../crates/zipzi-editor-core/src/mesh_edit.rs)和 [ControlSpace](../../crates/zipzi-editor-core/src/control_edit.rs) | 支持形态、Glue 及父级逆算 | 复用 ControlSpace 和 GlueMap 逆算；随后通过 UV 采样同时更新所有存储形态 |

不能先调用插入顶点，再补三角形，再补关键形。已有 `meshing: Some` 的网格在中间状态可能出现无面顶点或失效外围，逐条校验会正确拒绝。必须先构造完整合法结果，再提交一次。

## 模型内编辑与源图视图

进入模式时保留当前 Modeling 视口的完整模型、当前参数姿态和模型相机，在求值后的目标 Mesh 上显示网格和 Glue 端点。点命中、边命中、框选和连线使用同一份已显示的模型几何；不会自动开启 Isolation，也不会自动切换源图。

普通插点先从已显示三角形计算重心坐标和初始 UV，再通过父级逆算修正到当前姿态对应的 UV；新增点没有 Glue pair。边上插点按已显示端点投影获得插值比例，精确保留源 UV 边。非线性父级下，细分后的边可能与原显示弦线略有差异，这不构成整片三角形的外观等价承诺。

拖动在按下时固定参数快照、原始局部位置场、UV 三角形、父级和 Glue 映射。先联合逆算选中点的 Glue 位移，再逆算父变形，最后映射回原始 UV。连续帧始终引用同一来源；采样普通形态和所有 BlendShape 的逻辑不变。变形折叠、重叠导致不同 UV 解、奇异或弱控制权时，拒绝当前增量并提示切换源图，保留上一个有效 candidate。

“在源图上编辑”按钮显式切换到完整源图／UV 视图，两种视图均支持拓扑输入。源图按像素宽高比显示，第一次使用或图像身份／尺寸变化时 fit；模型相机与 UV 相机分别保存。切换先结算当前手势，并保留选点。源图视图只显示目标源图、网格和必要的 Glue 标记；其他对象不会混入源图坐标系。

编辑 candidate 只由一个 Modeling 视口拥有，其他视口保持正常模型展示。进入、退出、切换视图都不改参数默认值、父级或 Glue 强度。源图引用、内容身份、尺寸、目标或依赖发生外部变更时使旧计划失效；动画播放和参数变更仍按既有生命周期结算／退出。

## 形态转移数学契约

### 冻结来源与采样权重

在每个操作或拖动开始时冻结以下来源：旧 UV、三角形、VertexId、基础位置、完整普通绑定、全部 Mesh BlendShape、关联 Glue、父级引用、纹理身份及来源版本。拖动的每次绝对目标从这份来源计算。

对旧覆盖域内部的目标 UV `u`，找到旧 UV 三角形 `a,b,c` 并计算重心权重 `w`。正常内部采样要求权重非负且总和为 1；容差只用于处理浮点舍入，禁止把明显越界点静默夹回边界。

```text
B'(u)   = wa * B[a]   + wb * B[b]   + wc * B[c]
Kj'(u)  = wa * Kj[a]  + wb * Kj[b]  + wc * Kj[c]
Dbs'(u) = wa * Dbs[a] + wb * Dbs[b] + wc * Dbs[c]
```

`B` 为基础位置，`Kj` 为第 j 个普通关键形的绝对局部位置，`Dbs` 为某个 BlendShape sample 的局部增量。三者始终使用同一组权重。边上采样退化为两个端点权重，命中旧顶点时使用身份映射并保持原值。公共边命中采用稳定的端点／三角形身份排序，保证重复执行结果确定。

全部计算使用 f64 中间值，输出 f32 后验证有限值与数据结构。未移动的保留顶点直接复制原值，避免无意义舍入。基础位置可能折叠或重叠；只要旧 UV 域有效就能采样，不对所有动画形态施加 UV 的单闭环或无折叠条件。

### 移动与删除语义

移动顶点表示改变其在图片中的采样位置。保留该顶点的 VertexId，但对它的基础位置、全部普通关键形和全部 BlendShape 重新采样。不能只移动基础位置，不能把当前预览姿态复制到所有关键形，也不能把 BlendShape 增量当作绝对位置。

拖动以起始快照为采样源，以最近有效预览作为运动交叉检查起点。这两个基准必须分开：前者避免累计插值漂移，后者防止稀疏鼠标事件越过固定边。非法增量保留最近有效预览；松开提交屏幕显示的有效结果。

删除顶点时，按最终保留 ID 集合同步裁剪全部形态数组。删除导致的重连会改变纹理插值，这是该操作的正常影响；其他顶点的存储形态值保持原值。删外围边导致的孤立点清理也属于删除顶点，必须进入同一依赖检查。

约束固定、解除或自动重连没有改变顶点与 UV 时，完整保留所有形态数组。移动和删点继续沿用现有规则：保留 User 约束，释放生成的 EdgeSupport，并重建自动连接。

### 父级和非位置数据

转移全部发生在原 authoring space。Mesh 无父级时为 Model，有父级时为 RotationLocal 或 WarpParameter；来源快照必须准确记录该空间。不得将求值后的模型位置写回局部位置，不修改父变形器控制点或其绑定。

普通绑定的轴、sample ID、appearance、draw order 原样保留；所有 Mesh BlendShape 的 key table、sample ID、constraint IDs、opacity、draw order、multiply、screen 原样保留。共享关键值表及绑定到其他对象的 BlendShape 不参与重建。线性采样应保持中性 sample 的全零增量，规范化不得改变该语义。

Motion、Expression、Physics 等继续引用原参数和对象身份，本轮不重建这些资源。依赖收集必须涵盖尚未配置完整、当前未激活或预览权重为零的绑定，不能只迁移当前参与求值的数据。

### 旧覆盖范围之外

将图片矩形与旧 UV 覆盖域作为两个独立边界。所有点都必须位于图片矩形内；带依赖或非仿射基础形态的新增／移动点，首轮还必须能从旧 UV 网格取得采样来源。

无法采样时返回可定位的 `OutsideTransferDomain`，保留最近有效结果，不阻止进入模式或编辑其他区域。完整生成、边缘重建和补点使用相同规则，不能从生成入口绕过此条件。

为无父级、无绑定、无 BlendShape、无 Glue 且存在统一仿射基础映射的网格保留当前外围扩展行为：新基础位置由该映射得到。这个兼容分支也使用完整计划提交，不另开旧写入路径。

后续可增加显式 `ExtendBoundaryDisplacement` 策略：投影到旧边界取得形变位移，再为新 UV 建立基础位置并延伸位移。非仿射基础形态还需要确定外部基础映射；必须同时定义确定性、距离／数值边界和报告范围。本轮不隐式执行外推，不放宽通用 `VertexSeed::Interpolate` 的非负权重契约。

## 操作能力矩阵

| 操作 | 四类依赖存在时的行为 | 操作级拒绝条件 |
| --- | --- | --- |
| 进入、选择、框选、导航 | 可用，UV 展示不受当前姿态折叠影响 | 源图不可用、UV 拓扑不受支持、目标锁定 |
| 固定已有自动边 | 保留全部形态与 Glue；仅修改结构记录 | 边无效、实际修改对象锁定 |
| 新建固定连接、自动重连 | 保留顶点数据及 Glue；重新三角化 | UV 交叉、无有效面；形态外观可能改变但不因存在绑定拒绝 |
| 内部／边上加点 | 同权重生成全部形态；不自动创建 Glue pair | 无可用来源、数值退化、预算不足 |
| 移动普通顶点 | 保留 ID，在新 UV 处重采样全部形态 | 越出图片／来源域、固定边交叉或数值退化 |
| 移动 Glue 端点 | 保留 pair 和权重，重采样本 Mesh 对应顶点；预览显示 Glue 最终效果 | 同上；不要求最终模型点与 UV 指针重合 |
| 删除非 Glue 顶点 | 裁剪全部形态并重连 | 最终没有有效面、出现孔洞／分离区域 |
| 删除 Glue 端点 | 拒绝本次删除，报告具体 Glue／pair／顶点 | 包括部分配置 pair、self Glue、多个 Glue 及删边产生的隐式孤立点删除 |
| 删除外围边 | 保留剩余有效区域，执行隐式删除检查 | 会删除 Glue 端点或产生非法拓扑 |
| 内部补点 | 保留旧 ID、外围、约束和 Glue，只为新点转移形态 | 新点无来源或超预算 |
| 边缘重建、完整生成 | 全部结果在同一计划中重采样 | 结果超出来源域；若存在任何被引用的旧顶点，首轮禁用不保留顶点身份的生成方式 |

错误携带结构化 reason、相关对象和顶点 ID；UI 显示“该顶点连接到某 Glue，请先解除对应连接”等可操作说明。不能把后台英文错误字符串包含关系作为能力查询的唯一机制。

不会因某个操作不可用而关闭模式。外部删除目标、改变源图或破坏进入条件时才暂停输入，并保留退出及通过 Undo 恢复的能力。

## Glue 身份与接缝规则

保留的顶点沿用 VertexId；每个 Glue 的对象 ID、runtime ID、顺序、pair ID、两侧引用、权重、强度和强度绑定保持。单边未配置完整的 pair 也属于持久化引用，不能因为当前不参与运行就忽略。

首轮不自动把新点接到“最近的另一侧顶点”，也不自动插入另一 Mesh 的顶点。现有 pair 是顶点对，不能表达任意三角形上的重心锚点。自动补缝需要新的两 Mesh 联合编辑契约，不能混入单 Mesh 操作。

用户可以使用已有 Glue 编辑能力解除指定配对，再返回 Mesh Edit 删除顶点。该操作沿用现有模式退出和历史规则，不为每次正常网格操作新增确认对话框。

生成器内部序号不具有旧顶点身份。完整生成必须从持久分配器申请新 ID；不能因为生成器输出的整数恰好等于某个旧 ID 就认为 Glue 已被保留。首轮完整生成在存在被引用顶点时给出原因，并提供“内部补点”作为可用操作。

## 计划与原子提交

引入共享的手动编辑计划，示意如下：

```text
ManualMeshCapabilities
  entry_status, operation_status, structured_reasons

ManualMeshPlan
  source_identity, geometry_space, dependency_baseline
  edited_uv_topology, vertex_provenance
  complete_topology_replacement
  transfer_report, cost

VertexProvenance
  Retained(old_id)
  Sampled(old_triangle_ids, weights)
  AffineExtension(mapping_identity)
```

保留编辑纯几何函数作为算法层，但输出以 UV 拓扑及顶点来源为中心。SDK 负责从冻结的文档数据建立形态转移计划、能力报告和预算；Core 负责共享重映射原语与最终文档不变量；Runtime 负责来源、candidate 生命周期和唯一发布；Editor 负责交互、展示和选择。

计划流程固定为：

1. 验证目标、来源和操作参数，收集完整依赖及实际写入集合。
2. 在冻结 UV 域上计算最终拓扑、保留／新增／删除 ID，定位每个新采样点的来源。
3. 检查 Glue 引用、锁、采样范围和成本；在大数组分配前拒绝超预算。
4. 转移基础位置及全部形态，构建完整 Mesh、binding、blend_bindings、glues 和旧到新 ID 映射。
5. 联合验证完整候选，生成可复用的计划结果与转移报告。
6. Runtime 校验计划仍对应当前来源后原子应用；松开手势时提交一次历史。

Editor 不再先调用纯函数计算一遍、再让 Runtime 重算一遍完整转移。具体通过现有 plan 机制扩展还是增加 typed manual plan，应在 P1 落地时选择单一实现，Editor、Rust 和 Python 共用同一权威结果。

### 来源和拒绝语义

同时保存两种身份：冻结采样基线和当前 candidate 发布身份。前者在一次拖动内不变，后者包含 session、edit ID、base version 和 sequence，并随接受的预览递增。下一次指针事件必须基于当前 sequence，但仍从原始基线采样。

来源包含全部被读取的依赖与图片身份，不能只比较 Mesh 顶点数或正式 revision。外部提交、绑定替换、父级改变、图片更换、锁变化、Undo／Redo 和 candidate 取消均应让过期计划失效。失败、取消和净无变化不得清空 redo 或留下新 ID 的错误复用。

当前 Runtime／Edit 的错误处理可能终止 candidate。交互预览的“请求被几何拒绝”和“事务发生致命错误”必须区分：先在隔离计划中完成可失败计算，成功后再替换候选；不得通过捕获已经中止的 edit 错误假装还能继续拖动。SDK 普通事务原有失败语义保持明确，新增交互路径需单独测试。

### 锁与通知

入口只检查目标是否可编辑。计划针对实际变化的 Mesh、绑定及 Glue 检查锁；完整替换中原样携带的依赖不应因为被重新提交而被误认为实际修改。调整 Core 当前全依赖锁检查前，必须以字段等价证明未修改，不能依赖调用方传入的标志。

拓扑、普通绑定、BlendShape 以及受 Glue 影响的其他 Mesh 都必须获得正确的求值与画布失效。未变的参数轴和共享表身份不能失效。Undo／Redo 恢复精确的三角形、UV、形态数组、约束和引用，不重新采样。

## 生成与历史的一致性

内部补点、边缘重建、完整生成和自动连接都进入同一计划构建器，预览与应用使用同一份结果。生成设置、源图、目标或依赖变化后，旧异步结果不能应用。

拖动和生成预览继续保持不同生命周期：保存时按既有契约结算当前有效拖动；生成预览只有点击应用才提交，保存不能隐式应用生成。关闭窗口、失焦、Esc、多视口 capture 转移和延迟 Pointer Up 按现有交互契约取消或结算，不能发布旧手势。

完成的编辑沿用文档历史，不新增“整次网格编辑完成／取消”。选择其他对象、属性编辑及主工具切换仍正常退出模式；视图导航和已支持的历史操作保留模式。

## 视觉保证与转移报告

必须区分数据完整性、存储采样值和最终绘制表面。转移报告至少包含：新增／移动／删除顶点数、处理的形态数、是否重连、是否涉及 Glue 端点、是否存在非仿射父链、是否采用外部采样，以及无法给出全姿态外观保证的原因。

| 情况 | 可承诺的范围 |
| --- | --- |
| 仅固定已有边，三角形不变 | 全部渲染输入保持，只有编辑结构元数据变化 |
| 在旧面内细分，保留原分区；无 Glue 影响且整个父级映射为仿射 | 同权重采样保持原分片仿射位置场，允许浮点误差 |
| 删点、换对角线、移动采样点、一般重连 | 保留依赖和定义好的采样值；最终分片插值可以改变 |
| 非线性 Warp 或 Glue | 存储数据按契约转移；最终显示需要预览和测试，不能从局部采样推出全局等价 |

“父级是 Rotation”不足以证明整个父链仿射：其祖先仍可能有 Warp。严格保证必须基于实际完整映射证明；未证明时使用普通的保留依赖语义。

验收用旧／新结果在相同参数值和相同 UV 样本上比较，避免按顶点数组下标比较不同拓扑。记录模型空间误差、测试参数域及是否经过 Glue。像素预览还需覆盖轮廓、透明区和接缝。只采样关键值或若干中间值时，报告必须写“采样误差”，不得输出未经证明的全参数域误差上界。

一般重连和删除允许造成预期的变形变化，不设一个未定义的全局视觉误差阈值来重新阻止这些操作。严格细分用例必须满足测试容差；一般编辑提供结果预览和可撤销历史。无需每次操作显示技术性诊断或确认框。

## 预算和交互性能

沿用有限的 AuthoringLimits 和 ConversionControl，将全部形态、新旧几何、UV 空间索引、预览 candidate 和历史保留成本纳入估算。当前通用默认预算见 [binding_authoring.rs](../../crates/zipzi-core/src/document/binding_authoring.rs)，本方案不增加 UI 私有的无限制例外。

一次手势建立一次旧 UV 空间索引；每个受影响顶点定位一次来源，所有形态复用权重。采样时间与受影响顶点和存储形态数相关，不枚举所有普通参数与 BlendShape 的笛卡尔组合。完整数组复制和历史成本仍随总顶点数及总形态数增长，必须单独计量。

预览合并高频 pointer move，来源变化时取消旧任务；后台结果携带手势 ID 和 sequence。先明确正确性，再根据计量减少未变数组复制，不能以缓存命中为理由忽略依赖变化。

P0 记录目标硬件、基础数据规模和操作延迟；P5 对同一数据集记录中位数、p95、分配量和计划次数，定位交互卡顿与成倍重算。有限预算超限必须在完整分配前失败；取消后释放预览资源，保持文档与历史不变。虚线投影和显示预算沿用现有规则，命中仍读取完整拓扑。

## 分阶段实施

### P0 契约与基线

- 建立普通绑定、多个 BlendShape、父级链、Glue、自连接及混合依赖的合成夹具。
- 固定 UV 平面方向、宽高比、模型相机保留、源图视图切换及操作矩阵。
- 为新语义添加定向失败用例，保存旧形态和依赖身份作为期望基线。
- 记录现有性能基线、预算、当前 handoff／candidate 失败语义和锁行为。

完成标准：四类依赖与组合均有可复现输入；“不允许进入”和“操作被拒绝”的断言分开，后续各阶段有明确验收对象。

### P1 SDK 形态转移和联合替换

- 拆分 UV 拓扑与位置生成；实现内部／边上采样、稳定顶点来源及局部坐标标记。
- 复用 Core 重映射逻辑处理基础、普通和 BlendShape 数据，构造完整依赖替换。
- 实现操作能力、Glue 端点删除阻止、完整生成身份规则及来源域拒绝。
- 接入预算、取消、结构化诊断和转移报告；按实际写入审核锁。

完成标准：Rust SDK 可对带四类依赖的有效 UV 网格执行合法操作；全部数据原子更新，失败不留半成品。非仿射基础形态的域内编辑通过测试。

### P2 Runtime 和 Python 一致性

- 在现有 Modeling／plan 路径接入共享计划及来源守卫，支持交互冻结基线。
- 明确无效预览请求不会损坏最近有效 candidate；致命来源冲突取消旧手势。
- 更新 ApiShape、BinaryWire、生成模型和 Python 类型，不手改生成文件。
- Local／Connected 复用同一语义、错误、预算、history 和通知行为。

完成标准：Rust、Python Local、Python Connected 对同一夹具产生等价结果；旧 sequence、错误 edit／session、外部提交及超预算均被正确拒绝。

### P3 Editor 模型视图、源图平面与手动操作

- 用 UV 投影接入全部点／边操作、框选、批量拖动及提示。
- 删除四类依赖入口禁令与全局仿射入口要求，接入共享能力查询。
- 建立默认模型内编辑、模型到 UV 的输入回映、独立源图相机及多视口手势所有权。
- 贯通来源变更、手势取消、历史、保存和模式退出；移除双重完整转移计算。

完成标准：真实 UI 能编辑混合依赖 Mesh，实际结果仍经过完整绑定和 Glue 求值；基础数据与显示坐标不会互写。

### P4 自动生成贯通

- 内部补点保留原顶点及 Glue；边缘／完整重建使用新身份和共享重采样计划。
- 生成操作按矩阵显示可用性与具体拒绝原因，预览和应用使用同一份结果。
- 验证无依赖仿射 Mesh 的外围扩展仍可用；其他来源域外操作只拒绝自身。

完成标准：生成路径不再传空依赖，不通过 ID 数值巧合保留 Glue；保存不会隐式应用生成预览。

### P5 联合验收与文档收尾

- 完成下面的矩阵测试、原生截图和保存／重开／导出检查。
- 比较 P0 性能与内存，覆盖密集网格、多形态和连续拖动取消。
- 更新 SDK、Python 能力说明和 Editor 使用说明；旧方案添加后继链接并标明被替代规则。
- 每阶段记录实际命令、匹配数量、通过情况与未覆盖条件，所有交付项完成后才能将状态改为已实施。

P0 → P1 → P2 → P3 → P4 → P5 为实施顺序。外部域位移延伸、Glue 端点删除与解绑定联合操作、两 Mesh 接缝补点、指定关键形的位置直接改写单列后续方案，不作为首轮隐藏依赖。

## 验收矩阵

| 领域 | 必须覆盖的场景和断言 |
| --- | --- |
| 入口 | 普通、BlendShape、父级、Glue、混合依赖、未激活绑定、非仿射基础形态均可进入；缺图、共享图、锁定和非法 UV 拓扑仍报告原因 |
| 坐标 | 非正方形图片、UV 上下方向、不同缩放、相机恢复；Rotation、非线性 Warp、嵌套父链下编辑位置正确 |
| 加点 | 内部、斜边、旧顶点命中；基础／每个普通形态／每个 Blend sample 使用相同权重；中性增量仍为零 |
| 移点 | 单点、批量、跨旧三角形；直接到目标与多帧到相同目标结果一致；非法增量后恢复，motion crossing 不漏检 |
| 删点 | 同步裁剪所有数组；其他顶点身份和值保持；删外围边隐式删除也检查依赖 |
| 约束 | 固定已有边不改形态；重连不丢绑定；旧自动边不会成为移动障碍 |
| Glue | 非端点操作可用；端点移动保留 pair；删除被引用点原子拒绝；self Glue、多个 Glue、部分配置 pair、隐藏／零强度 Glue 都被纳入 |
| 身份 | 非连续 ID、顶点重排、生成 ID 与旧 ID 数值碰撞；表、sample、约束、pair 身份及顺序保持 |
| 生成 | 内部补点保留 Glue；带引用顶点的完整重建拒绝；外部域结果拒绝；旧预览不能应用 |
| 保真 | 仿射父链细分比较 UV 采样位置场；对角线反例和非线性 Warp 反例不得被标记为严格等价 |
| 原子性 | 失败、取消、锁变化、超预算、外部修改、目标删除、历史切换均不留下半更新数据或旧 capture |
| 候选 | 无效预览不终止可继续的交互；冻结来源与 candidate sequence 正确分离；相同目标不累计平滑 |
| 持久化 | 一步 Undo／Redo 精确恢复全部字段；保存重开一致；原生工程保留 meshing；外部格式按其能力保留运行结果 |
| 通知 | 普通／Blend 位置修改通知正确；Glue 另一侧的求值和画布刷新；无关共享表不被重建 |
| 跨端 | Rust、Local、Connected 对等；生成类型、正反向类型测试与成本报告一致 |

合成夹具进入现有 package 测试体系。真实本地授权模型用于补充验收，缺失资产必须明确 skip；不能以真实模型截图代替可重复的数值和引用断言。

## 测试入口与证据基线

从[根 README 测试表](../../README.md#tests)选择最小门禁；新过滤器实施后先确认匹配数量。以下已有入口可作为起点，扩大测试仅针对实际修改范围：

```sh
cargo test --locked -p zipzi-sdk --test integration -- manual_mesh:: mesh_editing:: scene_content::
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --lib mesh_edit::tests
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration mesh_generation::
./tools/test-editor.sh ui manual_mesh
```

Runtime、Python、codegen、项目 codec 和原生截图的具体 target／过滤器，按各 crate README、`tools/README.md` 及测试脚本 `--help` 选择；禁止把零匹配当作通过。SDK 临时实验使用 `./tools/dev-python`，脚本放在临时目录或 `target/python2/scratch/`。

2026-10-29 的可行性基线：SDK `mesh_editing::` 与 `scene_content::` 合计 26 项通过；Editor Core `mesh_edit::tests` 11 项及绑定网格生成集成测试 1 项通过。这些只证明现有基础设施可复用，不是本方案新增能力的交付证据。

## 实施记录（2026-10-29）

实现位于 `codex/mesh-edit-preserve-dependencies`，基于 `2dc75b84` 的独立 worktree。

- P0：共享合成夹具为 `fixtures/mesh-edit-dependencies.rs`，覆盖非连续 ID、非仿射基础形态、Rotation、普通绑定、两个 BlendShape、零强度 Glue 和不完整 self Glue。旧实现对此输入拒绝进入，因此不存在可比较的旧依赖拖动延迟；不将新增能力的测量写成旧版本性能回归比较。
- P1：SDK 使用 `ManualMeshSource`、`UvSampler` 和 `ManualMeshPlan`。每个受影响点计算一次 f64 权重，基础／普通／Blend 增量共用来源；完整替换保留所有其他字段。Core 对实际变化的依赖检查锁，并保持未变 Glue 的 runtime／pair 身份占用参与联合校验。
- P2：Runtime 在隔离计划中处理可拒绝的手动输入，发布仍使用唯一 Edit；冻结采样来源与 candidate sequence 分开。JSON、二进制上传和 Python Local／Connected 均保留可继续的候选。`WriteResult.mesh_transfer` 为生成的类型化转移报告。快照守卫可区分 revision 相同的分支。
- P3：UV 平面按源图像素宽高显示，独立相机和模型相机互不覆盖；输入、命中、框选、连接和 Glue 标记使用同一投影。实际结果关闭拓扑输入，其他 Modeling 视口保持模型上下文。源图 resize 重新 fit，外部修改使旧计划失效。
- P4：手动及 Alpha 生成均保存并应用同一份完整转移计划，不再提交空依赖。内部补点保留旧 ID；重建使用文档持有的持久分配器；引用端点的重建在生成前拒绝。Grid 沿用 SDK 的完整依赖细分路径。
- P5：补充 UV 位置场数值比较、非线性父链报告、锁／预算／取消／净无变化、候选来源、二进制错误恢复、源图 resize、多视口、保存重开及 CMO3 运行结果采样比较。旧方案和 SDK／Python／Editor 使用说明已更新。

实际验证命令与匹配数量：

| 命令／入口 | 结果 |
| --- | --- |
| `cargo test --locked -p zipzi-sdk --test integration -- manual_mesh:: mesh_editing:: scene_content::` | 61 项通过 |
| `cargo test --locked -p zipzi-runtime --test authoring --test attached_upload` | 11 + 4 项通过，含手动二进制拒绝后继续 |
| `env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration mesh_generation::` | 37 项通过 |
| `env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --lib mesh_edit::tests` | 11 项通过 |
| `./tools/test-editor.sh ui manual_mesh` | 原生 UI 15 项及 Editor Core 1 项通过；附带 render target 的零匹配不计作通过证据 |
| `uv run --locked python tools/test_tools.py -p test_python2_codegen.py` | 21 项通过 |
| Python `PythonSdk.test_manual_mesh_preserves_mixed_dependencies_and_invalid_candidate_recovers`、`test_manual_mesh_source_contract_and_existing_large_writes`、`test_manual_mesh_structure_roundtrips_local_connected_binary_history_and_project` | 3 项通过，每项包含 Local／Connected；混合数据保存重开精确一致 |
| Python codegen `generate.py --check` | 40 个生成文件一致 |
| Python 反向类型测试 `tests/type_invalid.py` | 28 个预期诊断逐项一致 |
| `ZIPZI_SCREENSHOT_SCENE=manual-mesh ZIPZI_SCREENSHOT_SAVE=1 ... --test headless_screenshot` | 原生交互场景通过，14 张 Metal 截图及 PNG 无损重读；12–14 为混合依赖 |

完整 Python 类型门禁仍报告 HEAD 中已有的 73 个 `_transport.py`／Windows ctypes 平台诊断。本次以临时目录导出的 HEAD 源码运行同一 pyright 配置，逐项比较文件、行号、规则和消息：无新增诊断。`tests/type_valid.py` 已加入转移报告的正向类型断言。不能把完整 Python shell 门禁记为全绿；运行时、生成文件和反向类型验证分别按上表记录。

性能探针为 `crates/zipzi-sdk/examples/manual_mesh_benchmark.rs`，入口：

```sh
cargo run --locked --release -p zipzi-sdk --example manual_mesh_benchmark
```

Apple M4、16 GiB、macOS，本轮最终测量为 1,089 点、2,048 面、58 个存储形态，
预热后连续 100 个绝对拖动目标，每目标一次计划和一次应用，中位数 23.579 ms、
p95 25.178 ms；每计划累计分配约 19.90 MB、28,186 次分配。该分配量是 allocator
累计请求，不是峰值 RSS。报告的工作集估算为 8,143,474 bytes、历史估算为
3,038,614 bytes；基线和候选的共享内存按保守 payload 计费。测量期间开发机同时
运行定向验证，不作为全硬件帧率承诺；源 UV 索引在一次连续拖动内复用，未枚举参数组合。
取消探针 candidate 后历史保持不变。有限预算与显式取消的失败路径另有定向测试。

合成仿射父链细分用例在相同普通参数与 Blend 权重、相同 UV 样点上比较模型坐标，
采样误差要求小于 `2e-5`；这不是全参数域上界。一般重连和非线性父链不作严格等价标记。
外部 CMO3 验收使用完整配置的图片及 Glue，检查采样姿态的运行结果，容差 `1e-3`；
内部 meshing 的精确保留由原生工程及 Undo／Redo 验证。

SKIP：该 worktree 没有带入 `models/local` 的本地授权模型，真实模型补充验收未运行。
截图与数值测试使用合成夹具，不依赖该资产，也不以截图替代身份／数组断言。
本轮不运行昂贵的完整 SDK／Editor 套件；门禁按实际修改模块选择。

## 模型内编辑迭代（2026-10-29）

根据试用反馈，默认进入独立 UV 平面的行为已改为模型内编辑。上面的交互契约以本次调整为准；初次实施记录中的“实际结果关闭拓扑输入”描述历史版本。新增 `tools/manual_mesh/mapping.rs` 冻结并回映当前姿态，复用 `ControlSpace`／`GlueMap`，SDK／Runtime 转移语义不变。源图按钮显式切换，两种视图都可编辑。

新增数值验收覆盖：普通参数与两个 BlendShape 混合姿态下的插点；有父 Rotation 和非零 Glue 的端点拖动、拒绝后恢复、连续／直接拖动一致和 Undo／Redo；非线性 quad Warp 下插点和拖动；折叠姿态歧义拒绝与源图恢复。已有源图比例、独立相机、resize、生成计划和多视口测试继续保留。

本轮在主分支运行的定向验证：

- `env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration mesh_generation::`：41 项通过。
- `env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --lib mesh_edit::tests`：11 项通过。
- `./tools/test-editor.sh ui manual_mesh`：16 项原生 UI 与 1 项 Core 测试通过，render target 的零匹配不计入。
- `env -u ZIPZI_DEV_SESSION_DIR ZIPZI_SCREENSHOT_SCENE=manual-mesh ZIPZI_SCREENSHOT_SAVE=1 ZIPZI_SCREENSHOT_DIR="$PWD/target/model-mesh-screenshots" cargo test --locked -p zipzi-editor --features test-support --test headless_screenshot`：14 张 Metal 交互截图通过并检查。第 12／13 张保留可见的其他网格作为上下文，第 14 张显式切换源图。
- `git diff --check`：通过。此轮没有修改 SDK／Runtime 或 Python 契约，未重复完整套件。

## 外部行为参考

- [Spine Mesh attachments](https://esotericsoftware.com/spine-meshes)：区分 UV 与变形展示；部分重建操作会清除权重和变形关键帧。本方案独立定义保留依赖的转移语义。
- [Cubism Glue](https://docs.live2d.com/en/cubism-editor-manual/glue/)：官方提示设置 Glue 后修改网格应先解绑，以免形态异常。本方案将端点删除收敛为操作级限制，并保留其他合法编辑。

外部资料用于对照交互与边界，不作为本项目算法正确性或所有动画外观不变的证明。
