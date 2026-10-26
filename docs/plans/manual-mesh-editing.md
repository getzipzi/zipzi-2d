# Manual Mesh 编辑实施方案

状态：实施中，P0 数据与契约已完成并通过验收；P1–P5 待实施。更新日期：2026-10-26。

在 Modeling 主画布中提供单 Mesh 手动网格编辑。用户调整布点和结构约束，系统维持有效三角化；每次操作通过 Runtime 提交一条文档历史。模型保存外轮廓、需要保留的结构边和实际三角形，为后续将自动生成拆成边缘布局、内部布点和自动连线提供共同基础。

本方案采用 Spine 的轮廓与约束交互方向，并明确 Zipzi 首版的能力限制。本文中的算法、数据格式、事务和错误策略是 Zipzi 的实施约定，不宣称逐项复刻 Spine。

## 范围与固定决策

| 项目 | 首版约定 |
| --- | --- |
| 编辑对象 | 一个可编辑 Mesh，一个无自交的闭合外围，无孔洞、无分离区域 |
| 有效状态 | 已提交 Mesh 始终具有非退化三角形，所有顶点和结构边都参与面 |
| 提交方式 | 一次点击加点、一次连线、一次删除、一次拖动或一次生成各提交一步；自动连接属于该操作 |
| 草稿 | 只有单次手势或生成预览的 candidate，没有整次模式草稿或独立历史 |
| 退出 | 保留已完成修改；不提供整次完成／取消 |
| 纹理语义 | 在固定源图映射下联动基础位置和 UV，改变网格覆盖范围，不拖动参考图片 |
| 首版输入 | 独立完整源图、可逆仿射映射、无普通形状绑定、无 BlendShape、无 Glue、无父变形器 |
| 数据范围 | 手动编辑限制不升级为所有 Document Mesh 的全局限制 |
| 暂缓 | 空网格重建、挖洞、删面、多区域手动编辑、Atlas 区域恢复、复杂绑定、合并、镜像、笔刷、缩放旋转工具 |
| 格式策略 | 允许破坏性修改，不实现旧项目或协议迁移 |

首版选择支持单选、多选、框选和批量拖动。套索沿用现有能力可后续接入，不作为首版阻塞项。创建工具使用点击加点、拖动连线，不实现连续布点链。

## 现有代码与改动边界

| 位置 | 已有能力与计划改动 |
| --- | --- |
| [Core Mesh](../../crates/zipzi-core/src/types.rs) | 稳定 VertexId、基础位置、UV 和三角形；增加可选建模拓扑 |
| [Core 几何校验](../../crates/zipzi-core/src/geometry.rs) | 基本渲染合法性，允许空三角形；新增独立的建模拓扑校验 |
| [Mesh 文档操作](../../crates/zipzi-core/src/document/meshes.rs) | 创建、替换、原子拓扑替换；统一校验新增数据 |
| [内容编辑](../../crates/zipzi-core/src/document/scene_content/mesh.rs) | 顶点及三角形增删、形状数组重映射；审计所有几何写入路径 |
| [SDK 几何](../../crates/zipzi-sdk/src/geometry.rs) | MeshGeometry、网格生成与依赖保留；新增纯拓扑操作和完整输出 |
| [Alpha 生成](../../crates/zipzi-sdk/src/alpha_mesh/mod.rs) | domain 构建后进入 triangulate；拆出边缘布局、内部采样、约束三角化 |
| [Project codec](../../crates/zipzi-project/src/codec/types.rs) | 独立 MeshWire；同步 JSON 与 CBOR 保存结构 |
| [SDK 内容变化](../../crates/zipzi-sdk/src/changes/content.rs) | 逐字段生成 MeshTopology 变化；增加结构边和外围变化 |
| [Runtime 网格读取](../../crates/zipzi-runtime/src/runtime/mesh_read.rs) | 手写 borrowed 输出及布局断言；同步类型、预算及快照 |
| [画布拓扑缓存](../../crates/zipzi-editor-core/src/canvas_topology.rs) | 已缓存顶点、三角形、去重边、边界；扩展分类并按 Mesh 共享 |
| [边绘制](../../crates/zipzi-editor/src/mesh_edges.rs) | 批量绘制直线；增加固定屏幕长度的虚线与有界缓存 |
| [视口交互](../references/viewport-interactions.md) | capture token、过期帧拒绝、多视口所有权；新工具沿用 |

Core 负责持久化类型、结构不变量和原子应用；SDK 负责纯算法、编辑语义和依赖处理；Runtime 负责会话、版本、candidate、Job 和唯一历史；Editor Core 负责模式、选择、输入状态和派生缓存；Editor 负责界面、命中反馈和绘制。三角化算法不进入 UI 或语言绑定。

## 持久化数据

以下为目标形状；真实声明继续采用项目现有 serde、ApiShape 和 BinaryWire 机制。

```rust
pub struct Mesh {
    // 既有字段保持其含义，其他属性省略。
    pub vertex_ids: Vec<VertexId>,
    pub base_positions: Vec<Vec2>,
    pub uvs: Vec<Vec2>,
    pub triangles: Vec<[VertexId; 3]>,
    pub meshing: Option<MeshMeshing>,
}

pub struct MeshMeshing {
    pub boundary: Vec<VertexId>,
    pub constraints: Vec<MeshConstraint>,
}

pub struct MeshConstraint {
    pub endpoints: [VertexId; 2],
    pub kind: MeshConstraintKind,
}

pub enum MeshConstraintKind {
    User,
    EdgeSupport,
}
```

### 外围与结构约束

`boundary` 是有序闭合外围，不重复存末尾起点。统一在 UV 空间按逆时针排列，循环起点规范为最小 VertexId，避免相同外围因遍历起点产生无意义差异。这个顺序不要求重排 vertex_ids。

约束端点按 ID 大小规范化，集合按端点排序。禁止自环、重复边和同时存在两种 kind 的同一条边；User 优先于 EdgeSupport。外围边不再重复存入 constraints，外围身份本身就意味着必须保留。

EdgeSupport 表示生成算法建立的内侧支撑边，可形成环、开放链或分支，但每个线段必须属于实际三角面。它不是无面的独立边，也不参与内外区域翻转。用户显式固定支撑边会把它转成 User；移动支撑边端点时，将受影响的 EdgeSupport 边转成 User，防止以后边缘重建静默覆盖手调。移动外围点只修改外围。

不持久化 EdgeId、FaceId、自动边集合、邻接表、三角化器状态、single/double 模式标签或完整生成来源日志。single/double 是生成选项，保存结果本身才能正确表达手工修改后的网格。

### 普通网格与首次启用

`meshing: None` 表示没有结构编辑数据。普通导入网格、零三角形网格、孔洞和多区域网格仍受原有通用校验支持。

首次进入模式时，从现有三角形的单面邻接边提取边界，检查是否恰好一个简单闭环并验证所有面和顶点。符合条件时构建临时 MeshMeshing，constraints 为空，不把所有已有内部边自动固定。进入或退出而没有实际修改，不改变文档、三角形或历史；第一次成功操作将 meshing 与几何一起提交。

有 meshing 的 Mesh 必须始终满足其拓扑不变量。后续添加动画绑定不自动删除 meshing，但会关闭首版手动编辑能力。姿态变形不在显示坐标上重新校验或重新三角化。

### 三角形与虚线

`triangles` 是实际绘制和撤销恢复的结果，继续持久化。外围和结构约束控制重新三角化，实际三角形不是可任意过期的缓存。

令 E 为三角形全部无向边的去重集合，B 为外围边，C 为结构约束，则虚线集合为 `E − B − C`。因此虚线的实际连接关系已经随 triangles 保存，不需要第二份持久化 dashed_edges。

保存重开、Undo、Redo 恢复已保存的 triangles；不能为了重建虚线显示再次执行三角化。固定一条虚线即使不改变 triangles，仍然是有历史和脏状态的正式修改。

## 不变量与几何规范

通用 `validate_render_mesh` 保持现有语义；meshing 为 Some 时额外验证以下条件：

1. 外围至少三个不同顶点，全部 ID 存在，UV 有限、外围面积非零且无自交。
2. 所有顶点参与至少一个非退化面，内部点位于外围内部，边界点显式属于 boundary。
3. 约束端点有效，线段位于区域内，不穿越外围或其他结构约束；共享端点合法。
4. 线段经过其他已有顶点时，操作层将其规范化为子边链；持久化结果不保留穿过未分割顶点的长边或 T 接头。
5. 三角形没有重复面、重叠、孔洞、漏点或非流形连接；边界与 boundary 完全一致，所有约束子边都属于三角形。
6. 三角形覆盖区域恰好等于外围内部。UV 面绕序一致，导入后的编辑保持既有合法面绕序；新生成采用统一绕序。外围规范顺序不强迫现有三角形翻面。
7. 同位置不同 ID 的 UV 顶点、共线零面积面、无法可靠分辨的几何退化被拒绝，不静默合并或丢点。

拓扑相交和区域合法性在源图／UV 空间判断。基础位置允许仿射反射，不能把模型空间面积符号变化误判成 UV 非法。运行姿态可以产生重叠，不能因此损坏合法建模数据。

使用稳健方向与相交谓词；计算采用 f64，落盘仍为现有 f32，转换后再次校验。几何容差依据源图尺度和 f32 精度统一定义。屏幕命中、吸附阈值使用逻辑像素，不能复用作几何合法性容差。

Core 在创建、替换、完整结构验证和最终 candidate 发布路径执行共同校验。算法返回后不能绕过它；一次批量操作只要求最终候选合法，中间计算数据不写入正式 Mesh。

## 源图与纹理固定

首版输入必须具有实际可读的完整独立图片资产。共享纹理、区域来源不明、缺图、零尺寸、非仿射映射、普通绑定、BlendShape、Glue、任何父变形器均给出明确不可用原因，不执行自动解绑或自动重置。

独立完整源图的判断不能仅凭当前只有一个 Mesh 引用该资产；导入路径、资源用途或明确的完整源图指定也必须能证明其范围。只有 Atlas 页、没有区域元数据的输入不获得完整源图资格。复用已有导入来源信息；不足时补充显式资源用途元数据，禁止用引用数量猜测。图片区域恢复和旋转 Atlas 仍后置。

复用并抽出 [TextureTransform](../../crates/zipzi-editor-core/src/mesh_generation_alpha.rs) 的仿射恢复思路到可供 SDK 使用的层级，得到 `position = origin + u * uv.x + v * uv.y` 及其逆。进入模式恢复基准，手势开始冻结该映射；一次拖动始终从操作前快照计算绝对结果，不累计逐帧偏移、不逐帧重新拟合。

首版不新增独立持久化图片摆放矩阵：至少一个有效三角形和统一仿射位置／UV 已能恢复它。外部修改几何或源图后结束当前手势，重新检查能力并恢复映射。资源更新的依赖包含内容身份和尺寸，不能只看 asset ID。

编辑时以完整目标源图作为参考，受网格裁切之外的部分仍可见。默认单独绘制参考图并隐藏目标 Mesh 的重复纹理层，保留其他对象作为上下文；提供实际结果预览与隔离显示。参考图只用于 Editor 覆盖，不改变导出或正常模型渲染。局部显示范围始终来自完整源图矩形，UV 限于该图片区域。

## 操作语义

所有操作以固定的 Mesh 身份和当前来源版本为输入。除进入、选择和导航外，以下成功动作各产生一条历史；失败、取消及净无变化操作不产生历史，也不清空 redo。

| 操作 | 完整行为 |
| --- | --- |
| 内部点击加点 | 分割命中的面，按固定映射计算位置和 UV；无需添加人工约束；生成有效面 |
| 边上加点 | 分割该边及相邻面；外围插入有序 ID，结构边拆成同 kind 的两个子边 |
| 命中已有点 | 选择或作为连线端点，不创建重叠点 |
| 外围之外点击 | 首版拒绝；通过分割和移动外围点扩展，不猜测新外围 |
| 拖动已有点 | 同步位置和 UV；维持外围和结构约束，必要时更新自动边；全选移动仍受图片范围限制 |
| 固定内部连接 | 已有虚线转 User；非现有连接插入约束并局部重连；经过已有点则转为连续子边 |
| 固定生成支撑边 | kind 从 EdgeSupport 改为 User；不必改变三角形 |
| 解除结构约束 | 删除 User 或 EdgeSupport 记录，重新连线；该边允许继续作为虚线存在 |
| 删除内部点 | 删除点及相连结构约束，填回局部区域；不跨被删点自动继承一条长约束 |
| 删除外围点 | 按原外围顺序移除点，连接保留的相邻点；新区域排除任何保留点、破坏约束或自交则拒绝 |
| 批量删除 | 同时构造最终外围和保留点，不按点击顺序逐个提交；整体成功或整体失败 |
| 删除自动边 | 不提供；可通过固定另一个合法连接改变自动三角化 |
| 删除外围边或面 | 首版不提供 |
| 自动连接 | 保留点、外围、User 和 EdgeSupport，只重建自由连接；结果未变化则不提交 |

删除判断针对重建后的最终网格。删除扇形网格中心点即使移除全部旧面，只要外围可重新成面就成功；少于三个可成面顶点或最终没有有效面的操作整体拒绝。

拖动保留起始快照和最近有效预览。非法请求不写入 candidate，显示阻挡边／原因并保留最近有效位置；松开提交当前显示的有效预览，Esc 恢复手势前状态。穿越障碍需要检查从上一个接受位置到请求位置的运动，不能因鼠标事件稀疏而跳过交叉。首版可拒绝整段非法增量，不实现跨障碍自动吸附。多点拖动整体处理，不只移动其中合法的点。

手动移动、增删及连线优先重建受影响三角区域，保留区域外连接。不能用对所有自动边的全局优化代替每次拖动；全局重新连接只由明确的自动连接命令触发。同一输入和约束产生确定性结果，给共圆等歧义规定稳定的 ID 排序规则。

## 原子编辑接口与写入审计

新增 SDK 纯操作输入／输出，命名可按现有模块风格落地：

```text
ManualMeshOperation
  InsertVertex / MoveVertices / DeleteVertices
  ConstrainEdge / ReleaseConstraint / Reconnect

MeshEditResult
  完整 MeshGeometry（含 meshing）
  旧 VertexId 到新 VertexId 的映射
  新增与删除的 VertexId
  受影响边及可定位诊断
```

纯算法只接受几何、映射及选项，不读取窗口或修改 Runtime。保留点的 VertexId 不变；新增 ID 由统一 SDK 分配规则产生，在 candidate 中保持稳定；取消后不能让过期引用误指新操作。不得将三角化器内部 handle 或坐标排序编号当作现有文档 VertexId。

使用现有 TopologyReplacement／candidate 发布路径提交。首次附加 meshing、位置／UV 修改、约束更新、三角化结果和 ID 映射属于同一原子结果。首版依赖为空仍由公共依赖校验检查，避免能力检查与实际提交间发生变化。

必须审计以下旧路径，不能只给新工具接线：

| 写入类型 | 对已有 meshing 的要求 |
| --- | --- |
| 普通 SetPositions 与变形编辑 | 保留 meshing 和 UV，不自动触发手动布点语义 |
| UV 修改 | 完整校验 UV 拓扑；无法维护则拒绝 |
| 顶点增删／三角形替换 | 使用维护结构的专用操作，或提交完整一致结果；裸操作破坏结构则拒绝 |
| 顶点重排 | 保留基于 VertexId 的外围与约束，正确重排平行数组 |
| 自动生成及整体替换 | 明确提供新 meshing，或通过显式放弃结构的整体替换语义提交 None |
| 克隆与复制 | 同步复制 meshing；若重分配 VertexId，全部引用一起重映射 |
| 属性修改、改名、外观与锁定 | 保留新增数据，不以默认值重建 Mesh |
| 导入导出 | 导入允许 None；导出使用实际三角形，外部格式不支持约束时不声称保留编辑意图 |

一般写入不能静默把 Some 清成 None。需要放弃约束的调用必须明确声明整体替换／清除结构；生成预览展示将被替换的范围。不得通过忽略未知字段或转换为旧 MeshGeometry 丢失元数据。

## 虚线与缓存

### 拓扑缓存

扩展现有 CanvasTopology 内的 MeshTopology，避免新建第二套边统计服务。每个 Mesh 持有可共享的不可变数据：

```text
MeshEdgeTopology
  vertex ID 到数组 slot 的映射
  全部去重边
  boundary edges
  user constraint edges
  generated support edges
  automatic edges
  必要的顶点及三角邻接
```

集合按规范端点排序。UI 渲染和命中使用同一分类；User 和 EdgeSupport 都是结构实线，可用颜色或悬停信息区分，自动边为虚线。鼠标尚未确认的连线预览单独绘制，不混入 automatic edges。

缓存生命周期由 mesh 身份及对应的拓扑内容变化控制。顶点 ID／顺序、三角形、外围、约束端点或 kind 变化均失效；仅改位置、UV 且最终连通性不变时复用分类。UV 合法性检查独立于分类缓存复用。

必须识别 SourceRef 的 Formal 与 Edit；Edit 的 edit ID、base 和 sequence 用于拒绝过期预览。不能只用正式 revision 作为所有 candidate 的缓存键。来源身份负责新旧校验，内容变化证明负责是否复用同一 Arc；不能为了复用缓存忽略新的 sequence，也不能每次 sequence 变化都无条件扫描全部三角形。

首次可以沿现有通知路径安全失效，完成时要求拓扑变化只重建受影响 Mesh 的边分类，其余 Mesh 共享已有 Arc。纯约束变化必须让覆盖层刷新，即使模型求值版本不变。Undo／Redo、目标删除／替换和重新打开文档不得复用不匹配的 slot 索引。

### 屏幕绘制缓存

每个视口另存投影后的实线与虚线段缓存。键包含边拓扑身份、当前位置／预览身份、相机缩放和平移、视口尺寸、DPI、线宽及虚线样式。位置、相机或样式变化只更新所需投影／分段，不重新扫描三角形。

虚线长度与间隔使用屏幕逻辑像素，缩放后保持可读。先裁剪视口外边，再分段并按现有批量 path 大小限制绘制。段数及缓存内存设上限；高缩放或密集网格时采用明确的显示降级，不能无限分配。命中检测按完整几何线段，不因鼠标落在虚线空隙而失效。

选择／悬停只更新强调样式，尽量不重建静态线段。不同视口共享拓扑，分别拥有相机相关数据。缓存不进入项目、checkpoint 或历史；关闭文档、结束预览和视口销毁时释放，不积累整个历史的几何副本。

## 自动生成算法拆分

### 三个纯算法职责

```text
build_edge_layout(mask, edge_options) -> EdgeLayout
sample_interior(layout_or_mesh, sampling_domain, interior_options) -> InteriorSites
triangulate_constrained(vertices, domain, constraints, options) -> Triangulation
```

EdgeLayout 使用独立的算法局部点身份，包含有序区域边界和结构支撑线。single edge 生成外围；double edge 额外生成内侧支撑环／链。细长部分允许退化为有效支撑链或没有内环，不能为了保持“双环”制造交叉或零面积面。

第二层内侧支撑不代表孔洞。现有 Alpha 算法中 outline 决定区域内外，support ring／chain 只约束连接，这个区分必须进入输出；不能在 MeshGeometry 阶段全部丢掉。

算法中间结果允许只有点和线，不受 Document Mesh 必须有面的约束。每次将结果应用到正式文档时，仍必须完成基础三角化和最终验证。

内部布点还需要采样区域，不能只用外围隐式代替原始前景：现有算法在覆盖透明孔洞时仍可能避开这些区域布置密集点。接口将采样域与三角化覆盖域分开。生成任务从已验证的源图／Alpha 构建采样域，并与当前外围相交；旧高级生成器保留原有 hole／island 规则。

纯算法可表达多区域和孔洞，以保留当前 Alpha 生成能力。首版手动编辑的应用适配器只接受一个无孔洞区域，不通过凸包、删除小区域或填洞强行转换。不满足时可作为普通 MeshGeometry 输出 meshing: None，清楚报告无法继续首版手动编辑。

### 面向用户的操作

| 操作 | 输入和保留规则 | 正式提交 |
| --- | --- | --- |
| 生成边缘布局 | 从源图生成单／双层结构；明确属于重新建立边缘的操作 | 用边缘点和结构边做基础三角化，一步替换；此时没有密集内部点也仍是有效 Mesh |
| 补充内部点 | 保留现有点、外围和全部约束，只添加新内部点 | 添加与重新连线一起提交一步 |
| 自动连接 | 保留全部点及结构，重建自由连接 | 一步提交，没有变化则不提交 |
| 一次完整自动生成 | 顺序组合三个纯阶段 | 整个结果一次提交，不暴露中间半成品 |

对已有人工约束的 Mesh，普通“生成边缘布局”不能静默抹掉 User 数据。首版采用明确的整体重新生成入口及结果预览，说明替换全部现有几何；保留人工线的选择性边缘重建后置。内部补点始终保留人工点和结构。

生成预览依附当前目标、SourceRef、选项代次、源图内容身份与尺寸。参数改变、目标变化、源图更新或正式版本变化后，旧结果不能应用；异步结果完成也必须重新校验，不自动覆盖新状态。

当前不保存内部点来源，因而不提供“只删除上次生成的内部点再重采样”。未来要提供这种操作时再增加生成点归属；禁止把所有非边界且非约束端点当成可删自动点。

拆分后保留 alpha_mesh_geometry 作为组合接口，维持既有文档化行为、预算、孔洞／多区域能力和确定性测试。算法内部可使用临时三角化构造细长区域支撑，这不改变三个公开阶段的职责。

## 编辑器模式与生命周期

模式状态属于文档的 Modeling 上下文，包含目标 Mesh、工具、点／边选择、映射、诊断及当前手势引用。多个 Modeling 视口共享目标和选择；各自拥有导航状态，同一时刻只有一个编辑 capture。

入口放在现有自动生成旁，并接入 Modeling 菜单与命令系统。快捷键先检查已有绑定，避免硬编码冲突。工具采用选择、创建、删除／解除约束；UI 显示当前目标和退出动作，不显示整次“完成／取消”。

| 输入或事件 | 行为 |
| --- | --- |
| 单击顶点、Shift／Option、框选 | 沿用现有选择习惯；选择变化不进入文档历史 |
| 拖动选中顶点 | 越过现有阈值才开始编辑；移动整组；一次拖动一步 |
| 创建工具单击／拖动 | 单击加点，顶点间拖动连线；顶部显示当前合法性反馈 |
| Cmd+A | 仅选择当前目标的可编辑顶点，文本输入保留自身行为 |
| Delete／Backspace | 点选择优先删点；边选择只解除结构约束；不提供模糊混合删除 |
| Esc | 当前手势取消；无手势时退出模式并保留已完成修改；不覆盖弹窗或文本输入的 Esc |
| Undo／Redo | 有手势先取消手势，本次按键只取消；空闲时调用 Runtime 文档历史 |
| 保存 | 按现有 FinishEdit 生命周期结算当前显示的有效 candidate，再保存；失败不继续保存 |
| 切换文档／模式／关闭 | 沿用 FinishEdit 结算有效操作后退出模式；旧 capture 失效 |
| 失焦／丢失按键／异常取消 | 取消当前手势，保留此前已提交操作 |
| 外部修改／目标删除或锁定 | 拒绝旧候选，清理失效选择与 capture；重新检查目标，必要时退出模式 |

目标在模式内固定，点击其他对象不隐式替换目标。对象树显式切换目标时通过统一上下文切换路径结束当前编辑。播放／参数姿态变化不能绕过首版能力限制。

命中优先级为点、结构边、自动边、面；边界与约束的诊断可定位。纯导航不修改文档。所有延迟输入必须验证原 capture token，旧手势清理不能取消后继手势。

## 发布与接口同步

MeshGeometry 增加可选 meshing，并贯穿 MeshCreateSpec、GeometrySnapshot、TopologyReplacement、Runtime MeshSnapshot、borrowed 输出及 Python2 生成类型。MeshGeometry 的 positions 文档统一为明确的 authoring 坐标约定，不能继续在父变形器存在时误称为 model space；首版入口限制不代替公共类型语义修正。

Project MeshWire、encode/decode、实验 CBOR 路径、格式版本检查及相关说明同步更新。实现时递增当前格式版本，不在计划中预占版本号。导入来源和导出格式的编辑信息丢失范围必须明确；原生项目往返保存保留 meshing。

结构变化与渲染变化分开处理：所有 meshing 修改更新正式文档版本、历史、脏状态及 MeshTopology 内容通知；只有实际求值输入变化才需要刷新模型求值。分类缓存不能依赖 evaluation_revision 单独失效。

审计 checkpoint 的逐字段求值等价判断、MeshPublication 快速路径证明、完整 ChangeSet 比较、content epoch、Editor projection 失效和传输／历史内存预算。纯约束变化必须通知，不得被标成净无变化；仅改名等快速路径不得把新字段变更漏掉。

Runtime 继续承载正式与 candidate 的读写。新增语义命令通过现有 ModelingCommand／类型化调用注册接入，不另建 Editor 私有写通道。慢生成使用既有 Job／预览设施；过期结果不提交。同步增量操作按现有 capture candidate 路径执行，先通过测量决定是否需要额外后台调度。

## 实施阶段

阶段依次落地；每阶段以可独立验证的纵向功能为边界。此计划不要求一次完成未来全部生成 UI。

| 阶段 | 主要交付 | 完成条件 |
| --- | --- | --- |
| P0 数据与契约 | MeshMeshing、校验、几何读写、Project codec、历史预算、通知、协议和绑定更新 | 包含纯约束修改的保存往返、撤销重做、Local／Connected 读取正确；旧写入不破坏结构 |
| P1 纯编辑算法 | 提取边界、加点、分割、删除、移动、约束与自动连接、稳定 ID 和确定性输出 | 无 UI 的操作矩阵通过；失败原子；最终结果全部满足不变量 |
| P2 Runtime 与缓存 | candidate 命令、版本检查、按 Mesh 边分类缓存、预览来源校验 | 正式版本相同的不同 candidate 不混用；纯约束变化刷新分类；无拓扑变化时复用 |
| P3 Editor 基础闭环 | 模式入口、工具、参考图、点边选择、虚线、命中、生命周期 | 单 Mesh 手调、保存重开、Undo／Redo、多视口和失焦行为通过原生验收 |
| P4 生成器内部拆分 | EdgeLayout、采样域、内部补点、公共受约束三角化、组合接口 | 单／双边结构输出不丢失；原 Alpha 孔洞、多区域、预算和确定性能力保留 |
| P5 分阶段生成交互 | 边缘生成、内部补点、自动连接各自预览／应用，完整生成组合入口 | 每阶段生成后都可手调、保存和独立撤销，过期预览不能应用 |

P1 可先抽出公共约束三角化设施，P4 再迁移现有 Alpha 实现，避免提前重写整个生成器。P3 接入当前完整生成器时，仅为满足准入条件的结果建立 meshing；P4 完成后才要求保留生成支撑边种类。该临时接入不能声称已支持 double edge 结构往返。

### 实施记录

2026-10-26，P0 数据与契约：

- 新增 MeshMeshing／MeshConstraint／MeshConstraintKind，贯通创建、完整替换、SDK 快照、Runtime 普通及 borrowed 读取、Python2 生成类型，并同步旧 Python 几何元组。
- 在 Core 创建、替换和完整结构验证中校验 UV 外围、约束、面邻接、绕序、退化、交叉和 T 接头；普通 meshing: None 网格保持原有规则。使用 f64 稳健方向谓词及空间索引进行相交候选筛选。
- 原生 JSON／实验 CBOR 格式升级为 21，保留精确三角形和建模结构，不提供旧格式迁移。
- 纯约束变更进入历史、脏状态、MeshTopology 通知和 Editor 投影失效；正式提交及 Undo／Redo 保持求值版本。新增数组计入 checkpoint 和 Runtime 读取预算。
- 普通替换不能清除结构；原始 UV／面／顶点操作只能提交合法最终结果。完整 TopologyReplacement 可显式放弃结构，旧网格生成入口对已有结构暂时禁用，等待明确的整体重新生成预览。

本阶段没有增加手动编辑入口、手势或三角化操作；接下来实施 P1 的纯编辑算法。

本次验收结果：

- Core 建模拓扑及预算定向测试 5 项、SDK mesh 相关集成测试 23 项、发布摘要与快速路径对照测试 17 项通过。
- Project JSON／CBOR 及项目集成测试 47 项、Runtime 定向测试 2 项、Editor Core 投影／历史定向测试 1 项通过；Editor Core 与旧 Python 适配器编译检查通过。
- Python2 Local／Connected 行为测试 81 项、恢复测试 16 项、正反向静态类型检查、37 份生成输出检查及 codegen 工具测试 16 项通过。覆盖建模结构二进制传输、保存重开和超预算输入拒绝后的 candidate 可用性。

## 验收与测试

测试从最小模块和集成切片开始，按 [根 README](../../README.md#tests) 与 [工具测试说明](../../tools/README.md#test-coverage) 扩大。新测试模块名采用 manual_mesh，新增 SDK 普通 CPU 集成注册到既有 integration target。不要为简单字段赋值写镜像实现测试，重点覆盖语义和数据边界。

### 几何与数据验收

- 凹形单外围不会变成凸包；双层内侧支撑不会被识别成孔洞。
- 内部、外围、人工边、支撑边插点正确分割；共边两侧同步更新，无 T 接头。
- 删除扇形中心点重建成功；删除最后可成面的点整体失败；批量删除与输入顺序无关。
- 约束相交、穿越外围、重合点、共线退化、f32 转换坍塌产生可定位错误，原数据和历史不变。
- 固定现有虚线和 EdgeSupport 转 User 不改面也产生正确历史与通知。
- 旋转、反射、缩放的仿射图片映射下，拖动保持参考图固定；源图更新和非仿射输入被正确处理。
- 所有旧几何 API 对 Some 维护或拒绝，对 None 保持原能力；不得遗留无效引用。
- 原生 JSON／CBOR、SDK 快照、Runtime borrowed、Python2 Local／Connected 往返保留新增字段。
- Undo／Redo 恢复精确 triangles、外围、约束和 ID；不再次三角化；取消和无变化操作保留 redo。

### 缓存与性能验收

- 静止画布重绘不重复统计边或分段；导航只更新投影层，不重建拓扑层。
- 纯位置更新且连通性不变复用边分类；约束 kind 改变立即更新样式。
- 同一正式 revision 下连续 candidate sequence 展示最新结果，取消后恢复正式缓存。
- 未修改 Mesh 的缓存 Arc 保持复用；关闭文档／视口后释放缓存；内存不随历史步数增长。
- 用现有 benchmark instrumentation 计数构建次数，以普通网格、密集网格和高缩放视口测量；禁止用无依据的帧率承诺替代验收。
- 虚线分段有界，视口外边不大量分配；密集时样式降级不改变拓扑或命中语义。

### 输入与生成验收

- 点击不意外拖动；文本输入、IME、Cmd+A、Delete、Esc 按当前输入所有权路由。
- 保存中拖动、失焦、多视口、延迟 Pointer Up、目标删除／锁定和外部提交不会错误发布。
- single edge、double edge、细长支撑链及内侧收缩消失都有有效结果和明确预览。
- 内部补点不移动或删除人工点，不破坏外围／User／EdgeSupport；满预算时原子失败。
- 旧 Alpha 的孔洞、多区域和透明区采样行为保留；手动模式不支持时明确禁用，不静默改形状。
- 参数改变或源图内容改变后，旧异步生成结果不能提交；每次应用各一条历史。

### 测试入口

以下 manual_mesh 过滤器在相应阶段新增测试后使用；运行时检查匹配数量，零用例不算通过。Project／Runtime target 按各 crate 当前测试清单选择，先用 --list 确认。

```sh
cargo test --locked -p zipzi-core --lib manual_mesh
cargo test --locked -p zipzi-project manual_mesh
cargo test --locked -p zipzi-sdk --test integration manual_mesh::
./tools/test-sdk.sh runtime manual_mesh
./tools/test-editor.sh fast manual_mesh
./tools/test-editor.sh ui manual_mesh

# P0 协议和生成类型变更后
uv run --locked python crates/zipzi-python2/tools/generate.py --check
./tools/test-sdk.sh python2

# P4 改动公共生成算法后
./tools/test-sdk.sh mesh
```

Editor shell gate 会清除 ZIPZI_DEV_SESSION_DIR；直接执行 Editor Cargo 测试时显式清除。UI 验收需要 macOS／Metal。共享几何和图片样本放 fixtures，生成器工具测试放 tools/tests；只有新几何失败或公共算法回归风险才扩大到 stress／完整回归，不默认运行 full。

## 后续扩展边界

空网格重建需要独立源图摆放和新的正式状态语义；Atlas 编辑需要图片区域及旋转／裁切映射；复杂绑定需要位置和形状重采样。这三项不借首版 meshing 字段隐式承诺。

孔洞、多区域手动编辑将来需要把单 boundary 扩为区域集合，并升级校验和操作语义；现有自动生成算法能力保持独立。按来源替换内部点需要新增点归属，选择性重建支撑带需要明确 User 与 EdgeSupport 的冲突规则。都通过明确的数据模型变更实现，不提前加入闲置的通用图结构。

## 参考

- [Spine Mesh attachments](https://en.esotericsoftware.com/spine-meshes)：编辑模式、轮廓、手工边与自动三角边，以及无孔洞限制。
- [Spine 关于 UV 交叉的说明](https://es.esotericsoftware.com/forum/d/10330-cant-move-big-groups-of-vertices-in-edit-mesh-mode/13)：UV 拓扑约束与显示姿态的区别。
- [视口交互约定](../references/viewport-interactions.md)：输入所有权、capture 和生命周期。
- [SDK README](../../crates/zipzi-sdk/README.md)：会话历史和当前 Alpha 生成契约。
