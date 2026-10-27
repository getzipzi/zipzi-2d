# dev-python API 易用性与网格工具问题记录

日期：2026-10-27。状态：仅记录，尚未修复。

本轮使用 `./tools/dev-python` 验证 `zipzi-python2` 的 Editor 连接、模型结构读取和编辑。
用户另提供了一轮只通过 `zipzi.describe()` 发现接口的结构读取反馈。
前三个文档问题已在当前源码环境中通过公开 `describe()` 调用独立复现。
后续章节补充模型编辑中遇到的 API 歧义和网格工具能力缺口，
区分已确认的行为、易用性问题及仍需核实的契约和能力范围。
宿主断连保留为用户报告的待排查项，不据此认定 `modeling.records()` 存在缺陷。

前三个文档问题的公开目录复核没有连接宿主或修改模型，没有使用源码读取、
`inspect` 或 `dataclasses.fields` 获取接口签名和字段。
模型编辑阶段和本次补充核对读取过 `zipzi-python2` 源码、文档及本轮临时结果；
不能将整个验证过程描述为完全依靠 `describe()`。补充核对没有修改模型。

## 问题状态约定

- **已确认问题或能力缺口**：有公开目录、类型定义或实际调用结果支持；能力缺口只限定于已检查的 Python 公开接口。
- **已确认行为，契约待澄清**：行为发生过，但不能直接判定实现错误，需要明确适用条件或补充说明。
- **待核实项**：有疑问或用户报告，尚不足以认定缺陷；列明后续所需证据。

以下改进建议均为待办，不表示实现已经修改或补验通过。

## 复现环境

- 入口：仓库根目录的 `./tools/dev-python`，加载当前 editable 客户端。
- Python：CPython 3.14.7；平台：macOS arm64。
- 包版本：`0.1.0a1`。
- 协议：`zipzi-runtime-r7-manual-mesh-1`。
- `describe("zipzi").documentation` 报告的客户端源码 SHA-256：
  `d2e7ef61598308add668663b327004d7474405ba01150fb1b0e8009d342cbddd`。

该身份用于界定本次观察，不代表后续工作区或发行产物始终保持相同内容。

## 已复现的文档问题

### Runtime 子接口导航遗漏 sessions

`zipzi.describe("Runtime").children` 未包含 `zipzi.Runtime.sessions`，
但直接查询 `zipzi.describe("Runtime.sessions")` 可以得到完整说明及
`attach/create/import_/list/open/subscribe` 子接口。
`Session` 的文字说明提到了 `runtime.sessions`，因此可以绕过遗漏继续发现接口。

影响：从 Runtime 沿 `children` 逐层导航时，用户无法发现会话注册表入口；
目录收录了该对象，但父子导航关系不完整。

建议修复父级导航生成，并审查同类属性入口是否也有遗漏。
验证应覆盖“父级可发现”和“直接查询可用”两条路径，而不仅检查符号存在。

### ModelingQuery 签名显示实现表达式

`zipzi.describe("modeling.ModelingQuery").signature` 在本次复核中长达
2034 字符，集合默认值展示为类似以下内容：

```text
assets: tuple[str, ...] = field(default_factory=lambda: tuple((string(v0) for v0 in sequence([]))))
```

布尔默认值也显示为 `field(default_factory=lambda: _boolean(False))`。

影响：公共签名混入生成器、转换与 dataclass 实现细节，难以快速理解实际默认值，
且增加帮助查询的输出长度。

建议在文档签名中显示等价的公共默认值，例如 `assets: tuple[str, ...] = ()`
和 `physics: bool = False`，保留现有字段语义说明。无需为了文档展示改变运行时默认值的实现。
验证应检查生成目录与参考文档，并确保实际调用语义不变。

### 直接打印 ApiDescription 无法展示完整帮助

`print(zipzi.describe(...))` 输出有界摘要，会截断签名和说明并省略部分字段。
例如本次 `ModelingQuery` 的完整签名为 2034 字符、说明为 2815 字符，
对象直接打印的摘要只有 317 字符。
完整内容仍可从 `.signature`、`.documentation`、`.fields`、`.children` 读取。

影响：用户容易把对象摘要误认为完整帮助，必须额外发现并逐字段输出。
这是展示与引导问题，不是文档内容丢失。

建议保留有界 `repr`，同时提供或明确说明完整帮助的输出方式。
如新增文本格式化方法，应明确完整输出与摘要输出的区别。

## 最小复现

以下调用不需要启动或连接 Editor，也不需要读取源码或反射类型：

```sh
./tools/dev-python - <<'PY'
import zipzi

runtime = zipzi.describe("Runtime")
sessions = zipzi.describe("Runtime.sessions")
query = zipzi.describe("modeling.ModelingQuery")

print("Runtime children:", runtime.children)
print("sessions discoverable:", "zipzi.Runtime.sessions" in runtime.children)
print("sessions children:", sessions.children)
print("query signature:", query.signature)
print("direct print:", query)
print("full documentation:", query.documentation)
for name, documentation in query.fields:
    print(name, documentation)
print("query children:", query.children)
PY
```

本次实际结果：`sessions discoverable` 为 `False`；直接查询 sessions 成功；
query 签名包含上述实现表达式；对象摘要截断而各字段内容保留。

## 其他已确认的 API 易用性问题

### 会话元数据缺少文档显示名称

`SessionInfo` 提供 `version/modified/history/id/state/path`，没有显示名称字段。
本轮读取三个会话时，只能从项目目录名推断 smoke、Shirousagi 和 mao_pro。
这是已确认的元数据便利性缺口；目录名不能保证等于 Editor 文档标题，
也不适用于没有关联路径的会话。

证据：[`SessionInfo` 定义](../../../crates/zipzi-python2/client/python/zipzi/_values.py)。
建议明确提供显示名称的入口及其与路径、未命名文档的关系。

### 根节点与未分配关系的表示不一致

`scene.Summary.part_id/parent_id` 使用 `None`；完整 `modeling.Mesh.part_id/deformer_id`
及 `modeling.Part.parent_id` 使用空字符串表示根或未分配。
读取 Part 树和完整建模记录时，需要做额外转换，不能直接混用这些字段。

证据：[`scene.Summary`](../../../crates/zipzi-python2/client/python/zipzi/_generated/scene.py)
及 [canonical Mesh/Part](../../../crates/zipzi-python2/client/python/zipzi/_generated/modeling.py)。
这属于已确认的表示差异；两侧已有字段说明，不能记为根节点语义完全未定义。
建议统一公开表示，或在跨接口示例中明确转换规则。

### Mesh 的两套读取入口和纹理关联链路

`meshes.get/get_many` 返回的 `meshes.Mesh` 仅包含 `id/name/geometry`；
纹理引用、组织 Part、变形父级、绘制顺序及外观需要从 `modeling.records()` 的
`modeling.Mesh` 获取。进一步读取图像，还需要解析 `texture_asset_id` 的引用变体，
取得图像对象 ID，再调用 `resources.get()`。

本轮识别网格语义时实际使用了“ID 目录 → 完整网格与资源元数据 → 图像”的链路。
这些逻辑读取步骤不等于底层 RPC 数量；本轮未测量其固定延迟或证明存在性能回归。

证据：[`meshes.Mesh`](../../../crates/zipzi-python2/client/python/zipzi/_generated/meshes.py)
及 [canonical Mesh 与 SceneReference](../../../crates/zipzi-python2/client/python/zipzi/_generated/modeling.py)。
建议补充选择入口的指南，以及从一个网格读取组织、外观和纹理的完整示例。
是否需要聚合检查接口，应由实际调用成本决定。

### 身份字段和关键形坐标采用不同表示

已确认存在以下区别，属于学习与发现成本，尚不是身份或数据错误：

| 表示 | 本轮遇到的区别 | 改进方向 |
| --- | --- | --- |
| Session 身份 | `SessionId.serial`、`Source.session` 属于 Runtime；`Version.session_id` 属于 SDK authoring session，数值可能不同 | 提供同一示例中的对应关系；已有 Version 字段说明明确区分，不能当成未定义行为 |
| 对象引用 | 普通对象使用 `.id`，`SceneReferenceResolved` 使用 `.object_id`，`TransformId/PartId` 包装使用 `.value` | 为引用变体和 ID 包装提供统一导航示例 |
| 创建返回值 | `edit.meshes.create`、`edit.parameters.create` 直接返回 ID；`edit.apply(scene.CreatePart(...))` 返回 `WriteResult`，需从 `.ids` 取结果 | 在操作示例中明确返回对象与 ID 提取方式 |
| 绑定关键形 | 便利 `bindings.Axis/Keyform` 使用数值 keys；canonical `BindingAxis/WarpKeyform` 使用 sample UUID 关联 | 补充数值坐标、轴和 sample 身份之间的对照示例 |

本轮把 `texture_asset_id.object_id` 误写为 `.id` 产生过 `AttributeError`。
正确字段已在文档中说明，该次失败是使用错误；不能以此证明 API 返回错误字段。
同样，模型编辑时直接查看类型定义是操作选择，不足以证明只能查源码才能使用这些接口。

证据：[`Version/Source/WriteResult`](../../../crates/zipzi-python2/client/python/zipzi/_values.py)、
[`bindings` 值](../../../crates/zipzi-python2/client/python/zipzi/_generated/binding_values.py)
及 [canonical 引用与绑定类型](../../../crates/zipzi-python2/client/python/zipzi/_generated/modeling.py)。

## 已确认行为但契约待澄清

### 创建 Part 自动显式化绘制分组

本轮整理 Part 前 `draw_order_groups` 为 `None`；创建 Part 后变成显式分组。
一次独立候选探测确认：原网格仍留在根绘制组，新 Part 增加分组结构，
组织上的 `MoveToPart` 不会自动把网格移入该 Part 的绘制组。

最初的验证把“元数据完全不变”作为条件，因此取消了候选；
核对后改为检查原网格绘制成员及顺序、绘制范围和最终像素，再完成正式整理。
正式结果保留原网格数据与绘制顺序，渲染像素一致。

**已确认的是规范化行为，不是绘制破坏。** 待澄清其触发条件、旧隐式绘制结构的
转换规则，以及组织 Part 与绘制分组的关系是否已在公开操作文档中充分说明。
后续应为“创建组织容器会改变哪些 canonical 元数据”提供明确契约与示例。

## 已确认的网格工具能力缺口

本节结论限定于本轮检查的 `zipzi-python2` 公开声明、文档和调用路径，
不据此判断 SDK、Runtime 或 Editor 内部是否已有可复用算法。

### 没有发现可直接调用的轮廓感知自动布网入口

当前公开建模命令提供 `ModelingCommandRemeshRectangleGrid`，其参数为
`id/columns/rows`，没有 Alpha 阈值、轮廓留白、目标边长、局部密度或特征约束参数。
本轮核对的命令清单和参考文档中没有找到直接执行 Alpha 轮廓自动布网的其他入口。
因此已确认的缺口是缺少本任务所需的现成公开工具，不是无法提交任意拓扑。

现有 `ModelingCommandManualMesh` 可插点、移点、删除、约束边及重新连接；
`ModelingCommandReplaceTopology` 可以提交包含几何、绑定和顶点映射的完整替换。
这些能力可以支持自建布网流程，不能笼统记为“不支持手工拓扑”或“不能修复”。

证据：[公开建模命令与拓扑类型](../../../crates/zipzi-python2/client/python/zipzi/_generated/modeling.py)。
建议补充轮廓感知布网入口，或提供从图像轮廓、特征点到受约束拓扑的公开工作流。

### 内置采样诊断不覆盖动画网格质量

`sampling.Checks` 提供最小三角形面积和模型边界条件；
`DiagnosticKind` 当前只有 `SmallTriangle/InconsistentWinding/OutsideModelBounds`。
本轮 AngleX 在 −30 到 +30 每 5 单位采样，共 13 个点，诊断为空。
这个结果只证明这些采样点未报告相应问题，不能证明整个参数区间无问题，
也不能证明拓扑适合动画。

公开诊断没有直接报告长细三角形、最小角、轮廓误差、透明区域覆盖浪费、
跨关键形面积与边长拉伸等指标。上述指标可以从公开几何与图像自建计算，
不能把“没有内置报告”写成“无法检查”。

证据：[`sampling.Checks/DiagnosticKind`](../../../crates/zipzi-python2/client/python/zipzi/_generated/sampling_values.py)。
建议提供几何有效性与动画质量分开的报告，并明确检查坐标空间及采样范围。

### GPU 观察接口缺少拓扑检查图层

`observation.View` 对应的公开 `ObservationView` 仅接受输出尺寸、ROI 和 padding，
没有线框、顶点编号、约束边或变形热图选项。
本轮成品预览能验证外观和姿态差异，但容易掩盖布点与连边问题。

可通过读取几何后在客户端绘制线框图；这属于可行的替代方式，
不是当前 GPU 观察接口已经具备该能力。
建议提供与同一 `Source` 和视图映射绑定的拓扑叠加输出。

证据：[`ObservationView`](../../../crates/zipzi-python2/client/python/zipzi/_generated/modeling.py)。

### 本轮网格质量问题与处理责任

AngleX 制作选择了均匀矩形细分：23 个头部网格合计 1430 顶点、2212 三角形，
未保存显式 `meshing` 轮廓和结构约束元数据。
两层卷毛图像分别约 89% 和 87% 的像素 Alpha 小于 16，均匀铺网浪费明显；
该比例是图像像素统计，不能替代透明区域中的顶点或三角形数量统计。
嘴部网格也出现长细三角形。

选择矩形细分且只以有限几何诊断和成品预览作为检查，属于本轮制作流程不足。
工具缺口应提出，但不应把已完成三个姿态等同于网格质量合格，
也不能把网格质量问题全部归因于 API。
后续修复需要结合轮廓布点、局部密度与特征约束，并重新验证三个关键姿态及中间插值。

## 其他待核实项

| 项目 | 当前证据 | 需要补充的核验 |
| --- | --- | --- |
| 活动 Editor 文档的发现契约 | 当时三个会话中只有 Shirousagi 返回非空 `editor_context()`，据此选中了会话 4；公开说明只承诺可选 Editor 上下文 | 切换活动文档、多个窗口及后台文档时读取结果；核对是否有直接活动会话入口。不能承诺“非空即唯一活动文档” |
| 任意重新布网后的关键形自动迁移 | 矩形重网格已有绑定迁移测试；手工拓扑文档承诺保留依赖，完整拓扑替换允许提交 replacement binding 和 vertex mapping | 验证轮廓重新布网时各关键形、BlendShape、Glue 和身份的迁移能力及边界；不能记为全部不支持迁移 |
| 底层轮廓布网与质量算法的可复用性 | 本轮没有发现 Python 公开入口 | 如进入实现阶段，再按授权范围核对 Rust SDK、Runtime、Editor；当前不推断底层算法不存在 |
| 坐标空间说明是否足以完成复杂换父级 | 公开类型区分模型、Rotation-local 与 Warp-parameter 空间，曾通过示例理解其差异 | 验证只依靠公开帮助能否正确完成非单位画布、Warp 换父级和关键形坐标转换；尚无坐标换算错误证据 |

## 宿主断连待排查

用户提供的另一轮反馈中，首次 `modeling.records()` 调用发生宿主断连；
重新连接后，通过 collection 分页及 `meshes.get_many()` 成功完成结构读取，
并报告各读取结果的 `Source` 一致。

该次异常、宿主日志及进程退出情况尚未核对，断连原因未确定。
后续排查需要保留具体请求参数、异常类型与消息、连接及构建身份、宿主日志，
并区分进程退出、连接关闭和协议或预算错误。
不能从其他查询成功推出 `records()` 有缺陷，也不能据此排除相关问题。

## 模型状态与证据边界

用户反馈中的 Shirousagi 为初始结构：24 个网格，12 个启用、12 个禁用，
每个网格 4 顶点、2 三角形，共 96 顶点、48 三角形；没有参数、变形器或 Part。
该结构与本聊天编辑前的读取一致，但不是编辑完成后的状态。

本聊天后续完成了语义重命名、14 个 Part 的组织和 AngleX 三关键姿态，
其中 23 个头部网格进行了细分并建立绑定。
因此不同轮次的模型统计必须关联各自的 `Source`，不能混用初始与编辑后数据，
也不能仅凭相同项目路径判断读取的是同一版本。

基础结构读取可以沿 `describe()` 完成。此前操作中选择查询源码，
不构成“必须查询源码才能完成基础读取”的证据。
本记录补充后续发现体验反馈，不改写既有 P7 的历史验收结果。
