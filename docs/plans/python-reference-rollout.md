# Python generated reference 质量与去重实施计划

日期：2026-11-02。状态：P0 已实施并通过试点验收；P1–P7 结构和各批已列出的行为证据已完成，跨领域语义复核及 P8 最终验收仍在进行。详见第 10–18 节。

目标是让读者从一个公开入口判断如何调用、会产生什么结果和有哪些约束，同时减少重复维护和重复阅读。
类型摘要缺口清零是完整性门禁，不是文档质量的替代指标；不把当前样板的长段落直接推广到全部 API。
实施顺序是 P0（去重模型与小范围试点）→ P1 → P2 → P3 → P4 → P5 → P6 → P7 → P8。
P0 先证明精简后仍能完成阅读任务，再扩展内容；不先补完 851 个摘要才处理版式和重复。
每批提交都应包含源说明、重新生成的产物和对应验证证据；下面的阶段不是已完成记录。

## 1. 当前边界与基线

“样板已完成”指 `Session/Edit + meshes` 常用便捷接口的说明、签名和导航已改善，
不代表完整的 Session/Edit、`modeling.mesh` 或其他领域都已完成内容验收。
例如 `Session.modeling`、`Buffer`、`PlanCost`、Manual Mesh 操作等仍有类型摘要缺口。
`parameters`、`bindings` 等已有部分字段说明，也不能只据此标记整个领域完成。

已具备的公共能力：

- Rust 的 `ApiShape`、`ApiFields`、`api_record!`、`api_union!` 传递类型、字段和分支注释。
- Python 属性、构造参数、默认值、枚举和类型别名有独立的提取/展示方式。
- `DocEntry.type_refs` 从注解和导入关系解析，catalog 与 reference 共用。
  外部类型每页集中声明一次；同页类型使用已有 API 标题。
- `zipzi.describe(path)` 支持离线查询；类型引用保留完整公共路径。
- 公共 owned-value 行为在 `reference/common.md` 集中说明，类型没有摘要时保留显式缺口。
- `docs/type-documentation.json` 已生成类型说明覆盖报告；`generate.py --check-docs` 提供严格检查。
- 独立的 `doc_semantics.py` 已移除；手写的 `mesh-workflow.md` 及其专用测试已移除。

2026-11-02 工作区内生成报告的静态快照如下；它包含尚未提交的工作区改动，不能当作 HEAD 的测试结果。

| 指标 | 数量/含义 |
| --- | --- |
| catalog 领域分片 | 44 个，另有 index/common 等支持文件 |
| reference 页面 | 45 个领域/公共页面，另有入口 `reference.md` |
| 文档类型身份 | 991 个；公开别名按同一个定义归并，Python 特殊投影仍可能是独立定义 |
| 已有类型摘要 | 140 个；表示非空，不等于人工语义审阅通过 |
| 缺失类型摘要 | 851 个，其中 `source` 为 Rust 的 835 个、Python 的 16 个 |
| reference 正文总字符数 | 550,613；统计 `reference/*.md`，含 common，不含入口 `reference.md` |
| 固定 discriminator 通用说明 | 93 处 `Fixed operation discriminator supplied by this value type...` |
| 占位调用示例 | 7 处 `using the arguments shown in its signature` |

类型覆盖数字来源是 [type-documentation.json](../../crates/zipzi-python2/client/python/zipzi/docs/type-documentation.json)，
不是通过统计 placeholder 文本推算。体积和重复次数来自当前生成 Markdown 的静态统计。
实施时重新生成并读取报告；不把本表数字写成测试固定预期。该报告目前不证明枚举成员、方法参数和错误说明都完整。

已核对的冗余来源：

- `generate_api.py` 给每个集合读写方法展开 Source、恢复、释放等通用规则；Session/Edit 入口又各展开一遍。
- `codegen/python_api.py` 为 operation discriminator 合成同一句字段说明，reference 在各类型下重复显示。
- `reference-intros/meshes.md` 和 `runtime_edit.md` 手写了坐标、所有权和提交规则，与源定义重复维护。
- `example` 默认指向 `docs/recipes.py`，Markdown 特别隐藏这个值，离线 `format()` 仍会显示；它并非条目专属示例。

因此应同时处理说明来源、共享文档结构和不同消费端的呈现，不能只对 Markdown 做字符串删减。

## 2. 不变的维护原则

| 内容 | 唯一维护位置 | 如何进入产物 |
| --- | --- | --- |
| 跨语言业务语义、单位、坐标空间、默认值、约束 | 对应 Rust contract 声明的 `///` 注释 | 编译导出 → 类型图 → docstring/catalog/reference |
| 手写 Python API 的调用、返回和生命周期 | API 自身 docstring | AST 提取 → 文档模型 |
| 生成的 Python 便捷接口特有的参数组织、返回投影和调用示例 | 定义该接口的生成描述符，例如 `generate_api.py` 中 `WRITES` 的同一项 | 同一次生成写入 docstring/catalog/reference |
| 不可变值、集合读写、Job/恢复等公共规则 | 对应实现/生成模板旁的说明；`CompactValue` 只负责其实际拥有的值行为 | 用明确规则引用生成公共章节，并由消费端选择展开方式 |
| 页面导航/短介绍 | 模块说明或已有 `docs/reference-intros/` 片段 | 只组织入口和阅读顺序；业务规则引用生成条目，不另写一份 |

具体约束：

1. 不直接编辑 `_generated/`、`modeling/`、schema、catalog 或生成的 reference。
2. 不重新引入按 Python 名称匹配业务说明的独立表。已有描述符可以携带该 API 的 Python 用法，
   公共操作含义从它绑定的 contract 类型/分支获取，不能靠 `Create` 等短名跨领域匹配。
3. 不用类名改写、字段列表或“immutable value”套话充当类型摘要。真实缺失进入报告。
   不按文本相似度自动合并语义；同一 Rust 类型的不同 Python 投影可能有不同构造方式和返回形状。
4. 本轮不新增独立手写 workflow、recipes 或领域教程来补 generated reference 的缺口。
   已有 guide/recipes 可以保留和用于回归，但不计作本轮 generated 内容的完成证据。
5. 先在源定义旁补说明、必要的结构信息及小型示例。若仍无法表达，再记录具体 API、
   失败任务、现有输出和尝试过的生成方案，另行评审是否需要手写文档。
6. 不以扩充内容为由改变业务行为、运行时资源策略、公开类型含义或 wire protocol。
   发现实现与预期冲突时，先明确当前真实行为；需要改变行为的工作单独立项。

### 2.1 条目保留什么，哪些内容共享

“一个维护源”不等于“所有消费端只出现一次”。单条离线帮助需要完整，整页 reference 需要紧凑。
三者共享事实和来源关系，不要求 Markdown、docstring 与 `format()` 的文本逐字相同。

| 内容 | 条目内必须保留 | 可共享的内容 |
| --- | --- | --- |
| 类型/构造 | 业务摘要、真实签名、字段特有的单位/空间/身份/省略语义 | owned-value、集合复制及无 IO 等公共规则 |
| 方法 | 作用、返回投影、Session 正式源或 Edit 候选源、发布/文件 IO 效果、同步或 Job、释放责任、特有失败条件 | 完整的恢复流程、通用准入、相同集合操作的公共约定 |
| enum/union | 分支选择条件、业务效果、分支字段差异 | discriminator 的只读/自动提供规则；不能用 wire 值替代业务说明 |
| 真正的别名 | 可定位的公开路径和到定义的链接 | 定义正文只展示一次；特殊投影保留完整差异说明 |
| 导航/示例 | 入口链接、必要前提和能解释调用的最小片段 | 模块入口集中链接 guide/recipes；无专属示例时条目的 `example` 为空 |

具体呈现规则：

1. 类型摘要解释用途，不重复完整字段列表。签名已有默认值时，字段说明解释默认行为和边界，
   不再逐字复述类型声明。单位、坐标空间、`None`/空集合等容易误用的信息仍留在对应字段旁。
2. Markdown 每页每条共享规则最多展开一次；已在公共章节定义的规则使用有语义的链接。
   方法保留短的效果/风险提示，例如“仅改候选，普通写失败会中止 Edit；恢复规则见……”，不能只剩“见公共说明”。
3. Session/Edit 同一实现的方法保留各自标题、签名和 Source 语境，不机械改成别名。
   Edit 的例子使用 Edit 读取候选，不能沿用 `session.*` 示例让读者误读正式内容。
4. 固定 discriminator 根据生成投影角色识别，显示固定值/只读属性并引用一次公共规则；
   不根据字段名 `kind` 全局折叠，业务枚举字段仍需完整语义。公开字段在 catalog/离线帮助中仍可发现。
5. 外部类型声明继续每页去重，但在使用处提供可定位的类型导航；不只把全部类型堆在页首。
   保留现有准确签名，可在其旁生成类型链接；无需为此手工重写签名或复制类型正文。
6. `describe(path)` 仍精确查询单个入口。`format()` 保持完整帮助，只展开该条目直接引用的有限公共规则，
   每条一次，不递归展开 children/type_refs。docstring 同样保留调用所需信息，不能依赖 Markdown 链接才能理解。
   catalog 以引用存储公共说明；不得为一次查询遍历全部分片、启动 host 或加载 native/GPU。

## 3. 每个领域的完成标准

领域只有同时满足以下条件才能标记完成：

- **范围完整**：登记该领域全部公开路径，包含便捷入口、canonical modeling 类型、
  enum/union 分支、别名及 Session/Edit/Preview 上的相关入口。新增导出必须进入范围检查。
- **摘要有用**：类型说明回答适用的问题：“它表示什么、在哪里使用、哪些上下文决定其含义”；
  简单类型可以一句话说清，不强制给每项套同样的多段模板。
  有文档的同义别名引用同一定义；特殊投影要说明其差异。
- **字段可解释**：单位/空间/顺序/身份、`None`/空集合/省略、默认值和保留语义有明确说明。
  参数的位置数组与稳定 ID 的区别、分页偏移与业务 ID 的区别不能靠读者猜测。
- **分支完整**：枚举成员说明取值含义；union 说明选择条件和各分支的业务效果。
  只有 wire 字符串不算成员语义完整。
- **调用可判断**：方法说明明确必要参数、返回形状、读取的 Source、候选/发布/文件 IO 效果、
  同步或 Job、需要释放的对象，以及与该方法有关的失败/恢复行为。
  公共规则链接一次，各条目突出差异，避免完整复制所有生命周期段落。
- **生成可追踪**：修改一处源注释或 Python 描述符，相关 docstring、catalog、reference 同步更新；
  同名跨领域类型不得串用说明。别名、共享规则和投影差异有明确来源，不能靠展示文本反推关系。
- **导航可靠**：`type_refs` 指向公开类型；导入别名、字符串注解、`TYPE_CHECKING`、泛型和 union 可解析；
  外部类型链接声明每页一次、使用处名称无歧义、Markdown 链接和锚点有效。
- **验证有证据**：该领域缺口清零、生成一致性通过、与语义主张对应的测试通过。
  仅有非空字符串、测试数量或“某个示例能跑”不能代替这些条件。
- **阅读有增益**：对相同公开入口比较修改前后；签名和字段约束仍可查，重复的规则正文已收敛。
  审阅者仅用生成页面和离线帮助，就能回答本批验收问题；不得借助实现源码补全答案。

示例只在能解释实际调用时添加。去掉生成的 `foo(...)` 占位示例；完整签名已足够说明简单构造时无需另写示例。
需要执行验证的片段放在 API 定义/docstring/描述符旁，由测试读取同一份片段，
通过 fixture 提供 Runtime、资源和临时路径，并声明 Local/Connected/GPU 前提，不再复制到手写 workflow。
先验证试点确实需要的片段；不要把建设通用示例框架变成去重前置条件，也不要只在 Markdown renderer 拼接代码。

## 4. 全领域分批范围

下列名称是 `docs/reference/<名称>.md` / catalog 分片的名称，不是新增的 Python 模块名。
当前 44 个分片在表中恰好分配一次；新增或调整路由时同步更新范围，不锁死分片总数。
跨批入口仍可被多批验证，例如 `Session.project` 属于 runtime_session 页面，
但其项目 IO 语义在 P5 验收；必须在验收记录中列出这种跨页范围。

| 批次 | 主负责分片 | 验收对象 |
| --- | --- | --- |
| S：保持样板 | `meshes` | 创建、读取、改名、位置写入；以后每批都保留回归 |
| P1：公共类型与运行时 | `runtime`, `runtime_session`, `runtime_edit`, `modeling`, `modeling_common`, `modeling_runtime`, `ops`, `modeling_commands`, `modeling_queries`, `events` | Runtime/Session/Edit/Job、结果、错误、查询、事件与通用命令 |
| P2：参数与绑定 | `parameters`, `bindings`, `modeling_parameters`, `modeling_bindings`, `modeling_bindings_edits` | 便捷操作与完整轴/样本/网格编辑类型 |
| P3：资源、场景与完整网格 | `resources`, `scene`, `modeling_resources`, `modeling_scene`, `modeling_scene_edits`, `modeling_mesh` | 图像/lease、场景层级与变形、Manual Mesh；补齐样板未覆盖的网格功能 |
| P4：引用、计划与删除审阅 | `references`, `modeling_addressing`, `modeling_addressing_kinds`, `modeling_addressing_patches`, `modeling_addressing_selectors`, `modeling_deletion`, `modeling_deletion_decisions` | 地址/能力、Plan 生命周期、依赖清理与分页审阅 |
| P5：项目与格式 | `modeling_project` | 项目开存、导入导出、格式诊断及跨页的 Project 方法 |
| P6：动画与 CPU 采样 | `animation`, `sampling`, `modeling_motion`, `modeling_expression`, `modeling_physics`, `modeling_pose`, `modeling_preview`, `modeling_sampling` | 动画资产/编辑、Preview、物理与表达式、采样结果 |
| P7：空间、观察及可选服务 | `spatial`, `observation`, `inspection`, `mesh_generation`, `modeling_observation`, `modeling_spatial` | CPU 几何查询、GPU 捕获、客户端产物保存和 Editor mesh generation |

## 5. P0：先验证去重效果，再准备推广门禁

输入：现有 `type-documentation.json`、`DocEntry`、类型引用解析器和生成测试。
输出：精简但语义完整的试点产物、按领域验收的缺口清单和结构门禁，不补一张新的业务说明表。

### P0a：最小共享结构与试点

选取 Session/Edit 的 meshes 读写、parameters.patch、一个 enum、一个 union 和一个公开别名，
覆盖共享生命周期、`None` 保留、分支语义及投影差异；不是要求提前验收整个 runtime 或 parameters 分片。

1. 先对这些入口记录原文、重复规则、必须保留的事实和来源。清理导航介绍中的业务正文，
   去掉伪示例和无条件的 recipes 绑定；源注释缺失继续如实报告。
2. 仅为试点需要扩展 `DocEntry`：增加有明确来源的公共规则引用、必要的投影/字段展示角色。
   规则由实际集合模板、生命周期实现或现有投影声明绑定；沿用已有 alias、`type_refs` 和字段提取。
   不能从 `Create` 等短名、相同句子或字段名猜测共享关系，也不建设通用文档 DSL。
3. 同步更新 catalog TypedDict、lookup、`ApiDescription`、`format()`、docstring 和 Markdown 消费端；
   公共规则可以进入 `reference/common.md` 的独立章节，但不能把生命周期规则归到 `CompactValue` 名下。
   生成顺序先取得结构化来源，再渲染；不可先展开长 docstring 后靠字符串替换删重复。
4. 共享关系/诊断使用稳定的源身份（contract 类型/分支、Python 定义及投影），不能把运行期 `id(node)`
   写进报告或用作跨次生成的验收键。保留当前别名完整性检查，避免去重吞掉投影差异。
5. 比较三个消费端：正式/候选 Source 能否区分，patch 省略是否保留，返回是否为 ID/WriteResult/Job，
   普通失败和 OutcomeUnknown 如何处理。证明信息保留后再将规则用于其他领域。

试点退出条件：占位示例为零；别名不重复定义正文；公共规则无同页重复展开；
相同入口的通用说明重复量下降且所需事实保留；本批修改的条目无新缺口；单条离线查询仍按需加载。
比较范围必须包含搬出的公共章节和新增导航，不能把文本移到另一页就宣称总体精简。
不设总字符数压缩百分比，也不把每个类型、方法都压成固定行数。

### P0b：范围、质量与回归门禁

1. 复用现有报告中的 `paths`、`source` 和类型身份分组，建立本计划的领域范围检查。
   可在 tooling 测试中维护“已验收分片”集合；这里只存验收范围，不存说明文本。
   页路由提取成 renderer/检查器共享函数，避免再复制一份 `modeling_*` 拆页规则。
   `modeling` 根分片不能通过 `zipzi.modeling.*` 前缀筛选，否则会误包含全部 canonical 领域。
2. 当前 `--check-docs` 是**全局**严格检查，不能当作每批已能通过的门禁。
   阶段内通过测试筛出本批和已验收分片涉及的 missing 项，要求为零；未处理领域保持报告可见。
   最后 P8 才要求全局严格检查通过。不要用无范围的临时豁免绕开缺口。
   以稳定身份记录现存缺口集合，新增/修改的公开条目不得增加缺口，已消除的缺口不得回退；
   不能只比较 missing 总数，否则一个类型补齐可能掩盖另一个类型退化。基线只记录债务，不存业务说明。
3. 扩充检查：空摘要、已知占位模式、缺失分支语义、悬空公共类型引用、重复/歧义的 Markdown 引用声明。
   对 builtin、typing 容器和类型变量明确分类；未解析的 SDK 类型应进入诊断，不能全部静默忽略。
   文本长度和字数不作为语义正确性的替代指标。
4. 用少量代表用例验证来源链路：类型摘要、分支说明、字段约束、Python 描述符及公共规则各改一处，
   检查相关消费端同步且无关同名操作不变。新增领域复用这些门禁，仅为新的投影/例外增补用例，
   不为每个类型添加镜像测试，也不把整页文字快照当成语义测试。
5. 校验非空摘要报告的边界：`type-documentation.json` 目前不逐项审计 enum 成员、callable 参数/返回说明。
   增加相应诊断或人工审阅清单；不能把类型摘要缺口为零解释为这些内容也完整。
6. 修复已有 graph 测试的基线问题：`test_python2_codegen_graph.py` 中固定的 311 个类型归属项
   与当前 331 个不符（这是 `graph.domains` 的条目数，不是文档领域数）。2026-11-02 审阅时已复现。
   将断言改为与非 opaque 定义集合/归属一致，
   保留明确的关键类型归属断言，不能只把数字改成 331。
7. 增加三类可判定的去重门禁：已绑定公共规则每页最多一个正文、真正别名没有重复正文、
   通用 recipes 不自动挂到每个条目。未归类的重复段落只列为审阅线索，不按相似度自动删除。
   保留直接帮助语义和懒加载回归；共享规则缺失、引用循环或投影绑定错误必须失败。

P0 复用 exporter、`type_refs` 和类型覆盖报告；仅增加试点证明必要的结构与诊断。
枚举成员、参数/返回等诊断应区分“缺失”“不适用”和“待人工语义审阅”；
非空检测、规则引用计数和链接检查可以自动化，单位/失败语义是否准确仍需行为证据和人工审阅。

## 6. 各批实施任务

以下 Python 源路径相对 `crates/zipzi-python2/`，Rust 源路径相对 `crates/zipzi-contract/src/`。
共同涉及 `tools/generate_api.py`、`tools/generate_dtos.py`、`tools/generate_modeling.py` 及
`tools/codegen/{python_api,model,catalog_output,projections}.py`；只改确有投影差异的配置。

### P1：公共类型、运行时、事件与查询

**源位置**：Python `client/python/zipzi/{__init__,_lifecycle,_jobs,_errors,_recovery,_calls,_values,_results,events,_documentation}.py`；
Rust `runtime/{identity,limits_context,capabilities,sessions_events,modeling_commands}.rs`、
`authoring/{receipts,modeling_query,modeling_intents}.rs`、`model/basics.rs`。

**必须说明**：身份作用域、版本/序列/Source 的区别；Snapshot/Batch/Page 的顺序和空值；
cursor 的固定源与重启条件；Session attach/detach/close 与 Runtime close 的区别；
Edit 状态与 commit Job、JobTimeout/OutcomeUnknown/普通拒绝的区别；RecoveryAction 的适用范围；
事件流的保留窗口、resync、无跨 Session 全序；`Session.modeling` 的真实访问方式。
`ops`、ModelingCommand 和查询 union 要说明选择分支的方式及其读写/规划边界。

**验收**：沿 `connect/local → sessions → query/edit → job/recovery` 能从生成条目判断生命周期。
采用 P0 验证的公共规则引用；get/get_many/list、候选写入与恢复规则各有明确归属，
方法条目保留返回投影、Source 和失败差异，不再逐领域复制完整通用段落。
`runtime` 分片中的 Buffer、PlanCost、PlanReport 等共享类型也在本批补齐真实摘要，
不能将这些摘要缺口推迟到其他批次，导致本批分片检查一直无法通过。
它们与资源、规划及具体领域交互的行为还要分别在 P3/P4 复核。
摘要清零只是完成标准之一；有跨批行为待核对的分片标为“结构通过、语义待复核”，
已有结构门禁继续执行，不能提前计为领域完成。

**行为证据**：已有 `P3ADiscovery` 和 `P3ARecovery` 的命名用例；事件用例
`PythonSdk.test_event_subscriptions_are_paged_immutable_independent_and_source_exact`。
按说明实际改动选择过滤，不因修改一个 docstring 重跑全部 SDK。

### P2：参数与绑定

**源位置**：Rust `authoring/parameter_binding.rs`、`model/{parameters_transforms,glue_bindings,blendshapes}.rs`、
`document/{binding_grids,binding_reparent}.rs`；Python 的 Parameters/Bindings 生成描述符与 DTO 投影。

**必须说明**：稳定 ID 与 runtime_id、范围/默认值/精度/repeat；patch 的 `None` 保留语义；
轴顺序、键值组合与 positions 对应；ordinary 与 blend 的绝对值/增量区别；
轴/样本/键的身份与位置索引区别；网格增删/重采样/切片和迁移结果的含义。

**验收**：能从参数与绑定的生成条目推导一次候选参数写入、绑定读取和 CPU 采样的预期值，
便捷类型与 canonical 类型的说明一致，不能仅依赖便利层已有的字段文字。
优先复用 `test_typed_parameters_share_a_mixed_batch_and_preserve_unspecified_fields`、
`test_mesh_binding_cpu_sweep_uses_exact_candidate_and_formal_sources`、
`test_binding_failure_aborts_mixed_batch_and_noop_preserves_redo`。

### P3：资源、场景与完整网格

**源位置**：Rust `runtime/edit_resource_operations.rs`、`document/{resource_edits,organization,scene,scene_content,scene_content_values}.rs`、
`authoring/{scene_specs,scene_operations,scene_content_specs}.rs`、`model/{appearance,scene_keyforms_meshes,deformer_original,offscreens_scene_bindings}.rs`、`warp_bezier.rs`；
Python `resources.py`、`buffers.py`、`_owned.py`、资源/scene 生成描述符。

**必须说明**：PreparedImage、ImageLease、Buffer 的创建者与释放者，替换后的旧快照；
RGBA 布局/alpha/行方向与字节预算；Part 组织层级与变形父子关系；重挂载与保形选择；
Warp/Rotation/Glue/Offscreen、附件与原始形状；Manual Mesh 的顶点身份、UV/源图坐标和依赖保留。

**验收**：资源输入、候选拥有者、已提交文档和独立 lease 的责任可分辨；
`modeling_mesh` 剩余操作不因 `meshes` 样板完成而漏掉。
优先复用 `test_replacing_image_retains_old_snapshots_and_exact_undo_redo_pixels`、
`test_reusable_buffers_large_images_and_captured_binary_pixels`、
`test_scene_organization_plan_and_metadata_recipe`、`test_manual_mesh_source_contract_and_existing_large_writes`。

### P4：引用、计划、地址和依赖删除

**源位置**：Python `references.py`、`_plans.py`、`_planning.py`；Rust
`runtime/addressing.rs`、`authoring/{capabilities,deletion_review,binding_deletion}.rs`、
`document/{deletion_requests,deletion_outcomes}.rs` 及 `scene_reference.rs`。

**必须说明**：ObjectRef、LabelAddress、FieldAddress 的 lease 和 incarnation/epoch；
能力结果不预留未来准入；Plan 针对哪个精确 Source，有效期、cost/loss/decisions、apply 与 close；
删除 roots 的真实范围、参数冻结切片、资源替换与未决选择；blocked/needs-confirmation 的处置；
审阅分页 offset 与固定快照；跨域原子性、Undo 后旧地址不能复活。

**验收**：生成条目能说明为什么 stale Plan 不能应用、为什么同 ID 的重建对象不是旧引用，
并区分“请求被阻止”“计划可审阅”和“已确认应用”。
复用 `test_sdk_references_addresses_and_undo_invalidation`、
`test_all_modeling_address_domains_keep_sdk_epochs_after_undo`、
`test_deletion_review_pages_cleanup_history_and_source_on_both_backends`、
`test_parameter_deletion_uses_frozen_default_and_retires_dimension_on_both_backends`。

### P5：项目 IO 和格式

**源位置**：Rust `project.rs`、`document/{model_references,display_info}.rs`；
Python `_lifecycle.Project` 及 project canonical 类型。

**必须说明**：save/open/replace/import/export/diagnose 的不同效果；路径在哪台主机解释；
格式枚举和输入资源选择；诊断、loss 和显式接受参数；文档历史、modified 与磁盘保存的关系；
Job 的返回报告及完成/持久化字段。区分项目侧 host 文件 IO 与 P7 inspection 的客户端文件 IO。

**验收**：读者能判断“提交成功但未保存”“导出受损/被拒绝”“项目替换完成”分别意味着什么。
复用 `test_headless_psd_build_save_replace_reopen_and_exports`、
`test_project_open_preserves_missing_resource_diagnostics`、
`test_p4_large_attachments_resource_relocation_and_package_roundtrip`。

### P6：动画资产、Preview 和 CPU sampling

**源位置**：Rust `animation.rs`、`physics.rs`、`physics_document.rs`、
`document/{motion,pose_motion_edits,state_and_pose}.rs`、`runtime/sampling_preview.rs`、
`authoring/sampling_quality.rs`；Python `animation.py`、`_preview.py`、`sampling.py` 及相关投影。

**必须说明**：Motion/Expression/Physics/Pose 的资产、注册、容器和稳定子身份；
各时间字段的单位、插值/混合/优先级和默认值；编辑数据与播放/Preview 状态的区别；
Preview 的冻结源、独立状态、seek/advance/sample；采样输入顺序、评估空间、quality 与诊断输出。
每个物理量按真实字段核实单位，不能把所有数值统一解释成秒或模型坐标。

**验收**：CPU 数值断言验证文档中的顺序、混合和冻结源规则，不以截图代替数值证据。
复用 `test_r5_animation_authoring_plans_addresses_and_sidecars`、
`test_r5_independent_previews_numeric_geometry_and_candidate_sources`、
`test_sampling_preserves_untranslated_rotation_basis_on_both_backends`。

### P7：空间、GPU observation、inspection 和 mesh generation

**源位置**：Rust `spatial.rs`、`runtime/{observation_textures,sampling_preview}.rs`；
Python `_spatial.py`、`_observation.py`、`inspection.py`、`mesh_generation.py`、`_mesh_workflow.py`。
最后一个是已有的 Python 实现模块，不是被移除的手写 Markdown workflow。

**必须说明**：CPU bounds/hit 的坐标与遮挡判断范围；GPU View/ROI/像素方向；
Original/FitBudget、estimate 与执行时准入的区别、纹理报告和释放计费；
inspection.save 的客户端路径和部分写入失败；mesh generation 的 Editor capability、alpha 方向、
生成结果与文档写入的分离、空间映射和 stale Source。

**验收**：CPU/离线说明与 GPU/Editor 前提分开。CPU 失败或 capability 缺失不能被当成 GPU 验收成功。
复用 `test_r5_spatial_frozen_sources_hidden_geometry_and_animated_numerics`、
`Observation.test_texture_policy_estimates_reports_and_source_alpha_on_both_backends`、
`Observation.test_r5_observation_frozen_pixels_animation_and_inspection`；
mesh generation 先运行已有无 Editor/GPU 测试，再按能力验证真实 Editor 服务。

## 7. 实施与验证命令

命令均从仓库根运行。以下都是当前已有入口；P0 的分领域审计、共享规则和示例消费端门禁
是待实现内容，尚无新的 CLI 参数。实现时将它们接入常规 tooling gate，不仅靠本计划的人工步骤运行。

每批修改源说明后：

```sh
uv run --locked python crates/zipzi-python2/tools/generate.py
uv run --locked python crates/zipzi-python2/tools/generate.py --check --no-cache
uv run --locked python tools/test_tools.py -p test_python2_codegen.py -k reference
```

涉及 P0 文档模型、catalog 或离线格式的修改，还要运行对应 model/alias/offline 用例；
不能假定 `-k reference` 会选中所有相关测试。仅审阅/修改本计划无需重新生成或构建 SDK。

按本批需要选择行为证据。例如：

```sh
# P1：离线发现与精确分页
./tools/test-sdk.sh python2 --discovery P3ADiscovery.test_offline_catalog_aliases_fields_and_cycle_detection
./tools/test-sdk.sh python2 --recovery P3ARecovery.test_timeout_is_known_pending_not_failure_cancel_or_unknown

# P2：同一用例包含 Local/Connected 验证
./tools/test-sdk.sh python2 PythonSdk.test_typed_parameters_share_a_mixed_batch_and_preserve_unspecified_fields

# P4：依赖清理的冻结报告与历史
./tools/test-sdk.sh python2 PythonSdk.test_deletion_review_pages_cleanup_history_and_source_on_both_backends

# P6：Preview 数值与冻结源
./tools/test-sdk.sh python2 PythonSdk.test_r5_independent_previews_numeric_geometry_and_candidate_sources

# P7：CPU 几何与可选服务的本地逻辑
./tools/test-sdk.sh python2 PythonSdk.test_r5_spatial_frozen_sources_hidden_geometry_and_animated_numerics
./tools/dev-python crates/zipzi-python2/tests/test_mesh_generation.py

# P7：有可用 GPU 时才执行
./tools/test-sdk.sh python2 --observation Observation.test_texture_policy_estimates_reports_and_source_alpha_on_both_backends
```

`python2` 的命名过滤默认交给 `test_api.py`；`--discovery`、`--recovery`、`--observation`
分别切换测试模块，不能把其他模块的类名直接传给默认入口。
SDK gate 同时执行静态类型检查；不另用全局 Python 或自行设置 PYTHONPATH 绕过环境。
仅修改文字且既有行为证据仍适用时，不需要为了每个条目重新构建 native。
修改生成模型、来源绑定、导出结构时，需运行相应生成测试；Rust metadata 变更再运行 contract 测试。

## 8. P8：最终收口

全领域完成后执行一次跨领域验收，避免每批重复昂贵的全套构建：

```sh
cargo test --locked -p zipzi-contract
uv run --locked python tools/test_tools.py -p test_python2_codegen.py
uv run --locked python tools/test_tools.py -p test_python2_codegen_graph.py
uv run --locked python tools/test_tools.py -p test_python2_codegen_outputs.py
uv run --locked python crates/zipzi-python2/tools/generate.py --check-docs --no-cache
./tools/test-sdk.sh python2
./tools/test-sdk.sh python2-connected
```

可选服务的最终证据还需 P7 的 GPU gate，以及对应 Editor 服务的验证。
真实 Editor 安装包检查使用已有 `python2-connected --editor-host <absolute-path>` 入口；
在记录中替换为实际可执行文件路径，不把 CPU fixture 成功解释为真实 Editor 服务成功。
普通离线文档查询必须在没有 native/Editor/GPU 的环境仍然工作。

最终验收要求：

1. 报告 missing 为零，实际导出的全部分片有归属和验收记录；enum 成员、callable 与链接检查也通过。
2. 同一业务含义有一个维护源；没有新增手写 workflow 或 detached semantics 名称表。
3. catalog、安装包中的 reference、docstring、`ApiDescription` 的语义和当前代码一致；
   共享规则按消费端的密度展开，公开路径、字段、分支和投影差异未因精简丢失。
4. 使用公开路径进行安装后离线查询；不会为 describe 或读文档启动 host/加载 GPU。
5. 占位示例和无条件 recipes 绑定为零；真正别名无重复定义正文；公共规则正文及类型链接声明按页去重。
   对同一范围记录正文字符数、公开入口数、重复规则展开数和人工审阅结论。
   公共章节计入比较，分别解释“新增必要语义”和“减少冗余”的变化；不以总字数下降代替质量验收。
   原始和精简页面均用相同阅读问题检验，离线 `format()` 也需要审阅。
6. 所有未执行的 GPU/Editor/平台门禁注明环境和剩余动作；对应服务领域保持待验收，不能宣称全领域完成。

## 9. 单批提交与交接模板

每批在本计划后追加一条验收记录，至少包含：

- 批次/分片及跨页入口；实际审阅过的类型、方法和别名范围。
- 修改的 contract 声明、Python docstring/生成描述符；发现的实现行为差异。
- 生成报告前后变化；本批缺口为零、已有缺口未回退的证据，其他未完成领域剩余量。
- 实际执行的命令与结果、复用的行为断言；未执行项目及原因。
- 同范围的文档体积及重复规则变化；搬出/新增内容的去向和原因；页面及离线查询能否回答本批验收问题。
- 状态分别记录结构门禁、语义审阅、行为证据；跨批或平台证据未完成时列明剩余项。

先提交通过试点的 P0，再从 P1/P2 开始；每批保留 P0 的信息完整性和去重门禁，
再扩大到场景、删除和动画等类型较多的领域。阶段定义不代表完成；实际进度以追加的验收记录为准。

## 10. P0 验收记录（2026-11-02）

状态：结构门禁通过；以下试点的页面与单条离线帮助语义审阅通过；CPU Local/Connected 行为证据通过。
未验收整个 runtime、parameters、modeling_mesh 或 spatial 领域，P1–P8 状态不变。
比较基线为开始实施时的 `e4ad1c35`；原文可从该版本重现，不把整页快照加入测试。

**范围与来源**：

- 已验收分片集合先登记 `meshes`；跨页精确路径见 tooling 测试 `PythonReference.PILOT_PATHS`。
  包含 Session/Edit.meshes 的入口及 get/get_many/list、Edit.meshes.create/rename/set_positions，
  Edit.parameters.patch、parameters.ParameterPatch/Patch，以及 ApiDescription/format。
- enum 为 `modeling.mesh.MeshConstraintKind`；公开别名为 `meshes.ConstraintKind`。
  union 为 `modeling.spatial.ObjectTarget` 的 Mesh/Part 分支，另检查公开别名 `spatial.Target`。
  这些分支已有 contract 说明，本批没有扩展空间查询业务。
- 规则分别维护在 `_facade_support.Collection`、`_lifecycle.Edit` 旁及 discriminator 投影模板旁。
  原文集合方法各含一份 “Through Session … Through Edit …” 段落；候选写方法各含完整失败/恢复段落；
  现在条目保留各自 Source、返回、同步、候选效果及失败短提示，完整规则只在 common 展开。
  docstring 保持完整；catalog 存直接规则身份；format 只展开该入口直接绑定的叶规则。
- `DocEntry` 增加规则引用和字段展示角色；身份改为稳定的 Python 定义或 Rust 类型/分支与投影身份。
  固定 discriminator 展示固定值和只读角色，ParameterPatch.kind 仍保留普通业务字段说明。
  外部类型使用处引用每页唯一声明，同页类型链接已有标题；Session/Edit 保留独立 Source 语境。
- Rust `authoring/parameter_binding.rs` 补 ParameterPatch、ParameterOperation 及其分支说明。
  同时纠正创建参数的 runtime_id 默认值：None/空字符串在创建时采用稳定 ID；
  依据 `zipzi-core/src/document/parameters.rs` 的实际行为和既有数值测试，没有改变执行行为或 wire。
  Python patch 用法及示例保留在同一 WRITES 描述符中；网格描述符继续绑定各自 contract 分支。

**覆盖与门禁**：

- 991 个类型身份不变。旧报告 140 个非空摘要、851 个缺口；新报告 112 个真实摘要、879 个缺口。
  原有 DTO 模板有 31 个 “Owned … value; reading fields …” 套话摘要；
  取消套话后，3 个改由本批真实源说明覆盖，另 28 个既有缺口被如实暴露。
  旧的 851 组缺口全部仍可见，试点及已验收 meshes 范围缺口为零。
- 新 `documentation-quality.json` 独立暴露 1,121 条 union 分支缺口、217 条 enum 成员缺口、
  37 条未解析 SDK 类型诊断及 877 条空摘要诊断；这些按定义/投影记账，不能理解为独立 Rust 类型数。
  callable 参数和返回另列待人工审阅/不适用状态，非空摘要不代替其验收。
- `tools/python/python-reference-debt.json` 只存 3,131 个身份/类别/成员键，不存业务说明。
  新增缺口失败；已修复键必须从基线移除，之后不能回退，不能用总数互相抵消。
  每个实际导出页面均在 S/P1–P7 中恰好归属一次；modeling 根页按共享路由精确筛选。
- 已加入缺失/重复规则、非叶规则（含循环）、错误投影绑定、缺失分支、未知 SDK 类型、
  Markdown 重复/歧义声明、真正别名重复正文及 recipes 默认绑定的门禁。
  graph 测试以非 opaque 定义及归属集合验证，不再固定 311/331。

**体积与阅读比较**：

比较包含 runtime、runtime_session、runtime_edit、meshes、parameters、modeling_mesh、
modeling_parameters、spatial、modeling_spatial 和 common 共 10 页，覆盖 268 个相同公开标题，
包括搬出的规则和新增类型导航。该范围正文字符数为 141,330 → 136,944。
runtime_session 为 31,880 → 26,835，runtime_edit 为 35,487 → 28,947，
common 为 1,584 → 3,609。

全库 reference 正文为 550,613 → 607,335；增加主要来自全领域使用处的可定位类型链接，
本批不据此宣称全库变短，也不把导航新增当作各领域已语义验收。
30 处正式/候选双语境长段落改为条目各自的 Source；10 处候选写入完整恢复段落改为 1 个公共正文。
原有 93 处 discriminator 通用字段套话归零，公共解释只展开一次，固定值仍留在各字段旁。
占位调用示例 7 → 0；catalog 的无条件 recipes 示例绑定 1,140 → 0，模块入口仍有 guide/recipes 导航。
真正别名均只展示定义导航，特殊投影保留正文。

页面及 format 可直接回答：Session 读取正式内容，Edit 读取自己的候选；patch 省略/None 保留；
create 返回 ID、rename/set_positions/patch 返回 WriteResult，显式 commit 返回需等待/关闭的 Job[Receipt]；
普通失败中止候选，Confirmed Busy/准入拒绝按 recovery_action 处理，OutcomeUnknown 恢复原 Call。
字段身份、坐标空间、顺序、默认值和分支选择仍在类型定义旁。

**实际验证**：

- `generate.py` 重新生成 351 个文件；`generate.py --check --no-cache` 通过。
- `uv run --locked python tools/test_tools.py -p test_python2_codegen.py -k Python2Codegen`：40 项通过；
  `-k reference`：17 项通过，含 8 项新增 P0 门禁。
  原有摘要/分支/字段来源变更测试和新增参数分支、描述符及公共规则变更测试覆盖三种消费端，
  并检查同名的其他领域操作不变。离线子进程只加载目标 catalog 分片及直接规则，不加载 native/GPU。
- `test_python2_codegen_graph.py`：8 项通过；`test_python2_codegen_outputs.py`：7 项通过；
  `cargo test --locked -p zipzi-contract`：16 项通过；pyright 正反例通过。
- `./tools/test-sdk.sh python2 PythonSdk.test_generated_reference_pilot_examples_on_both_backends PythonSdk.test_typed_parameters_share_a_mixed_batch_and_preserve_unspecified_fields` 通过。
  新用例从实际 docstring 读取同一份示例，fixture 提供 Runtime/Geometry/参数，验证候选、正式内容及发布后的结果。
- `./tools/test-sdk.sh python2 --discovery P3ADiscovery.test_offline_catalog_aliases_fields_and_cycle_detection P3ADiscovery.test_candidate_cursor_undo_and_wrong_collection_fail_without_aborting_reads P3ADiscovery.test_recovered_page_keeps_captured_source_after_peer_publication` 通过。
- `./tools/test-sdk.sh python2 --recovery P3ARecovery.test_confirmed_busy_and_domain_failure_have_distinct_guidance_without_retry P3ARecovery.test_typed_helper_executes_public_write_once_and_retains_complete_projection` 通过。
- `git diff --check` 及受影响 Rust 文件的 rustfmt 检查通过。

未运行全局 `--check-docs` 成功门禁、完整 SDK、隔离 checkout 构建接受测试或真实 Editor/GPU 门禁；
本批复用既有 exporter，针对修改运行生成、contract、CPU 行为与离线检查。
全局摘要、分支、callable 和剩余未解析类型继续由 P1–P8 处理；Editor/GPU 的领域验收仍留在 P7/P8。

## 11. P1 中间记录：公共类型和命令/查询摘要（2026-11-02）

状态：**未完成 P1**。本次只完成摘要及部分分支来源补充；结构与语义均尚未整体验收，
不能把摘要为零当作领域完成。P2–P8 尚未开始。

范围为 P1 的 10 个分片，重点检查 `ModelingCommand`、`CollectionQuery` 各分支、
事件投影及其订阅、Buffer、Resources、EditState、JobState、RecoveryAction、
PlanCost/PlanReport、运行时能力和 EditorContext。Rust contract 的对应声明维护类型与分支说明；
Python 入口维护生命周期和投影说明，事件 DTO 生成器直接使用 contract 分支注释。
本次未更改运行时行为。资源和计划交互仍须在 P3/P4 复核。

生成报告中 P1 范围缺失摘要 95 → 0，全局缺失 879 → 784；P1 union 分支缺失诊断
136 → 0，稳定债务键只移除已解决项，无新增项。仍有 EditState 的 7 个成员缺失诊断和
6 个类型引用诊断；callable、事件保留窗口及无跨 Session 全序、恢复和生命周期的完整语义
仍须逐项审阅。原有 meshes 门禁继续执行。不能将自动非空检查视作人工语义验收。

相同 10 页公开标题均为 306 个，正文字符数 158,742 → 164,812；增加来自补充各分支和
类型的用途，公共规则结构未改变。这里没有声称总体去重或完成 P1 阅读验收。
离线帮助的全面人工审阅、发现与恢复的额外行为用例仍待执行。

已运行生成及无缓存一致性检查、reference tooling 门禁、`cargo test --locked -p zipzi-contract`，
以及 `./tools/test-sdk.sh python2 PythonSdk.test_event_subscriptions_are_paged_immutable_independent_and_source_exact`；
后者通过 Local/Connected 行为与静态类型检查。新来源链路用例覆盖事件投影、查询和命令分支。
后续应先解决剩余诊断并完成 P1 全部阅读问题和行为证据，再登记已验收范围。

## 12. P1 结构门禁与运行时阅读检查（2026-11-02）

状态：P1 **结构门禁通过、跨批语义待复核**，不计为完整领域验收；P3/P4 仍需核对
Buffer/资源与 Plan/删除交互，其他全部领域仍待实施。本次将 P1 十个分片加入独立的
`STRUCTURAL_PAGES` 门禁，不扩大表示语义验收的 `ACCEPTED_PAGES`。

`EditState` 的七个成员在定义旁各自记录状态，Python enum 提取器同时支持成员旁的说明与
Rust enum 分支说明。事件订阅入口记录独立有界窗口、`RESYNC_REQUIRED`、重新捕获但不回放，
以及不同 Session 和 registry 流没有共同序号。Modeling facade 的源描述符说明真实入口是
`session.modeling`/`edit.modeling`，分别读正式源/候选源。订阅由工厂创建，生成签名不再暴露
内部 Backend 参数。类型引用解析展开非公开 union 别名到实际公开分支；无法解析的分支仍报错，
而非把内部名字一概忽略。新增测试覆盖成员文义、异步规划命令导航和私有 union 的未知分支。

P1 摘要及 union 分支缺口继续为零，七个 enum 成员和六个类型引用诊断全部消除；全局类型
缺口仍为 784。基于稳定身份的债务基线移除 18 项，其中五项是其他页面上解析到内建 tuple
的 Point 别名；没有新增键。P1 同范围仍有 306 个标题，正文 164,812 → 171,413 字符，
主要增加公开命令分支的使用处链接；共享规则正文未新增重复展开。

阅读检查覆盖 Runtime/Session/Edit 的构造、attach/detach/close、正式和候选 Source、
Snapshot/Batch/Page 的排序和 continuation、commit Job、JobTimeout 与 OutcomeUnknown、
事件订阅及 Modeling facade；相应条目可直接给出入口、结果和恢复方向。
没有把这些检查扩展为所有跨领域条目的完整人工验收。

执行：`generate.py --check --no-cache`、reference tooling 门禁、graph 测试，以及
`./tools/test-sdk.sh python2 --discovery` 的离线 catalog、候选 cursor 和恢复页面三项用例；
`./tools/test-sdk.sh python2 --recovery` 的 JobTimeout 和 Busy/领域失败两项用例。
两个 SDK gate 均通过 Local/Connected 测试与静态类型检查。事件 Local/Connected 行为证据
见前一记录；尚未执行完整 SDK、GPU 或真实 Editor 服务门禁。

## 13. P2 参数与绑定结构和数值证据（2026-11-02）

状态：P2 **结构门禁及所列数值行为证据通过，完整语义审阅尚未结束**；不将自动非空检查
视作整批验收。仍需逐项审阅 binding grid/BlendShape 全部操作及其计划报告、特殊投影和离线
`format()`，并在 P3/P4 复核场景内容、Offscreen、重挂载和地址等跨领域类型。

本批覆盖 `parameters`、`bindings`、`modeling_parameters`、`modeling_bindings`、
`modeling_bindings_edits` 及跨页 Session/Edit 的参数和绑定方法。Rust 的 parameter/binding、
model、binding grid、reparent 和 scene-content 声明分别维护其类型、字段、分支语义；不新增
独立说明表。说明了稳定 ID 与 runtime_id、Normal/BlendShape、patch 的 None 保留、轴及样本
身份、输入数值坐标与返回样本 ID、普通形状的绝对位置和 BlendShape 的增量、完整笛卡尔
组合与第一轴最快的排序、零轴使用当前基础形状，以及 Slice 命中样本/Resample 在域内插值。
确认 ConstraintSample.weight 是 BlendShape 约束权重，修正原先误写的顶点插值说明。
共享集合读取模板明确查询失败不会中止 Edit；Session 和 Edit 各自的正式/候选 Source
仍分别呈现。另在 P1 的 `ModelingCommandUpdatePositions.positions` 修正乱序子集按传入
vertex_ids 配对的说明，未改变运行时行为。

报告中本批类型摘要缺口 105 → 0，union 分支诊断 114 → 0，enum 成员诊断 9 → 0；
全局类型缺口 784 → 678，稳定债务键移除 335 项，未新增。P2 已加入独立结构门禁；原有
P0/P1 结构门禁继续执行。比较本批五页、runtime_session、runtime_edit 和 common，
相同 233 个标题，正文字符数 114,294 → 121,491，新增主要是类型/分支解释及字段约束；
公共规则正文无新增重复，不以增加或减少字符数替代语义验收。

已执行 `generate.py --check --no-cache`、reference tooling、`cargo test --locked -p zipzi-contract`；
`./tools/test-sdk.sh python2` 的 typed parameter mixed batch、mesh binding CPU sweep、binding
failure/no-op 三个指定用例均通过 Local/Connected 和静态类型检查。CPU sweep 验证候选与正式源、
参数顺序、插值 0/5/10 及范围钳制；mesh grid 四个 SDK 集成用例验证 Slice/Resample、零轴
及轴变更的数值结果。`zipzi-core` 的 topology 用例增加并通过乱序顶点 ID 子集配对断言。

## 14. P3 资源、场景和完整网格结构检查（2026-11-02）

状态：P3 **结构门禁和指定 CPU 行为证据通过，完整语义审阅待复核**；不把非空检查
当作领域完成。仍需逐项审阅所有 scene/content 操作、变形保形范围和资源/计划生命周期，
并在 P4 核对地址、删除依赖，在 P5 核对附件 IO，在 P6/P7 核对采样和 GPU 交互。

范围为 `resources`、`scene`、`modeling_resources`、`modeling_scene`、
`modeling_scene_edits`、`modeling_mesh`，以及跨页 Runtime/Session/Edit 资源和网格入口。
Rust contract 的资源、scene、organization、content、deformer、Bezier、Manual Mesh 和附件
声明维护摘要及各分支说明；Python 的 `prepare_image` 说明字节布局、大小和租约归属。
scene convenience union 的摘要从绑定的 Rust union 生成，来源链测试验证更改会同时传播到
Python、catalog 和 reference。修正 Offscreen keyform 插入参数原先误写成顶点 ID，以及
SceneEdit 字段原先误写成参数标量/Edit 句柄的说明；未改变执行逻辑。

资源入口区分客户端 Buffer/PreparedImage、候选/文档持有者和独立 ImageLease；RGBA8 为紧密
排列、首行在上、straight alpha，Python 字节输入和 host 分别受预算约束。Manual Mesh 说明
稳定顶点 ID、UV 目标、约束、重连、精确候选 Source 及依赖形状转移；普通内容插入还区分
基底种子和 BlendShape 零位移、UV 只能修改 Base。场景条目区分组织/变形层级、保留局部
数据与按上下文保形，以及静态/采样 Part 到 Offscreen keyform 的索引映射。

本批类型摘要缺口 223 → 0，union 分支诊断 277 → 0，enum 成员诊断 30 → 0；全局摘要
缺口 678 → 455，稳定债务键移除 753 项，无新增。本批六页已纳入独立结构门禁。
包含这六页、runtime、runtime_session、runtime_edit 和 common 的相同 466 个标题，正文
223,252 → 238,194 字符，增加来自类型、分支和字段约束；公共规则正文无新增重复。
该比较不是对所有新增说明的语义完成证明。

执行无缓存生成检查、reference tooling、scene 生成器测试、`cargo test --locked -p zipzi-contract`；
四个计划列出的 Python 用例分别验证 Manual Mesh 原始 Source、可复用 Buffer 与大图、
图像替换后旧快照和 Undo/Redo 像素、场景组织计划，均通过 Local/Connected 及静态类型检查。
离线 `format()` 检查资源布局、普通绑定坐标及保留局部数据的重挂载，并确认无 native/GPU 导入。
未运行全量 SDK、GPU 或真实 Editor 服务门禁。

## 15. P4 引用、地址和删除审阅结构检查（2026-11-02）

状态：P4 **结构门禁与四项指定行为证据通过，完整语义审阅待复核**。仍需逐项审阅
所有删除关系/效果、跨领域选择及 `format()`，并在 P5–P7 核对格式诊断、动画与可选服务；
不能把类型非空及结构诊断为零解释成这些跨域行为已全部验收。

范围为 `references`、`modeling_addressing` 及 kinds/patches/selectors、
`modeling_deletion` 及 decisions，另含跨页 `Plan`、Session/Edit 的地址和规划入口。
Rust contract 的 addressing/capabilities、reference metadata、deletion requests/outcomes/review
是业务说明源；Python 引用与 Plan 自身维护客户端租约及分页用法。修正 AddressPatch 中将
scene/content/document 类型值误写成参数标量、以及引用容器 dissolve 目的地误写成泛化
collection 的字段说明。`references.ObjectKind` 的 Python 特殊投影从同一 Rust enum 获得
说明，并注明同时接受 canonical enum 和字符串；来源变更测试覆盖生成 Python、catalog、reference。

入口说明 ObjectRef、LabelAddress 和 FieldAddress 绑定客户端租约、incarnation/context/field
epoch，UUID 复用或 Undo 不能复活旧引用；Plan 针对精确 Source，report 冻结，apply 与 close
分离。能力结果不保留未来执行准入。删除条目区分根范围、冻结默认参数切片、资源替换、
待选项、阻止、可执行但需确认及实际应用；报告页 offset 是同一固定 section 的行位置，
不是业务 ID，close 后不可继续分页。

本批类型摘要缺口 199 → 0、union 分支诊断 294 → 0、enum 成员诊断 135 → 0；全局类型
缺口 455 → 256，稳定债务键移除 827 项，无新增。本批七页纳入独立结构门禁。
比较本批七页与 runtime、runtime_session、runtime_edit、common，相同 432 个标题，正文
194,244 → 211,594 字符，新增主要是类型、分支、字段及投影说明；公共规则正文未新增重复。
该数量不证明全部新增语义已通过人工审阅。

执行 `generate.py --check --no-cache`、reference tooling、`cargo test --locked -p zipzi-contract`，
以及计划列出的四个 Python 用例：引用/地址 Undo 失效、全部地址域 epoch、固定删除报告分页
及清理历史、参数删除冻结默认值。四项均通过 Local/Connected 和静态类型检查。
离线 `format()` 检查引用失效、删除分页和三种非 Ready 状态，并确认不加载 native/GPU。
未运行全量 SDK、GPU 或真实 Editor 服务门禁。

## 16. P5 项目 IO 与格式结构检查（2026-11-02）

状态：P5 **结构门禁及指定行为证据通过，格式语义仍待全面复核**。仍须对所有 sidecar
格式和拒绝场景逐项检查，并在 P6/P7 复核动画、客户端 inspection 文件 IO 和可选服务；
本记录不以非空摘要取代完整领域验收。

本批主范围为 `modeling_project`，跨页检查 `Runtime.sessions.open/import_`、
`Session.project.run/diagnose/replace/open/export/save` 和 `SdkFailure.project_report`。
Rust project、document metadata/CDI、model3 和 Editor state 声明维护类型、格式、分支和字段
说明；Python 生命周期方法维护 host 路径、Job 和报告解释。更正了原先不存在的
`ProjectReport.losses` 提示：应检查 diagnostics/warnings/published/durable，部分拒绝导出
在异常上提供 project_report。`durable=True` 在 Diagnose/New 也可出现而没有写盘；项目
持久化仍须检查独立 SaveReceipt。说明 CMO3/CAN3/Cubism 的 allow_loss 和 CAN3/Cubism 的
动画选项，以及裸 MOC3 的显式纹理槽；export 不将 Session 标记为已保存。

JSON opaque 扩展的递归 `ExtensionValue` 原本被导出字段引用但没有可导航的公开定义；
新增 `zipzi.modeling.ExtensionValue` 公开别名并在 Python 源旁说明不可变 JSON 值及数值边界，
不改变已有字段的类型含义。modeling_project 的 9 个未解析引用因此可链接到同一公开路径，
并以离线查询和生成测试检查解析，不静默豁免递归类型。

本批类型摘要缺口 54 → 0、union 分支诊断 56 → 0、enum 成员诊断 24 → 0、未解析类型
诊断 9 → 0；全局缺口 256 → 202，稳定债务键移除 214 项且无新增。新增公开别名使文档类型
身份 991 → 992，其摘要完整；modeling_project 已纳入独立结构门禁。比较该页与 modeling、
runtime、runtime_session、runtime_edit、common，标题 260 → 261，正文 139,235 → 144,393
字符；新增一个公开类型与必要的格式、字段和导航说明，公共规则未新增重复。

执行无缓存生成检查、reference tooling、graph 和 output 测试、`cargo test --locked -p
zipzi-contract`；计划列出的 PSD 导入/保存/替换/导出、缺失资源诊断和大附件迁移往返
均通过 Local/Connected 与静态类型检查。新增数值/状态断言确认 Diagnose 的 durable 标志
不表示保存，导出后 Session 仍 modified；新增无轴绑定的 CMO3 测试确认默认拒绝时返回
`EXPORT_LOSS` 和诊断报告，`allow_loss=True` 可发布且不改变历史。离线 format 检查
扩展类型及导出报告提示，未加载 native/GPU。

## 17. P6 动画、Preview 和 CPU 采样结构检查（2026-11-02）

状态：P6 **结构门禁与指定 CPU 数值行为证据通过，完整语义审阅待复核**。仍需逐项核实
所有物理量、资产编辑和播放组合，及其与 P7 observation/inspection 的交互；不把非空摘要
或 CPU 成功解释为 GPU/Editor 验收。

范围为 `animation`、`sampling`、`modeling_motion`、`modeling_expression`、
`modeling_physics`、`modeling_pose`、`modeling_preview`、`modeling_sampling`，以及
Session/Edit 的 Preview/采样入口。Rust contract 的 animation、motion、expression、Pose、
Physics、preview 和 sampling 声明维护类型与分支语义；Preview 方法本身说明绝对时钟时间以秒
计、冻结 Source、独立状态、seek/advance/sample、Job 与取消边界。补充普通曲线与
InverseStepped 的端点行为、Expression 混合方式、稳定 key/registration/entry 身份、
Physics 输入/输出百分比权重与各自的原生量纲、稳定 node ID 与导出索引区别、模型坐标与
绘制索引/顶点 ID 区别，以及采样质量相对同源默认姿态的限制。修正 motion unresolved target
误称 Physics、Physics 输出目标误称容器等字段说明，没有改变运行行为。

`sampling.RenderKind` 是将 Rust render-command union 投影为 Python enum 的特殊表示；其类与
成员说明由同一 contract union/分支生成，来源变更测试检查 Python、catalog、reference。
P6 类型摘要缺口 182 → 0、union 分支诊断 226 → 0、enum 成员诊断 12 → 0；全局缺口
202 → 20，稳定债务键移除 602 项、无新增。本批八页加入独立结构门禁。
比较这八页及 runtime_session、runtime_edit、common，相同 351 个标题，正文 172,323 →
181,923 字符，新增为分支、字段和导航说明；共享规则正文未新增重复。字符变化不证明
语义已全部验收。

已运行无缓存生成、reference tooling、`cargo test --locked -p zipzi-contract`；计划指定的
动画资产/地址/sidecar、独立 Preview 数值和候选源、旋转基底三项 Python 测试均通过
Local/Connected 与静态类型检查。离线 format 检查 Preview 秒制时间和 Motion 分段，且不
加载 native/GPU。尚未运行全量 SDK、GPU 或真实 Editor 服务门禁。

## 18. P7 结构与服务行为记录

本批覆盖 spatial、observation、inspection、mesh_generation 及 modeling 下的 spatial、
observation。补充 Rust 空间查询/结果、纹理预算和身份、GPU 图像与视图类型和分支说明，
分别说明模型坐标、几何而非像素遮挡、逻辑预算与设备检查、PNG 与 RGBA 身份、渲染结果的
坐标映射和纹理报告。已有 Python 入口说明保留客户端 inspection 文件 IO、部分写入失败、
Editor capability、alpha 方向、草稿与写入分离、仿射映射和 stale Source。新增来源变更
贯穿 catalog/reference 的测试，以及不加载 native/GPU 的公开路径 `format()` 断言。

P7 开始时全局缺少 20 个类型摘要、18 个分支诊断；本批结束后两者均为零，58 个稳定结构
债务键全部消除且没有新增债务。P1–P7 所有页面加入结构门禁；这不代表所有语义已经验收。
同范围六个 reference 页正文从 48,955 增至 50,294 字符，公开入口均为 91，规则展开数均为零；
增量是必要的类型和分支说明，没有新增独立语义表或 workflow。所审阅入口及离线 `format()`
可回答以上坐标、预算、服务及文件 IO 问题；跨领域完整字段和别名语义复核仍待 P8。

验证通过：`generate.py --check-docs --no-cache`；reference 生成测试 29 项；
`PythonSdk.test_r5_spatial_frozen_sources_hidden_geometry_and_animated_numerics`；
`./tools/dev-python crates/zipzi-python2/tests/test_mesh_generation.py`（8 项）；
GPU observation 两项指定测试（Local/Connected，含纹理预算、像素、动画和 inspection）；
当前源码 `cargo build --locked -p zipzi-editor` 后，使用独立临时端点运行
`./tools/dev-python crates/zipzi-python2/tests/editor_mesh_generation.py --host /Users/liangbohao/web/zipzi-2d/target/debug/zipzi-editor`，
实际验证五种算法、阶段、Edit 提交、参数验证及 Local 拒绝。`cargo test --locked -p zipzi-contract`
和 codegen graph/output 测试通过。结构门禁和所列行为证据通过；全套 SDK、安装客户端、
跨领域人工语义验收及真实 Editor 安装客户端 gate 继续在 P8 处理。

## 19. P8 安装包与全局检查进展

全局严格检查报告 992/992 类型摘要、结构诊断为零，所有分片有结构范围归属；
`cargo test --locked -p zipzi-contract`、codegen graph/output 测试、完整
`./tools/test-sdk.sh python2` 通过。`python2-connected --editor-host` 首次执行发现安装包
的 `MeshDraft` 公开模块无法解析类型注解，以及两个 reference-intros 源片段在安装包中的相对链接
失效；修复公开模块类型注解解析并让链接在源片段和生成页面均有效后，重跑安装 wheel 离线审计
（D01/D02/D04/D07/D08 均零）、21 项连接测试和真实 Editor 冷启动/保存/重开测试均通过。
`test_mesh_generation.py` 的类型注解回归及全部 8 项测试通过。

从计划基线 e4ad1c35 到本批前的 45 个 reference 页面（含 common），正文从 550,613 增至
676,477 字符，入口从 1,295 增至 1,296；固定 discriminator 重复句从 93 降为零，占位
示例从 7 降为零。增加的文字主要是原先缺失的业务、字段和分支语义，公共规则集中维护；
总字符数上升不是阅读质量的验收证明。完整 codegen tooling gate 及 P1–P7 跨领域人工语义
复核仍须结束后才能宣称 P8 完成；机器报告的 `review_required` 不是自动通过标记。

继续人工抽查时发现 Physics 序列化目标曾被错误描述为容器/排序字段，Physics 节点位置、
Motion/Expression 的复制源及事件目标也有过于泛化或不适用的描述。已按实际声明和 authoring
实现修正，增加生成 catalog 字段上下文回归；这说明结构清零不能替代语义审阅。安装类型注解
修复的静态检查还暴露未使用导入错误，调整公开模块绑定后 pyright 通过。此前完整 codegen
测试一次因运行期间修改源文件而报告生成漂移，另一次为修正上述问题主动中断；两次都不计作
最终通过，须在最终源码稳定后重跑。跨领域的其他字段、分支和调用语义仍待逐项复核。

复核 Motion/Expression/Pose 字段时，继续将曲线控制点、持久端点、事件字符串、表达式参数
贡献、组成员和接收目标从跨类型套话改为各自的真实含义。结构测试新增关键字段断言；具体
复核仍在继续，本记录不把全领域语义审阅标记为完成。

Physics 复制操作的 `group_id` 现按实现区分：复制 rig 时必须为 None，复制输入或输出时
必须指定目标 rig；复制节点需使用链操作。Pose Part 和 Motion PartOpacity 的目标说明也已区分，
并增加离线 catalog 字段回归。修改前的 reference 测试 30 项与 contract 测试通过；本次新增
字段断言通过，最终完整生成/SDK gate 仍须基于后续最终源码重跑。

## 20. P7 空间、观测及 inspection 页面语义验收

逐条复核 `inspection`、`spatial`、`modeling_spatial`、`observation`、
`modeling_observation` 的公开模块、类型/字段、分支和别名，以及跨页
`Session/Edit.modeling.{spatial,estimate_observation,observe}` 与 Preview 采样入口。
阅读基线及当前页面和离线 `format()` 时，以相同问题检查：几何查询使用什么坐标、隐藏对象
和遮挡如何处理；triangle 序号与顶点 ID 有什么区别；estimate 是否预留资源/验证设备；
纹理预算和身份报告代表什么；ROI 与图像像素如何映射；结果在哪里落盘、失败如何恢复。
当前页面能回答这些问题。三角形序号现明确是索引三元组序号而非 buffer 偏移；Original
纹理策略明确保留解码像素，overlay 空选区改为跨语言表述。源码、catalog、reference 和离线
帮助复用同一含义；别名只指向定义，已安装 wheel 的链接审计通过。

这五个页面在 e4ad1c35 到当前同范围正文从 36,128 增至 40,989 字符，均为 81 个入口；
增加的说明补齐类型/分支以及坐标和预算语义。规则正文没有在这些页面重复展开。CPU 空间
冻结源/隐藏几何测试、两项 GPU observation/inspection 测试（含 Local/Connected）均通过；
安装 wheel 离线审计通过。上述五页从结构通过提升为语义验收，范围不包含尚待逐项复核的
mesh_generation 或其他批次；P8 仍未完成。
