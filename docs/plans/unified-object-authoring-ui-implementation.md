# 对象交互 UI 实施记录

对应[UI 方案](unified-object-authoring-ui.md)。2026 年 10 月 23 日开始实施，当前 **U1 共享适配功能已收口，U2 的创建、树与 Parameter 导航、共享移动和保持形态改父级交互已接入；U3 Motion/Expression 创建、注册属性、成员、列表排序、Motion 复制/引用与删除影响审阅已接入；U4 Physics/Pose 根配置、业务组、稳定子项与结构管理已接入；U5-A 普通绑定与 U5-B 纹理资源闭环已接入**；U6 工程清理、回归与 Release CPU 复核已完成，后续 UI 由用户人工验收，U6 不标记为全部验收完成。

## 首轮实现：创建与输入边界

- `editor/src/authoring.rs` 统一对象操作的文档/工作区上下文、输入结算和结果检查。延迟菜单同时捕获 SDK 文档 revision，拒绝已删除再 Undo 恢复的旧目标；完成输入结算后才调用类型化 core planner。能力显示区分 Unsupported（不展示）与 Unavailable（禁用并保留原因），先接入 Physics 复制、删除和排序按钮。
- `editor/src/editing.rs` 从 Save 提取统一输入边界，接入 Parameter 管理、Physics 列表操作、Motion 创建、资源行激活、文档/工作区切换、Open/Import、New、GenerateMesh 和文档 Undo/Redo。合法文本提交，非法文本恢复并显示原因；数值、颜色、Timeline 与画布的未完成候选取消。候选取消先于 Redo 可用性查询；原生文本的快捷键和菜单路由继续优先。没有用全局暂停替代输入结算，因此纯展示操作不会因边界本身暂停普通播放。
- 结算后的字段值与 revision 同步回控件，阻止排队的 Blur/Change 或迟到 Release 重复提交。保存/关闭后台文档不清理当前文档的输入捕获。输入法组合未完成时阻止依赖输入结算的操作。
- 新建后通过 `OperationResult.suggested_selection` 检查结果、显示 Properties，并在延迟回调中复核文档、上下文、revision 和检查对象后聚焦名称。资源检查使用独立检查接口；共享适配不把重排伪造成一次预览点击。Parameter 的祖先展开和滚动沿用现有定位器。
- `zipzi-editor-core/src/authoring/creation.rs` 统一 Parameter/ParameterGroup 创建默认值、内部 ID、runtime ID 与稳定插入位置。名称可以由 UI 本地化；关系、名称校验、身份、锁和最终提交继续由 core/SDK 负责。
- Parameter 工具栏、命令和行菜单共用创建路径。普通参数之后插入；单个文件夹内追加；空组可嵌套；同容器多目标按完整文档顺序确定末尾，跨容器回到根集合。右键使用点击行上下文，延迟命令不重新读取全局选择。创建与后续名称编辑分开进入历史，Esc 不删除新对象。

## 阶段状态

| 阶段 | 当前状态 | 后续内容 |
| --- | --- | --- |
| U1 | 共享适配功能已收口：对象/集合能力映射、统一回执转换、检查/结果定位、输入边界与现有入口的重复业务判断清理完成 | U3–U5 新增入口继续复用；跨阶段原生/性能完整验收随 U6 |
| U2 | Parameter 与组织树创建、组织树新建项过滤显露；Parameter/文件夹/Part 组织树的 core 移动规划、跨组/嵌套、树多选拖放、过滤排序与拒绝原因已接入；树 Shift/Cmd 选择、焦点导航及可配置上下移已接入；组织树和 Parameter 可通过可搜索“移到…”选择指定容器；右键和快捷键支持多选分段上下移，变形树限同一实际父级；Parameter 文件夹单击检查、独立折叠与列表键盘导航已接入；场景及 Parameter 创建主菜单、场景可配置创建快捷键已接入；变形树 Mesh/Warp/Rotation 创建、双投影位置与显露已接入；保持形态审阅、可搜索变形目标、原子落点与后台取消已接入 | 原生截图和完整阶段验收待 U6 收口 |
| U3 | 接入 Motion/Expression 创建、空 MotionGroup、Clip/当前注册分区、继承/覆盖、Expression 成员、列表排序与可搜索移组；补齐 Motion 内容/组复制、添加已有引用、注册/组/资源删除与影响审阅；保留原激活路径 | 中英文原生截图、实际窗口与 Release 性能验收随 U6 |
| U4 | 接入 Physics/Pose 根配置、业务组与成员/links 管理，统一原生计划、创建命令、迁移/删除审阅；Physics 子项选择和节点参数复用按稳定 ID 解析，保留专用拓扑与执行顺序语义 | 中英文原生截图、实际窗口与 Release 性能完整验收随 U6 |
| U5 | U5-A 普通绑定和 U5-B Texture/ImageAsset 资源闭环已接入，详见下方交付记录及范围核对 | 高级功能按方案第 8.1 节暂缓，跨阶段原生/性能验收随 U6 |
| U6 | 工程清理、跨阶段回归、审阅列表修复与 Release CPU 对照已完成；历史截图和原生检查保留为辅助证据 | 用户确认跳过后续自动化 acceptance，UI 待人工验收，不标记为全部完成；见 [U6 验收记录](unified-object-authoring-u6-acceptance.md) |

### U5 范围调整

2026 年 10 月 23 日按用户确认更新[方案第 8.1 节](unified-object-authoring-ui.md#81-u5-调整后的范围与完成条件)。本次只调整方案，未开始 U5 产品实现。

- 必做顺序：U5-A 普通 MeshBinding/SceneBinding 的参数轴、关键值、Keyform 与画布形态编辑闭环；U5-B Texture/ImageAsset 创建/导入、Mesh 配置、内容替换、缺失修复及依赖删除闭环。
- 完成条件改为两个实际工作流可完成，保存重开、Undo/Redo、稳定身份、输入失效与资源异步结果正确；补齐相关检查和引用反馈，不再要求所有核心类型都有完整创建、复制、排序、删除 UI。
- BlendShape、Glue 专用绑定/配对、Offscreen 形态/映射、绘制与行为集合新增管理、CdiCombinedSet 主动创作、完整 Attachment/sound/UserData 流程暂缓。已有入口与数据保留，相关删除/替换仍检查真实依赖；文档字段和已有引用编辑仅按工作流补必要缺口。
- 收口时记录已接入、保留、尚未接入和核心不支持的区别。暂缓功能不自动转入 U6；U6 验收已交付范围与暂缓边界，不要求补齐这些高级入口。

## 首轮验证

Editor 与相邻 `zipzi-2d` 都在独立 worktree 中。依赖固定为 `zipzi-2d` 的已提交版本 `4bb2fee`；本轮没有修改该仓库产品源码，也没有使用原 checkout 的未提交更改。

```sh
# 受影响 UI、属性表单和输入路由
# 多个过滤词由测试 harness 取并集，不执行无关全套。
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --lib -- \
  headless_tests::authoring headless_tests::parameter_creation \
  headless_tests::parameter_groups headless_tests::parameter_management \
  headless_tests::parameters headless_tests::properties headless_tests::physics \
  headless_tests::animation_panels headless_tests::workspaces \
  headless_tests::command_targets headless_tests::input_routing \
  headless_tests::ui_performance properties::form::tests

# 创建规划和组织关系
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core \
  --test integration organization_authoring::

cargo fmt --all -- --check
git diff --check
```

新增回归覆盖创建后的名称选区、独立改名历史、命令来源与完整集合插入、合法/非法文本跨创建边界、Save 取消数值拖动与迟到 Release、候选存在时 Redo、折叠父组内创建空子组、工作区往返后的过期操作，以及核心嵌套/多目标/跨容器创建与原 ID Undo/Redo。

验证结果：受影响 UI / 表单 / 输入路由 / 虚拟化工作量共 **112 项通过**；组织创建核心回归 **7 项通过**；格式和空白检查通过。专项回归与此前运行有重叠，不重复累计。

本轮未运行昂贵全套、原生截图或新的 Release 性能测量。


## 第二轮实现：共享移动规划与交互

2026 年 10 月 23 日：依照用户要求，直接在已搬回的当前 checkout 继续，不再创建 worktree。本轮依赖 `zipzi-2d b86b13b`，没有修改该仓库。

- 新增 `zipzi-editor-core/src/authoring/moving.rs`：`PaletteMove` 捕获投影、源对象集合、稳定目标和 Before/After/Inside；规划转成现有 Organization/Scene SDK 计划。原生集合提供顺序，SDK 负责祖先归并、锁、循环、整体预校验、no-op、过期计划拒绝和单条历史。`plan_palette_step` 用同一适配生成菜单上下移的计划，按完整集合定位相邻项。
- `panels/sorting.rs` 移除 UI 的父级表、循环遍历和同父级/组嵌套限制。命中与释放使用同一 core 规划入口，释放时重规划并校验上下文；不再调用原来各自分散的排序 `EditorCommand`。旧公共命令仍保留给其他调用者，未宣称完成全部旧入口清理。
- 文件夹支持进入另一个文件夹，Before/After 支持合法跨父级移动；Part 组织移动可携带现有树多选集合。SDK 归并已选祖先的后代，并保持来源投影顺序；锁定成员导致整个计划失败。多选拖放不伪造单击，也不更改当前选择。
- 手动排序视图在搜索/类型过滤下仍可拖放，通过完整集合的稳定对象锚点确定位置，不清除筛选、不用可见行号写模型。保留现有虚拟列表、边缘滚动、Esc、失活和文档切换取消行为。
- 参数、文件夹和层级树行菜单增加单对象上移/下移。多选状态明确禁用这些单对象菜单项并说明使用拖放，避免只移动多选集合中的点击项；完整多选菜单与可配置快捷键尚未完成。
- 合法移入显示容器边框，排序显示插入线；无效落点在拖动预览中显示核心原因，锁定和循环原因提供中英文说明。600ms 连续悬停展开，离开后再进入重新计时；合法 Inside 落点在释放后立即展开目标。
- 一次拖动只缓存当前落点的规划，不为每个经过的行保留文档计划，也不在 begin 克隆整棵树建立父级表。重复悬停输入及重绘复用同一规划，释放后重新校验。

变形树本轮只迁移同父级排序。改变实际变形父级仍需独立的保持形态转换计划，不提供未经转换的 Inside 落点或静默 KeepLocalData 替代。层级树创建、键盘和完整选择行为继续按 U2 推进；U3–U6 状态不变。

第二轮验证：

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --lib -- \
  headless_tests::sorting headless_tests::hierarchy headless_tests::authoring \
  headless_tests::parameter_creation headless_tests::parameter_groups \
  headless_tests::parameter_management headless_tests::parameters \
  headless_tests::properties headless_tests::physics headless_tests::animation_panels \
  headless_tests::workspaces headless_tests::command_targets \
  headless_tests::input_routing headless_tests::ui_performance properties::form::tests

env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration -- \
  palette_moves:: organization_authoring:: sorting::
```

受影响 UI 回归 **149 项通过**，包含新增的 8 项移动 UI 回归；其中悬停规划工作次数测试验证连续输入和重绘不重复构建计划。新增 core 移动专项 5 项通过。core 受影响过滤门禁共 **21 项通过**，包含新增移动专项 5 项；格式与空白检查通过。本轮未运行完整套件、原生截图和 Release 性能基准。


## 第三轮实现：组织树创建与筛选显露

前两轮代码已提交为 `65dd84a`。依照用户“commit 后继续”的要求，本轮在当前 checkout 接续 U2；依赖仍为 `zipzi-2d b86b13b`，没有修改相邻仓库。

- 新增 core `SceneObjectType` / `SceneObjectCreationPlan`：统一 Part、空 Mesh、Warp、Rotation、Offscreen 的创建规划。UI 只提供本地化名称与捕获的上下文；core 分配 UUID、按拥有者避重默认名称、确定完整集合插入锚点，SDK 生成数据默认值、验证归属/锁并发布单条历史。
- 单个 Part 内追加；普通行同级之后创建；同拥有者多选在完整顺序的最后选中项之后创建；跨拥有者选择回到根集合。根部 Part 与未归属对象按各自显示分支确定顺序。组织树中的变形器行只表示组织归属，创建不会意外改变实际变形父级。
- 组织树工具栏提供新建菜单，行菜单捕获点击行，空结果/根行提供根集合菜单。菜单打开前结算输入，一次规划后捕获版本；切换文档/工作区或修改文档后拒绝旧菜单。锁定与 Offscreen 缺少 Part 归属保留禁用原因和中英文说明。
- 创建成功后复用共享结果检查，选中对象、展开祖先、滚动、显示 Properties 并聚焦名称。后续改名与创建分别撤销。
- 搜索或类型排除时，临时显露新对象及其祖先，标记“新建”；不扩散到兄弟或后代，也不改写筛选条件。已有清除筛选按钮仍可用；修改筛选、选择其他对象或切换文档会清理临时显露。

变形树创建尚未接入。当前 SDK 的 `SceneEdit::Create.before` 定位组织集合，变形顺序仅追加，不能原子表达在完整变形同级顺序的指定位置插入；需补充类型化创建位置契约后再接 UI，详见[核心方案第 6.1 节](unified-object-authoring-core.md#61-ui-u2-发现的后续创建位置契约)。本轮没有用第二条移动历史补齐位置。

第三轮定向门禁：

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --lib -- \
  headless_tests::scene_creation headless_tests::hierarchy headless_tests::sorting \
  headless_tests::authoring headless_tests::properties headless_tests::panel_toolbar \
  headless_tests::command_targets headless_tests::ui_performance \
  panels::hierarchy_tree::filter

env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration -- \
  scene_creation:: scene_lifecycle:: palette_moves:: organization_authoring:: sorting::

cargo fmt --all -- --check
git diff --check
```

受影响 UI/筛选/工作量回归 **92 项通过**（包含新增的 6 项创建 UI 回归和 1 项筛选显露单测）；core 创建、场景生命周期、移动和组织回归 **31 项通过**（包含新增的 5 项创建回归）。格式与空白检查通过。没有运行完整套件、原生截图或 Release 性能基准。U2 仍未全量验收；主菜单/快捷键创建、变形树创建和阶段状态表中的剩余内容继续待接入。


## 第四轮实现：树选择与可配置移动快捷键

第三轮组织树创建已提交为 `5359c2d`。本轮继续在当前 checkout 工作，没有创建 worktree 或修改相邻 `zipzi-2d`。

- 新增层级树 `selection.rs`，以稳定对象引用保存 Shift 锚点，以稳定行键保存键盘焦点。Cmd 切换成员；Shift 以当前可见行作范围；Cmd+Shift 合并范围。反向范围仍以点击项作为 primary，重复 Shift 保留原锚点。过滤和折叠不会自动清除模型中的既有选择；范围不纳入隐藏行，也不把行号当对象身份。
- 普通上下键移动行焦点、滚动到焦点，Enter 才更新选择；Shift+上下扩展选择，左右键折叠/展开或进入子级/返回父级，Home/End 定位首尾。箭头和 Enter 不截获搜索框输入；父级折叠保留隐藏成员的选择。文档或工作区 token 改变后重置光标和锚点，删除对象后校正锚点。
- 新增稳定命令 `palette.move-up` / `palette.move-down` 和 Palette 快捷键作用域，默认 Alt+Up/Down，可通过现有 `keybindings.json` 改键。组织树、变形树同父级和 Parameter/文件夹的单对象移动调用现有 `plan_palette_step` / `execute_palette_move`，不新增领域排序规则。右键上下移显示同一命令的快捷键。
- Palette 命令按当前可见面板的精确列表焦点解析目标，树中使用焦点行，Parameter 使用检查目标；移动不伪造选择或激活。文本、搜索框、弹出控件、隐藏面板和活动拖拽不会触发这一入口。匹配的 Palette 快捷键优先于悬停画布的同键映射，其他窗口/文档/画布路由保持现有规则；冲突检查把 Palette 与 Document 视为重叠作用域。
- Parameter 名称和文件夹检查按钮在选择前结算 Properties 输入，并将键盘焦点放回列表。树选择也使用统一输入边界，防止旧字段在选择改变后补交。
- 单对象键盘移动与原菜单一致：多选集合中任一项聚焦时拒绝单项移动并说明使用拖拽；锁定、边界 no-op、输入结算及错误仍由共享适配和 core/SDK 处理。多选上下移和“移到…”菜单尚未实现，不声称已经覆盖全部移动入口。

第四轮定向验证：

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --lib -- \
  headless_tests::tree_selection headless_tests::palette_commands \
  headless_tests::hierarchy headless_tests::sorting headless_tests::scene_creation \
  headless_tests::parameters headless_tests::parameter_groups \
  headless_tests::parameter_management headless_tests::authoring \
  headless_tests::properties headless_tests::panel_toolbar headless_tests::command_targets \
  headless_tests::input_routing headless_tests::ui_performance \
  keybindings::tests input_routing::policy

env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration -- \
  commands:: keyboard:: sorting:: palette_moves::

cargo fmt --all -- --check
git diff --check
```

受影响 UI/路由/工作量回归 **140 项通过**，含新增 5 项树选择 UI、3 项移动命令 UI 和 1 项路由优先级单测；既有 keymap 单测补充作用域隔离断言。core 命令/键盘/移动回归 **33 项通过**，含新增的改键和作用域冲突回归。格式和空白检查通过。本轮未运行完整套件、原生截图或 Release 性能基准；U2–U6 仍按阶段状态表继续。


## 第五轮实现：指定容器移动

第四轮树选择和移动快捷键已提交为 `412c7ee`。继续使用当前 checkout，依赖 `zipzi-2d b86b13b`；本轮没有修改相邻仓库。

- core 新增轻量 `PaletteMoveDestination` 与 `palette_move_destinations`。根集合、Part 和 ParameterGroup 按原生容器顺序提供稳定身份与完整祖先路径；空容器也可选择，同名路径保留不同身份。使用迭代遍历，不复制整棵场景树及非容器对象，也不为每个容器生成移动计划。
- 组织树和 Parameter/文件夹行菜单增加“移到…”；点在现有树多选内时冻结整个集合，点在集合外时只捕获点击对象。目标选择器可搜索完整路径；同名路径附加内部 ID 以区分。根集合是明确选项，成功移动追加到目标的完整顺序末尾。
- 新增 `palette.move-to` 命令，使用既有 Palette 作用域和改键系统，无默认快捷键；列表焦点与行上下文沿用第四轮策略。菜单与快捷键共用同一目标选择器，实际操作调用 `plan_palette_move` / `execute_palette_move`。SDK 决定祖先归并、来源顺序、锁、循环、Offscreen 归属和整体原子性。
- 搜索和重绘不规划。选定目标后只保存一个当前计划以显示校验结果，确认时重新规划、复核文档/编辑 token 与 revision，再执行一条历史。打开前结算输入并结束阻塞编辑，然后捕获对话框上下文；修改文档后及时禁用提交。取消、无目标和 no-op 不消费历史或 redo。
- 不合法目标保留可读原因，禁用移动；补齐 Part 的原生 `RELATION_CYCLE` 映射，以及 Offscreen 必须归属 Part、拥有者已存在合成与需要转换的中英文反馈。对话框复用 `FormDialog` 的单次提交生命周期，选中目标的 Enter 只确认下拉选项，不同时提交移动。
- 成功后不伪造单击、不改变检查或原多选集合。组织树展开结果路径并滚动，Parameter 定位器可按显式对象定位而不改写参数焦点。筛选下临时显露移动对象及祖先并标记“已移动”，保留查询/类型条件，沿用清除筛选入口；延迟定位也复核执行后的上下文。

变形树不展示“移到…”菜单；变形父级改变仍需保持形态转换计划。多选上下移、Parameter 文件夹选择导航、其余创建命令入口，以及 U3–U6 仍未完成，不把本轮交付表述为 U2 全量验收。

第五轮定向门禁：

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --lib -- \
  headless_tests::palette_move_to panels::sorting::destination::tests \
  headless_tests::tree_selection headless_tests::palette_commands \
  headless_tests::hierarchy headless_tests::sorting headless_tests::scene_creation \
  headless_tests::parameters headless_tests::parameter_groups \
  headless_tests::parameter_management headless_tests::parameter_creation \
  headless_tests::authoring headless_tests::properties headless_tests::panel_toolbar \
  headless_tests::command_targets headless_tests::input_routing \
  headless_tests::ui_performance keybindings::tests input_routing::policy \
  panels::hierarchy_tree::filter

env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration -- \
  palette_moves:: organization_authoring:: sorting:: keyboard:: commands::

cargo fmt --all -- --check
git diff --check
```

受影响 UI/路由/筛选/工作量回归 **160 项通过**，其中新增 10 项“移到…” UI 回归和 1 项规划工作量测试。工作量验证搜索及 8 次连续通知重绘不规划，目标选择后 1 次，提交前重规划累计 2 次。core 移动、组织、命令和快捷键回归 **43 项通过**，含新增 3 项目标元数据/空容器/重名身份回归。格式与空白检查通过。本轮未运行完整套件、原生截图和 Release 性能基准。


## 第六轮实现：多选上下移动

第五轮指定容器移动已提交为 `25bf0f9`。本轮继续在当前 checkout 实施；依赖仍为 `zipzi-2d b86b13b`，没有修改相邻仓库。

- 新增 core `plan_palette_steps`，单对象 `plan_palette_step` 也委托此入口。按完整原生集合识别连续选区，每段跨过一个未选邻居，保留段内顺序和选区间隔；边界选区不动。向上从前往后、向下从后往前规划，并按稳定对象锚点解析候选顺序，不使用过滤后的行号。
- 组织树与 Parameters 可同时处理多个拥有者集合；根 Part/未归属对象、根变形器/根 Mesh 和文件夹/参数按各自显示集合处理。原生组织子树查询归并被选祖先内的后代，锁检查覆盖全部源子树，包含已经到达边界的源。锁定成员使整个操作失败，避免只移动其他选区。
- 规划只克隆一次原生 COW 文档，在候选上逐段执行原生 Organization/Scene 计划；执行通过 SDK 单个 `edit` 组合所有意图，使用捕获版本，原子提交一份 receipt 和一条历史。没有增加独立撤销栈。全选或边界 no-op 保留 redo；过期计划即使经过 Undo 恢复相同内容也拒绝执行。
- 右键上下移与可配置快捷键冻结同一源集合：点击/焦点在选择内使用全体，否则只使用该行。菜单回调不重新读取选择，提交前复核上下文并重规划。移动保留检查与多选；成功后复用带上下文保护的展开、滚动与过滤显露定位。
- 变形树只支持同一实际父级内的多选上下移；跨父级选择显示中英文原因。此入口不改变实际变形父级，也不使用 KeepLocalData 替代保持形态转换。原生规划错误以类型化错误码提供给 UI，继续复用锁定等本地化反馈。

第六轮定向门禁：

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --lib -- \
  headless_tests::palette_steps headless_tests::palette_move_to \
  panels::sorting::destination::tests \
  headless_tests::tree_selection headless_tests::palette_commands \
  headless_tests::hierarchy headless_tests::sorting headless_tests::scene_creation \
  headless_tests::parameters headless_tests::parameter_groups \
  headless_tests::parameter_management headless_tests::parameter_creation \
  headless_tests::authoring headless_tests::properties headless_tests::panel_toolbar \
  headless_tests::command_targets headless_tests::input_routing \
  headless_tests::ui_performance keybindings::tests input_routing::policy \
  panels::hierarchy_tree::filter

env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration -- \
  palette_steps:: palette_moves:: organization_authoring:: sorting:: keyboard:: commands::

cargo fmt --all -- --check
git diff --check
```

受影响 UI/路由/筛选/工作量回归 **166 项通过**，包含新增 6 项多选上下移 UI 回归；core 受影响门禁 **51 项通过**，包含新增 8 项分段移动、祖先归并、多拥有者、混合参数/文件夹、变形同级、边界与锁、过期计划及 Undo/Redo/保存回读回归。格式与空白检查通过。本轮未运行完整套件、原生截图或 Release 性能基准。

U2 仍未完成：变形树创建的位置契约、保持形态改父级、Parameter 文件夹选择导航与其余创建命令入口继续待实施；U3–U6 状态不变。


## 第七轮实现：Parameter 文件夹选择与键盘导航

第六轮多选上下移已提交为 `24601bc`。本轮继续使用当前 checkout，没有创建 worktree 或修改相邻 `zipzi-2d`。

- Parameter 文件夹名称单击检查文件夹，箭头独立展开/折叠；原检查按钮复用同一激活路径。参数仍使用既有 `FocusParameter`，文件夹使用 `Navigate`，保留场景选择、参数预览值和参数焦点。空文件夹也可以检查，折叠不清除隐藏对象的检查身份。
- 新增仅用于展示的稳定对象光标。上下键及 Home/End 只移动焦点并滚动，Enter 才激活；左右键展开/折叠、进入首个可见子行或返回父行。空文件夹不会跳入下一个根组；“未分组”有独立展示身份，可以折叠和导航，但不会提交对象检查或移动。
- Palette 的上下移与“移到…”快捷键使用当前可见焦点行，避免浏览到文件夹后仍操作旧的检查对象。重排后仍保持该稳定光标及原检查目标；删除或隐藏光标行后校正，Undo 不复活已失效光标。文档或工作区 token 改变后重建导航上下文。
- 单行检查与焦点分别显示背景、边框和选择元数据；只在状态变化时通知受影响的缓存参数行，保留原滑块与数值控件实体。普通预览值更新不重新协调整份导航样式。文件夹箭头补齐中英文可访问名称。
- 激活先通过共享输入边界结算旧 Properties 文本；模态、忙状态、拖拽、输入法字符输入和文本控件焦点不会触发列表导航。本轮延续 Parameter 的单行检查策略，没有新增 Parameter 的 Shift/Cmd 多选或批量属性编辑。

第七轮定向门禁：

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --lib -- \
  headless_tests::parameter_navigation \
  headless_tests::palette_steps headless_tests::palette_move_to \
  panels::sorting::destination::tests \
  headless_tests::tree_selection headless_tests::palette_commands \
  headless_tests::hierarchy headless_tests::sorting headless_tests::scene_creation \
  headless_tests::parameters headless_tests::parameter_groups \
  headless_tests::parameter_management headless_tests::parameter_creation \
  headless_tests::authoring headless_tests::properties headless_tests::panel_toolbar \
  headless_tests::command_targets headless_tests::input_routing \
  headless_tests::ui_performance keybindings::tests input_routing::policy \
  panels::hierarchy_tree::filter

cargo fmt --all -- --check
git diff --check
```

受影响 UI/输入路由/移动/创建/虚拟化工作量门禁 **174 项通过**，包含新增 8 项文件夹导航回归：独立折叠与检查、可见层级导航、按焦点移动及未分组 no-op、旧文本结算与文本按键隔离、空组和文档/工作区切换、模态阻止、删除/Undo 光标校正，以及 80 行虚拟列表 Home/End 定位。格式与空白检查通过。本轮没有 core 产品改动，未重复运行 core 门禁；未运行完整套件、原生截图或 Release 性能基准。

U2 仍待变形树创建的位置契约、保持形态改父级和其余创建命令入口；U3–U6 状态不变。


## 第八轮实现：创建主菜单与可配置命令

第七轮 Parameter 文件夹导航已提交为 `db58fce`。本轮继续使用当前 checkout，依赖仍为 `zipzi-2d b86b13b`；没有创建 worktree 或修改相邻仓库。

- 命令目录新增 `scene.create-part`、`scene.create-mesh`、`scene.create-warp`、`scene.create-rotation`、`scene.create-offscreen`。均为 Modeling 文档命令，无默认键位、不可按住连发，可通过现有 `keybindings.json` 配置；文本、搜索框、输入法字符输入和模态继续由既有路由隔离。原 `tool.generate-mesh` 保留其内容生成用途。
- “编辑 → 创建”主菜单列出五种场景创建，以及已有 Parameter 和 ParameterGroup 创建命令。场景命令在调用时捕获与组织树工具栏相同的场景选择，使用组织投影的完整集合位置，不因焦点处于变形树就隐式指定实际变形父级。右键继续使用显式点击行上下文。
- 新增共享 `authoring/scene_creation.rs`，组织树工具栏、右键、原生菜单与命令调用同一执行及结果协调路径。创建成功后显露结果和祖先、定位树、打开 Properties、选中名称；隐藏面板可重新打开。Properties 最后显示，支持树与 Properties 共用标签组的布局。延迟定位复核执行后上下文及检查对象；新建会重置树的范围选择锚点，移动仍保留原锚点。
- core 提供轻量创建上下文可用性查询，复用创建拥有者解析、SDK 集合能力和原生 Offscreen 校验；查询不分配对象身份、不计算避重名称、不枚举插入顺序、不生成或保存文档计划。实际创建才解析完整集合锚点并规划，继续由 SDK 校验最终数据、锁及事务。
- 原生菜单明确禁用锁定容器、缺少 Offscreen 拥有者或已有 Offscreen 的创建动作，并显示中英文原因。菜单缓存纳入创建原因，避免可用性一直为 false 时保留旧原因。过期文档目标、Animation、忙状态或模态不发布创建；输入结算和后续独立改名历史沿用共享边界。

第八轮定向门禁：

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --lib -- \
  headless_tests::scene_creation_commands headless_tests::parameter_navigation \
  headless_tests::palette_steps headless_tests::palette_move_to \
  panels::sorting::destination::tests \
  headless_tests::tree_selection headless_tests::palette_commands \
  headless_tests::hierarchy headless_tests::sorting headless_tests::scene_creation \
  headless_tests::parameters headless_tests::parameter_groups \
  headless_tests::parameter_management headless_tests::parameter_creation \
  headless_tests::authoring headless_tests::properties headless_tests::panel_toolbar \
  headless_tests::command_targets headless_tests::input_routing \
  headless_tests::ui_performance keybindings::tests input_routing::policy \
  panels::hierarchy_tree::filter menus::tests

env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration -- \
  scene_creation:: scene_lifecycle:: organization_authoring:: palette_steps:: \
  palette_moves:: sorting:: keyboard:: commands::

cargo fmt --all -- --check
git diff --check
```

受影响 UI/命令/移动/创建/输入路由/虚拟化门禁 **184 项通过**，包含新增 9 项创建命令 UI 回归：原生菜单真实动作、各命令来源、实际改键与文本/重复按键隔离、输入结算及过滤显露、禁用原因缓存更新、过期目标与工作区/模态拒绝、隐藏面板恢复、共用标签组名称聚焦、新建后的范围锚点。core 定向门禁 **63 项通过**，包含新增 2 项命令可用性/原生归属、历史与 redo 保留、稳定命令 ID、改键及冲突检查回归，并扩展已有忙状态断言。格式与空白检查通过。本轮未运行完整套件、原生截图或 Release 性能基准。

U2 仍未全量验收：变形树创建需要核心方案第 6.1 节的双投影插入位置契约；改变实际变形父级需要保持形态转换交互。U1 的全入口收口与 U3–U6 状态不变。


## 第九轮实现：变形树创建与双投影位置

第八轮创建主菜单和快捷键已提交为 `6db8b70`，没有包含其他任务已暂存的 SDK 设计文档。本轮在当前 checkout 继续；`zipzi-2d` 以 `b86b13b` 为基础补充原生契约，Editor 与 SDK 本轮改动均尚未提交，没有创建 worktree。

- 补齐[核心方案第 6.1 节](unified-object-authoring-core.md#61-ui-u2-发现的后续创建位置契约)：原生 `SceneEdit::Create` 增加独立可选的 `deformation_before`，组织集合仍由 `part_id` / `before` 确定，实际变形集合由 `parent_id` / `deformation_before` 确定。创建 planner 校验两个拥有者、锁和稳定锚点，在同一创建 delta 内发布两份顺序；不使用创建后再移动的第二条历史。Offscreen 和 Glue 拒绝变形锚点。SDK 的版本校验、事务回滚、双集合变化摘要与 Undo/Redo 沿用原生路径。
- core 新增按明确投影创建的入口，计划携带投影供结果定位；原组织创建入口委托此入口。变形树只支持 Mesh、Warp、Rotation。单个变形器内追加，单个 Mesh 同父级之后插入，同父级多选按完整顺序的最后成员之后插入；跨实际父级选择回到根集合。根变形器与根 Mesh 保留各自展示集合顺序。
- 组织归属独立于实际变形父级：单个上下文沿用该对象的组织拥有者，同父级但组织拥有者不同的多选在组织根集合追加，同时保留共同实际父级。变形投影创建不顺带重排组织成员；名称继续按组织拥有者避重，数据默认值与最终校验由 core/SDK 提供。
- 变形树增加创建工具栏、行右键和根集合菜单，复用共享创建执行、Properties 名称聚焦、过滤下临时“新建”显露、展开和滚动。行菜单捕获点击行，不改用菜单打开后的全局选择；锁定目标禁用并显示原因，过期菜单整体拒绝。Part/Offscreen 不展示误导性的变形树创建项。
- 新增 `deformation.create-mesh`、`deformation.create-warp`、`deformation.create-rotation` 三个明确投影的文档命令，无默认键位，可通过现有改键系统配置；主菜单“编辑 → 创建 → 在变形树中新建”提供同一动作。命令目录使用可区分组织/变形创建的中英文名称，面板内菜单保留简短名称。已有 `scene.create-*` 继续明确表示组织投影，不从当前焦点暗中切换领域语义。
- 创建可用性继续使用轻量原生查询；补充原生集合验证入口，使变形集合能力查询只校验父级及锁，不枚举完整同级集合或构建逐项 SDK handle。实际计划解析完整顺序，过滤和折叠不影响稳定锚点。补充 Python API 文档；JSON 命令入口可直接传入新锚点，无需新增面板资产拼装逻辑。

第九轮定向门禁：

```sh
# zipzi-editor
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --lib -- \
  headless_tests::deformation_creation headless_tests::scene_creation_commands \
  headless_tests::parameter_navigation headless_tests::palette_steps \
  headless_tests::palette_move_to panels::sorting::destination::tests \
  headless_tests::tree_selection headless_tests::palette_commands \
  headless_tests::hierarchy headless_tests::sorting headless_tests::scene_creation \
  headless_tests::parameters headless_tests::parameter_groups \
  headless_tests::parameter_management headless_tests::parameter_creation \
  headless_tests::authoring headless_tests::properties headless_tests::panel_toolbar \
  headless_tests::command_targets headless_tests::input_routing \
  headless_tests::ui_performance keybindings::tests input_routing::policy \
  panels::hierarchy_tree::filter menus::tests

env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration -- \
  deformation_creation:: scene_creation:: scene_lifecycle:: organization_authoring:: \
  palette_steps:: palette_moves:: sorting:: keyboard:: commands::

# ../zipzi-2d
cargo test --locked -p zipzi-sdk --test integration -- \
  scene_creation_placement:: scene_lifecycle:: scene_content:: scene_authoring:: \
  organization_authoring:: hierarchy_order:: authoring_acceptance::

# 两个仓库分别执行
cargo fmt --all -- --check
git diff --check
```

受影响 UI/命令/创建/移动/路由/虚拟化门禁 **191 项通过**，包含新增 7 项变形创建 UI 回归；core 定向门禁 **69 项通过**，包含新增 6 项投影位置、默认归属、多选、根集合、不可用/过期与命令能力回归；SDK 受影响门禁 **78 项通过**，包含新增 6 项双锚点、双集合发布、三种数据默认值、错误集合/缺失/自身锚点、两个拥有者的锁、事务回滚、JSON、过期计划、单条 Undo/Redo 与保存回读回归。既有快捷键回归扩展稳定 ID 与冲突断言。测试集合与前轮有重叠，不累计相加。

两个仓库格式与空白检查通过。本轮未运行完整套件、Python 绑定构建/门禁、原生截图或 Release 性能基准。U2 尚未全量验收，剩余重点是改变实际变形父级时的保持形态转换交互；U1 全入口收口与 U3–U6 状态不变。


## 第十轮实现：保持形态改父级审阅与执行

上一轮已分别提交为 Editor `206333b`、SDK `2eaeef6`。本轮继续在当前 checkout 工作，没有创建 worktree；没有包含其他任务的 SDK runtime 设计文档。本轮新实现暂未提交。

- 变形树拖放支持 Before/After 跨实际父级、变形器 Inside 和根集合落点；修正根落点投影身份。右键与既有 `palette.move-to` 命令接入可搜索变形容器选择器，路径同名时附稳定 ID。移动源来自打开前捕获的集合，不重新取全局选择；同父级普通排序继续走轻量场景移动。
- 打开保持形态审阅时先结算旧输入并结束当前编辑，冻结 Modeling 参数值、完整参数区间、编辑时间、SDK 会话版本、编辑上下文与预览 revision。默认“仅当前编辑态”；用户可显式切换“整个参数范围”。不会因域证明失败自动选择较弱策略，也不提供静默 KeepLocalData 回退。
- 绝对/相对容差初始为核心约定的 `1e-5` / `1e-6`，可显式输入；无效、非有限或负值禁用确认。默认误差界无法满足时保留原生原因，不自动放宽。测试包含非平凡旋转在默认容差下拒绝，以及用户明确请求 `0.001` 模型单位绝对容差后成功的路径。
- 转换仅针对当前选定目标、范围与容差，在 120ms 防抖后从一份 COW 只读快照后台规划。搜索、重绘和拖放悬停不执行几何转换；切换目标/范围/容差、Esc/Cancel、关闭、文档/工作区/版本或冻结参数失效会取消旧控制并丢弃迟到结果。等待期间保留 Cancel，显示原生检查进度；共享 FormDialog 增加关闭生命周期钩子。
- 审阅展示受影响对象、保留身份/重写/生成/删除/重映射地址数量、原生位置保证、冻结参数和时间、证明区间、实际误差与上界、绝对/相对容差及参考尺度、采样/数据/临时内存/历史容量。继承不透明度、颜色、启用状态、域可见性、Glue 和有状态动画的未覆盖范围按原生报告显示中英文说明。
- 补充原生 `ReparentPlacement`，在保持形态计划中同时写入目标实际变形集合的稳定顺序，并纳入容量估算。SDK 只读快照可后台规划场景编辑；最终执行仍检查原会话版本、共享取消状态和当前容量。确认执行用户已审阅的同一份计划，不重新转换、不分成两条历史。失败事务整体回滚；取消、无变化和过期保留 redo。
- 成功后保留选择和检查目标，通过既有变形树定位器展开/滚动到移动结果；筛选条件保持，必要时临时标记“移动”显露。锁、循环、Warp 非线性逆变换、变形器子树和无法证明的参数域显示原生拒绝原因。

本轮保持形态交互在原生可证明范围内已接通。静态可逆仿射 Mesh 与已有 MeshBinding 的基础/关键形态转换继续由原生负责；不声称新增 Warp 非线性或变形器子树保持算法。U2 功能入口已接入，完整中英文原生截图、实际窗口验收与 Release 性能对照仍待 U6；U1 全入口收口及 U3–U6 其他工作状态不变。

第十轮定向门禁：

```sh
# zipzi-editor：沿用第九轮的 UI/core 过滤集合，并加入以下专项
./scripts/test.sh regression -- \
  panels::sorting::reparent panels::sorting::destination \
  headless_tests::deformation_creation headless_tests::scene_creation_commands \
  headless_tests::parameter_navigation headless_tests::palette_steps \
  headless_tests::palette_move_to headless_tests::tree_selection \
  headless_tests::palette_commands headless_tests::hierarchy headless_tests::sorting \
  headless_tests::scene_creation headless_tests::parameters headless_tests::parameter_groups \
  headless_tests::parameter_management headless_tests::parameter_creation \
  headless_tests::authoring headless_tests::properties headless_tests::panel_toolbar \
  headless_tests::command_targets headless_tests::input_routing \
  headless_tests::ui_performance keybindings::tests input_routing::policy \
  panels::hierarchy_tree::filter menus::tests \
  deformation_creation:: scene_creation:: scene_lifecycle:: organization_authoring:: \
  palette_steps:: palette_moves:: palette_reparent:: sorting:: keyboard:: commands::

# ../zipzi-2d
cargo test --locked -p zipzi-sdk --test integration -- \
  binding_authoring::conversions::reparent scene_creation_placement:: scene_lifecycle:: \
  scene_content:: scene_authoring:: organization_authoring:: hierarchy_order:: \
  authoring_acceptance::

# 两个仓库分别执行
cargo fmt --all -- --check
git diff --check
```

受影响 Editor UI **199 项通过**，包含新增 8 项保持形态交互回归：审阅保证与单条提交、原选择/检查/筛选保留和结果显露、默认编辑态与显式域证明/拒绝、搜索及重绘工作量、预览过期、待计算取消、根拖放、跨父级 After 锚点、右键入口、容差校验与 no-op/redo。core 定向门禁 **74 项通过**，包含新增 5 项保持形态回归。SDK 定向门禁 **92 项通过**，包含新增 2 项只读快照版本、转换及顺序原子发布、非法锚点、取消、JSON、失败事务、参数域采样、保存回读与 Undo/Redo 回归；其余保持形态专项已有用例一并执行。测试集合与前轮重叠，不累计相加。

两个仓库格式与空白检查通过。未运行昂贵完整套件、Python 绑定构建、原生截图或 Release 性能测量。


## 第十一轮实现：U1 能力、结果和业务边界收口

上一轮已分别提交为 Editor `31970d0`、SDK `c1d4a47`。本轮继续在当前 checkout 工作，没有新建 worktree；本轮 SDK 配套改动已提交为 `26afe00`，Editor 改动随本次提交交付。

- 新增统一 `AuthoringAction` / `ObjectAuthoringCapabilities`，覆盖全部 Editor 对象类型，集中维护 SDK ↔ Editor 类型及 GlueBinding 等投影别名。只复制原生能力状态；候选手势作为独立 session reason，检查仍可用。SDK 补充通用集合能力查询，覆盖 Organization、Scene、References、Pose、Physics、Motion 注册/组/Clip、Expression/Entry、Texture 与 Attachment。查询不创建 ID、候选或变形转换计划。
- Parameter 工具栏、行菜单、主菜单和命令可用性共用原生状态；Edit 保留检查能力，删除引用中的 Parameter 直接禁用并保留原生原因。Physics 组/通道/节点动作改用原生能力，不再在面板复制根节点、最短链、输出引用、锁和排序边界规则。不支持的节点动作省略；暂不可用显示共享原因。Properties 的编辑许可与身份锁标识也直接查询 SDK。
- 所有领域执行回执经一个转换器生成 `OperationResult`，统一 created/modified 实体及集合拥有者的类型、选择建议和定位提示。创建、删除回退、拖拽、上下移、移到指定位置、保持形态改父级和既有 New Motion 接入同一结果协调。New Motion 保留已有的激活行为；统一结果处理本身不额外激活 Motion/Expression。
- 统一检查接口支持 Animation 中的全部现有类型和子条目，保持 Modeling 选择、播放资源、播放状态、时间与历史。共享定位器覆盖两棵树、Parameter、Motion/Expression、Physics 和现有文档面板；子项通过 Contains 回到可见拥有者。当前工作区没有对应集合视图时显示 Properties，后续 U3–U5 再增加专用列表，不把 U1 结果协调等同于这些新入口已经完成。
- 定位和字段焦点延迟回调复核文档、工作区、模型代次与检查目标；共享标签页先显示 Properties，集合重新打开后应用待定位对象。普通滚动列表采用首次布局也有效的按项置顶定位；Parameter 保留原虚拟列表的下一帧显露适配。Relations 使用当前工作区的检查对象。
- 参数范围判断移到原生 `validate_parameter_range`，草稿、属性字段和最终发布复用同一规则；UI provider 移除领域校验回调。Parameter 删除旧命令转到统一原生规划器；Physics 创建默认内容、追加节点与稳定排序锚点移到 core，面板只处理控件和光标。Keyform 的二维控件限制改为明确的预览垫提示，避免误报核心绑定只读。
- 回归发现并修复项目加载顺序：持久化锁定状态必须在 CDI、Motion、Expression、Physics、Pose 等全部身份恢复后校验，避免锁定文件夹或业务子项保存后无法打开。新增跨 CDI/Physics/Pose 的保存回读测试。

U1 的共享适配功能在本轮收口。U3 的 Motion/Expression 新管理入口、U4 的完整 Physics/Pose 子条目交互、U5 的各领域入口仍按阶段推进；U6 的原生截图、中英文实际窗口与 Release 性能验收尚未执行。

第十一轮定向门禁：

- Editor：沿用第十轮创建、移动、保持形态、导航、输入边界和虚拟化过滤集合，加入 `authoring_coordination`、Physics、Animation 面板、工作区、Timeline、紧凑布局和受影响领域适配测试。**UI 285 项、core 集成 112 项、core 单元 4 项通过**。
- SDK：对象/集合能力、绑定、Motion、Expression、Physics、Pose、组织/场景/内容、引用、资源、身份与锁、保持形态和共享回执等定向过滤集合，**278 项通过**；既有 Release 大库规划测量 1 项按默认忽略。
- Project：`cargo test --locked -p zipzi-project --test project_tests -- resources_codec version_migration`，**18 项通过**。
- 两个仓库分别执行 `cargo fmt --all -- --check` 与 `git diff --check`。

新增 7 项 core 协调用例、5 项 UI 协调用例和 4 项 SDK 集合/范围/锁定保存回读用例；旧的引用删除回归改为验证原生禁用状态和原因。既有测试跨轮有重叠，不累计相加。

未运行昂贵完整套件、Python 绑定构建、原生截图、实际窗口验收或 Release 性能测量。

## U1–U2 Review：输入边界、上下文和能力查询

审查范围为 `1e2ac82^..7a42eb2`，包含起始提交。2026 年 10 月 23 日在当前 checkout 修正以下问题：

- **锁定前丢失文本草稿**：树锁定按钮此前先写锁，迟到的名称 blur 再因只读而失败。锁定、可见性及其来源定位入口现在先通过共享输入边界；合法改名和锁定各占一条历史，非法文本恢复后的原因保留。
- **参数删除重复结算导致过期拒绝**：管理入口先提交草稿，再用提交前的上下文执行删除，误报 `Object operation context changed`。删除现在只由共享执行器结算输入，并在结算后读取参数版本；Undo 删除恢复已提交的新名称。
- **树行创建忽略多选**：右键点击多选中的成员时，创建只使用点击行，导致与工具栏/命令的插入位置不同。现在捕获整个相关多选集合，按完整文档顺序插入；菜单打开后的选择变化不会替换捕获的上下文。
- **Motion 定位时只展开部分组内容**：定位折叠组内后部注册时，前面的行已被跳过、组头也已按折叠状态生成。现在先解析唯一定位目标并展开所属组，再生成行和游标；不激活预览。
- **创建成功覆盖输入错误**：场景创建不再清除本次输入结算产生的恢复原因。场景菜单和命令共用一个创建执行入口，统一在输入结算后构造和发布计划，删除两套预先规划及重复结算路径。
- **属性事件重复查询整组能力**：`authoring_capabilities` 包含删除依赖等查询，过去每个控件或预览事件都重新执行。现在按 SDK 会话版本和类型化对象身份缓存原生能力；候选编辑状态在每次读取时叠加，锁定、发布和 Undo/Redo 均随版本失效。工作量回归验证连续 12 次透明度预览复用原生查询，取消候选立即恢复会话可用性。

新增 5 项 UI 回归和 1 项 core 工作量/失效回归。验证采用定向过滤集合：

```sh
./scripts/test.sh regression -- \
  document::authoring::capabilities \
  headless_tests::authoring headless_tests::scene_creation \
  headless_tests::deformation_creation headless_tests::tree_selection \
  headless_tests::hierarchy headless_tests::sorting headless_tests::palette \
  headless_tests::parameter headless_tests::properties headless_tests::physics \
  headless_tests::animation_panels headless_tests::workspaces \
  headless_tests::ui_performance properties::form::tests panels::sorting:: \
  authoring_coordination:: palette_moves:: palette_steps:: palette_reparent:: \
  organization_authoring:: scene_creation:: deformation_creation::

# 创建入口合并后重新验证受影响路径
./scripts/test.sh ui -- headless_tests::scene_creation headless_tests::deformation_creation
cargo fmt --all -- --check
git diff --check
```

范围回归 **UI 201 项、core 集成 46 项、core 单元 1 项通过**；创建入口合并后，受影响的 **24 项 UI 测试重跑通过**。格式和 diff 检查通过。未执行昂贵全套、原生截图或 Release 性能基准；缓存优化的验证是查询工作量和行为回归，不代表已完成实际窗口性能验收。U3–U6 阶段状态保持不变。


## U3 首轮：Motion、Expression 与稳定成员编辑

2026 年 10 月 23 日，在当前 checkout 开始 U3。依赖 `zipzi-2d 26afe00`；本轮没有修改相邻仓库。

- core 新增 `AnimationLibraryPlan`，统一 Motion、空 MotionGroup、空 Expression 和未配置 ExpressionEntry 的身份、名称与稳定插入位置。创建与初始排序通过原生类型化操作组合在一个 SDK 事务内，创建和后续改名分别撤销。旧 Timeline New Motion 也复用核心默认创建路径。
- 工具栏、行菜单、主菜单和可配置创建命令调用同一适配器；右键捕获点击对象。Motion 创建后激活当前注册并重置时间，不自动播放；Expression 创建使用空参数表并沿用原选择路径。成员创建明确捕获拥有者，显示 Properties 并聚焦参数选择。
- Motion 组从真实组集合渲染，空组可见，折叠按组 ID 保存；箭头只折叠，名称单击检查。列表焦点跟随稳定身份；上下键只浏览，Enter 激活，左右键处理折叠。定位 Clip 先解析唯一注册，避免共享 Clip 的多个注册同时展开、抢占焦点。
- Properties 区分“动作内容”和“当前注册”。名称、时长、FPS、循环与 Clip fade 修改共享内容；注册 fade 和 sound 修改当前注册，空覆盖保留 `None`。字段旁显示继承/覆盖及有效值，共享 Clip 显示注册数量。右键检查其他注册不会更换正在播放的注册，检查锁保留当前注册身份。
- Expression 成员的参数、value 和 blend 使用普通 PropertyForm；未配置、未解析目标和默认 Add 与显式 Add 保持区别。创建、移除、上下移和跨 Expression 移动使用原生操作；数值使用原生局部候选更新，整次手势一条历史。重复字段携带稳定成员 UUID，并捕获不回退的文档 revision；过期事件不按新数组位置重试。
- MotionGroup、注册和 Expression 列表接入共享拖放状态、落点反馈、600ms 悬停展开、边缘滚动与取消；一次只缓存当前落点规划，释放时重规划。菜单、Alt+Up/Down 和拖放共用 core 移动语义。注册/成员“移到…”复用可搜索选择器，搜索和绘制不逐个规划。
- 位置操作只检查和定位结果，不调用 Motion/Expression 激活。目的地选择器声明保持普通播放的策略，仍阻断其他输入；关闭、释放、文档或工作区变化后策略失效。其他阻塞弹窗保持既有暂停行为。组改名、重排、移组继续保留注册、覆盖、时间与播放代次。
- 补齐新能力的原生查询与中英文文案。字段适配只重定向 MotionRegistration/ExpressionEntry，Physics 的稳定通道字段保留原拥有者；Timeline 的临时选择属性也保留原有作用域与撤销后的身份语义。

本轮是 U3 的首轮交互接入；Motion 内容复制/添加已有引用、组和资源删除影响审阅仍需继续。未执行原生截图、实际窗口和 Release 性能测量，这些不由 headless 行为通过替代。

本轮最终定向门禁：

```sh
./scripts/test.sh regression -- \
  headless_tests::animation_authoring headless_tests::animation_panels \
  headless_tests::animation:: headless_tests::authoring headless_tests::properties \
  headless_tests::physics headless_tests::sorting headless_tests::palette \
  headless_tests::workspaces headless_tests::command_targets \
  headless_tests::input_routing headless_tests::ui_performance \
  panels::sorting::destination properties::form::tests \
  properties::timeline_tests properties::physics_tests \
  keybindings::tests menus::tests animation_library:: \
  animation::motion_authoring:: expression_authoring:: \
  commands:: keyboard:: timeline:: workspace::

cargo fmt --all -- --check
git diff --check
```

**UI / 表单 / 输入回归 199 项、core 集成回归 78 项通过**，包括新增 8 项 U3 UI 回归和 3 项 core 回归。新增用例验证空组可见、折叠组内创建与激活、Expression 空内容和成员焦点、注册覆盖清除、成员稳定字段与单条手势历史、实际拖放与快捷键、右键捕获与可搜索移组、播放策略不泄漏到后续阻塞弹窗、过期上下文及地址拒绝、单条创建历史和 no-op 保留 redo。此前单独测试与最终过滤集合重叠，不累计相加。

最终格式与空白检查通过；没有运行昂贵完整套件。

## U3 续轮：Motion 复制、已有引用与删除影响审阅

2026 年 10 月 23 日继续实施，仍使用 `zipzi-2d 26afe00`，未修改相邻仓库。

- 行菜单区分“复制动作内容”“添加已有动作引用”和“复制动作组”。内容复制由原生 `DuplicateClip` 重建 Clip、轨道、关键帧和事件身份，外部资源共享；组复制重建组与注册身份，保留共享 Clip、注册覆盖和引用集合。核心负责唯一名称与稳定相邻插入，复制和定位检查结果不激活预览。
- Motion 工具栏和行菜单共用可搜索引用选择器，捕获目标组或相邻注册；同名 Clip 以 ID 区分，空组可以接收引用。确认添加单个注册，初始 fade/sound 使用继承值，不复制 Clip。选择时生成原生计划，搜索与绘制不规划；取消不产生历史，过期上下文拒绝执行。
- 移除当前注册、删除组及注册、解散组、删除动作内容及全部注册、删除 Expression 使用类型化 `AnimationDeletionPlan`。通过未提交的原生候选比较实际移除、迁到未分组、保留内容和外部资源，并列出删除集外的阻断引用；UI 不推断级联规则。
- 空组等无额外影响的删除直接执行；非空拥有者、共享内容和最后注册保护显示影响审阅。注册按组、原序号、Clip 名与稳定 ID 展示，曲线内部条目汇总数量；行列表虚拟化，冻结显示数据共享存储，不随播放刷新重算。确认执行审阅过的同一原生计划，单条撤销历史；取消按钮默认聚焦，Enter/Esc 可取消。
- 原生“最后一个注册”语义保持：删除组可把最后注册迁到未分组，并保留 ID 与覆盖；移除最后根注册为 no-op，审阅说明原因并禁用确认。“删除动作内容及全部注册”是独立动作，明确列出全部注册、内部曲线和保留的外部资源。
- 两个弹窗复用可退休的播放策略；审阅/选择期间保持普通播放，其他输入仍受阻断。提交前校验文档 revision 和工作区上下文；Undo、切换文档或工作区往返后禁用确认。删除后仅检查幸存对象或邻居，不自动激活另一个 Motion/Expression。
- 复制计划仅新增身份和调整资源库顺序，发布时保留已有播放输入代次，避免复制出的轨道或原生回执标记的相邻注册导致暂停；普通 Undo/Redo、删除和现有内容编辑继续使用已有的播放协调策略，不自动激活复制结果。补齐中英文入口、解释和影响分类文案。

本轮最终定向门禁：

```sh
./scripts/test.sh regression -- \
  headless_tests::animation_management headless_tests::animation_authoring \
  headless_tests::animation_panels headless_tests::animation:: \
  headless_tests::authoring headless_tests::properties headless_tests::physics \
  headless_tests::sorting headless_tests::palette headless_tests::workspaces \
  headless_tests::command_targets headless_tests::input_routing \
  headless_tests::ui_performance panels::sorting::destination \
  properties::form::tests properties::timeline_tests properties::physics_tests \
  keybindings::tests menus::tests animation_management:: animation_library:: \
  animation:: expression_authoring:: commands:: keyboard:: timeline:: workspace:: \
  authoring::publication::

cargo fmt --all -- --check
git diff --check
```

**UI / 表单 / 输入回归 210 项、core 集成回归 125 项、core 单元回归 3 项通过**，包括新增 9 项 UI 和 8 项 core 管理回归。覆盖真实右键复制和引用选择、空组接收引用、共享组复制、审阅默认取消、最后注册保护、实际迁移与覆盖保留、共享 Clip 的所有注册和内部曲线影响、Expression 清理、空组直接删除、过期审阅/选择拒绝且不消耗 redo、深复制身份与单条历史、复制保持当前播放及已有曲线编辑仍使播放失效。此前单独运行的重叠测试不累计相加。

最终格式与空白检查通过；未运行昂贵完整套件。原生截图、实际窗口与 Release 性能验收继续留待 U6，不以 headless 行为验证替代。


## U4：Physics、Pose 业务集合与稳定子项

2026 年 10 月 23 日开始实施，依赖仍为 `zipzi-2d 26afe00`；本轮未改相邻仓库。

- core 新增 Physics/Pose 的类型化 `BusinessAction` / `BusinessPlan` 适配。单例默认内容、组的唯一默认名称、相邻 Pose 成员/links 插入位置、执行顺序和节点链位置由 core 解析稳定身份，再交给 SDK 原生规划。边界移动为 no-op，保留 redo；计划以原生版本冻结，确认后单条历史发布。
- 新建物理组、Pose 组、成员和 link 同时提供工具栏、上下文菜单、主菜单与可配置命令。不存在根资源时，首个组在一个原生事务内创建根；已有根时复用同一身份。Pose 初始组为空，成员与 link 的 Part 初始为未配置，不凭 UI 猜测业务目标。
- Pose 面板改为可编辑导航列表，使用虚拟行；根、组、成员和 links 可单击检查，方向键浏览、折叠，Alt+Up/Down 调用共享移动命令。空组可见，定位成员会展开其拥有者。继续显示同一 presented snapshot 的 Part opacity；检查、折叠和定位不切换 Motion/Expression 或写入历史。
- Pose 根 fade、组名、成员 Part 和 link Part 接入普通 PropertyForm；物理根 FPS 也可在无组时编辑。None、未配置和 `runtime:` 未解析引用分别保留，Part 选择可搜索并以稳定 ID 区分同名项。原生处理重复 Part/link、锁和引用有效性；不通过整段 JSON 修改。
- Pose 组提供“迁移成员到其他组”，明确选择接收组，连同 links 一起保留成员身份；不提供迁往虚构根成员集合的解散。成员/links 可选择合法新拥有者。Physics 输入/输出可跨组移动，输出必须明确选择目的采样节点，改变目的组后清空旧选择，禁止按节点位置猜测映射。
- 根、非空业务组及拥有 links 的成员删除展示冻结候选的实际移除集合；Pose 迁移展示将移除的组与保留身份的成员。取消默认聚焦，Enter/Esc 可取消；过期文档、工作区或 Undo 后拒绝提交。Part/Parameter 资源保留，最终校验和事务仍由原生负责。
- Physics 组排序明确称为提前/推后执行，结构变化沿用原生测试预览重建；输入/输出行菜单捕获点击条目，不依赖其他选择。节点继续走专用链结构编辑，补上链节点复制、前移和后移；输出的采样引用按节点身份保留。根节点只检查并保持其根身份，不变成下一节；被输出引用、最短链及锁由共享原生能力提示。
- Physics 输入、输出和节点选择保存稳定 ID，渲染时重算当前位置；重排、跨组移动、撤销/重做后检查和字段继续指向同一条目。复用节点参数的来源也从组/数组下标改为节点 ID。Physics 属性控件和 Pose 字段携带文档 revision，旧控件无法在 Undo 恢复后重用；普通表单候选仍保持单条手势历史。
- 业务动作复用共享输入结算、检查和定位。审阅/目的地弹窗复用可退休的播放策略；提交后不自动选择另一播放资源。删除 Physics 单例会退出已失去配置的手动试验并恢复原动画位置，Undo 不自动重启试验。

本轮最终定向门禁：

```sh
./scripts/test.sh regression -- \
  business_authoring:: headless_tests::physics \
  headless_tests::animation_authoring headless_tests::animation_management \
  headless_tests::animation_panels headless_tests::animation:: \
  headless_tests::authoring headless_tests::properties headless_tests::sorting \
  headless_tests::palette headless_tests::workspaces headless_tests::command_targets \
  headless_tests::input_routing headless_tests::ui_performance \
  input_routing::policy::tests panels::sorting::destination \
  properties::form::tests properties::timeline_tests properties::physics_tests \
  keybindings::tests menus::tests animation_library:: animation_management:: \
  animation:: pose_authoring:: expression_authoring:: commands:: keyboard:: \
  timeline:: workspace::

cargo fmt --all -- --check
git diff --check
```

**UI / 表单 / 输入回归 228 项、core 集成回归 140 项通过**，包括新增 10 项 UI 和 10 项 core 业务集合回归。覆盖根单例与空组、创建命令、稳定相邻插入、普通属性表单和引用状态、Part 修复、真实右键复制和 Alt+Up 排序、Pose 组迁移、删除默认取消、过期审阅拒绝、Physics 输出必须显式指定采样节点、节点链复制/移动与输出引用保持、根节点检查、Physics 子项重排与 Undo 的稳定选择、边界 no-op 保留 redo、执行顺序重建手动预览、删除 Physics 单例退出试验且 Undo 不自动重启。既有 presented Pose 值、手动试验、数字候选、输入路由、工作区与 U3 路径同时回归；重叠测试不累计相加。

最终格式与空白检查通过；未执行昂贵完整套件。原生截图、实际窗口与 Release 性能验收继续留待 U6。


### U4 Review 修复

- Physics 输入、输出和节点作为检查目标时，presented frame 继续通知 Properties，保持摆链和实时数值刷新。新增回归同时验证三类子项的通知与显示数值；发布帧后使用普通绘制，不以强制全窗口刷新掩盖通知遗漏。
- 删除最后一个输入或输出通道时，检查目标回退到所属物理组，保留属性面板与添加入口。新增回归覆盖输入和输出两种情况，并验证删除后可直接创建新通道、每次操作仅增加一条历史。

修复验证：以下定向 UI 门禁 **32 项全部通过**，包含新增 2 项回归；格式与空白检查通过。review 阶段的 10 项 core `business_authoring` 集成测试已通过，本次未改 core。

```sh
./scripts/test.sh ui -- headless_tests::business_authoring headless_tests::physics headless_tests::animation_panels headless_tests::ui_performance
cargo fmt --all -- --check
git diff --check
```


## U5-A：普通绑定创作闭环

2026 年 10 月 23 日完成本轮功能接入。普通绑定删除、快照规划接口及对应 SDK 回归已随 U5-B 配套修改一并提交到 `zipzi-2d e27230a`；此版本或兼容后继是本轮完整功能的依赖基线，见 `DEPENDENCIES.md`。

- core 新增 `OrdinaryBindingAction`、不可变捕获和版本化计划适配。Mesh、Warp、Rotation、Part 从合法目标创建零轴普通绑定，已有绑定直接打开；绑定、轴、采样点、所属目标始终以稳定身份定位。新轴可待配置，也可显式选 Parameter 并用其最小值、默认值和最大值生成初始关键值，最终合法性由原生校验。
- Properties 接入创建/管理绑定、目标形态定位、轴参数配置、关键值增改删、轴删除及绑定删除。可搜索参数选择保留未配置和缺失目标的区别；绑定根、轴和采样点可相互检查，不依赖其他面板当前选择或长期数组位置。
- 网格结构修改先审阅后应用。SDK 快照在后台按同一版本、预算和取消控制生成转换计划；审阅显示受影响目标、生成/重写/删除地址、转换保证、损失和容量。改草稿、关闭弹窗、文档/工作区/检查对象/参数预览变化使旧计划失效。确认执行同一计划，默认聚焦取消，不重新规划另一份影响范围。
- 有效轴删除必须明确保留已存储切片，最后一个有效采样点不能通过普通删除绕过降维策略。SDK 新增 `BindingGridEdit::Delete`：明确丢弃绑定拥有的轴、采样点及形态，保留共享 Parameter 和原基础对象；报告列出真实删除地址。Part 绑定仍关联 Offscreen 映射时明确拒绝并解释需专用映射转换，本轮不猜测替代映射。
- Keyform 面板忽略待配置轴来绘制现有有效网格，保留一/二维参数 pad。更高维度使用虚拟化的已存储形态列表选择完整坐标，画布取消旧的两轴只读限制；实际写入仍要求精确命中已存储形态。pad 的点击和拖动捕获上下文及版本，结构修改、Undo 或切换后旧事件不得继续按旧点位写入。
- Mesh 形态继续走既有属性与画布编辑，Warp/Rotation 继续走专用控制点/旋转编辑。Part 提供明确的已存储形态绘制顺序操作；绑定驱动时隐藏会写入静态基础值的普通字段，冻结选中形态并拒绝过期预览。零轴绑定仍可编辑基础对象。
- `BindingInfo` 的轴和坐标改为共享不可变投影，检查和普通重绘不复制完整网格。Properties 的管理行、已存储形态列表及影响列表沿用缓存或虚拟化；只有显式审阅才构造转换计划。
- 修复共享输入边界中行内 HEX 文本被当作颜色弹窗取消的问题：合法行内文本在新操作前提交，未接受的弹窗或拖动仍取消。保留输入法、非法值恢复及单条手势历史规则。

最终定向门禁：

```sh
./scripts/test.sh regression -- \
  ordinary_binding:: headless_tests::ordinary_binding binding_authoring:: \
  mesh_properties:: properties:: canvas_objects:: headless_tests::canvas \
  headless_tests::authoring headless_tests::properties \
  headless_tests::ui_performance headless_tests::workspaces

./scripts/test.sh ui -- headless_tests::parameters headless_tests::hierarchy \
  headless_tests::docking_tabs headless_tests::animation_panels \
  headless_tests::business_authoring

cargo test --manifest-path ../zipzi-2d/Cargo.toml --locked \
  -p zipzi-sdk --test integration binding_authoring::

cargo fmt --all -- --check
git diff --check
# 相邻仓库的变更文件也完成 rustfmt 和 git diff --check。
```

**UI/输入/工作量回归 118 项、core 集成回归 94 项、core 单元回归 4 项，以及相邻 SDK 绑定集成回归 61 项通过**；UI 原有 1 项 ignored 保持原状态。新增 8 项 UI、7 项 core 集成及 1 项 SDK 回归，覆盖真实按钮创建、参数搜索、待配置轴配置、关键值增改删与降维审阅、修改草稿后重新审阅、默认取消、过期计划/控件、绑定删除与共享目标保留、Mesh 形态保存重开和 Undo/Redo、Part 形态、三轴画布编辑、共享网格投影及取消、创建前颜色文本/弹窗边界。与此前迭代重复的测试不累计。

格式及两个仓库的空白检查通过。未执行昂贵全套、原生截图或新的 Release 性能测量；这些验收仍按 U6 执行。**当时 U5-B 纹理资源闭环尚未开始（后续交付见下节）**，高级功能仍按第 8.1 节暂缓，不把本轮结果表述为整个 U5 完成。

补充导航核对：参数、层级树、停靠与动画/Pose 面板定向回归 **73 项通过**。首次补查发现 4 项停靠测试无法定位空 Pose 面板；在两个仓库各自 HEAD 的临时源码副本中复现相同失败，确认是既有空状态缺少统一面板根标识。已恢复 `AnimationPose` 根标识并保留 `animation-pose-empty` 提示标识，4 项失败随本轮定向回归通过；此修复不改变 Pose 数据或业务动作。

## U5-B：纹理资源闭环

2026 年 10 月 23 日完成功能接入。SDK `import_prepared_image` 原子导入接口及资源回归已与 U5-A 的相邻仓库修改一起提交到 `zipzi-2d e27230a`；编辑器依赖记录已同步，两个仓库分别提交交付。

- Modeling 的 Resources 面板提供“新建纹理”和“导入 PNG”。New 立即创建空资源、检查并聚焦 Properties 名称；Properties 再选择图像，取消选择保留空资源。直接 Import 在后台解码成功后才以一次历史事务创建资源并写入内容，取消、无效图像或过期结果不创建条目。
- Mesh Properties 增加可搜索的纹理选择，支持未配置、现有空资源与已有图片资源；导入的未解析引用保留原状态，直到用户显式替换。引用修改使用原生 `SceneField::Texture`，不把引用或图像资源拼装成 UI JSON。空资源仍保留 Mesh 的求值顶点和几何编辑能力。
- Texture Properties 提供名称、空内容/缺失来源/已就绪状态、图片尺寸、存储来源、实际使用者及检查导航。选择/替换内容/修复来源共用加载入口；替换保持资源 ID，并清除原缺失来源。名称编辑使用原生资源 patch，属性地址包含当前资源版本。
- 删除采用版本绑定的原生计划，弹窗列出全部实际依赖；有使用者时必须显式选择另一资源，将使用者重定向后原子删除。默认焦点是取消；未选替代资源、锁定使用者或过期计划均不得发布。不提供静默清除引用或删除 Mesh 的退路。内容替换与删除时重定向使用者在文案及入口上明确分开。
- PNG 读取、解码及哈希在后台执行。已有资源使用原生 `ImageLoadToken`，配合宿主请求序号和编辑上下文；取消加载、后续请求、Undo、删除后恢复、文档/工作区切换均不得让旧结果误写。直接导入还校验捕获的文档版本；由于资源尚不存在，它在原子创建前不申请已有资源令牌。加载中提供取消入口，原生文件选择器也可取消。
- 发布后核对求值版本及纹理快照，清除宿主缓存场景，保证所有消费者和下一次视口请求采用新图片。仅名称变化保留纹理快照及求值帧。准备好的图片由原生层保留，保存、Undo/Redo 和重开不依赖最初选择的外部文件。
- 格式及容量边界沿用 C5：PNG、编码数据最多 64 MiB、单边最多 16384、解码 RGBA 最多 256 MiB。本轮未扩展格式或附件管理。

定向验证全部通过（两组不重叠）：

```sh
./scripts/test.sh regression -- resource_authoring:: mesh_properties:: ordinary_binding:: \
  headless_tests::texture headless_tests::mesh_properties headless_tests::authoring \
  headless_tests::ui_performance

./scripts/test.sh regression -- scene_content:: scene_lifecycle:: business_authoring:: \
  scene_creation:: deformation_creation:: headless_tests::scene_creation \
  headless_tests::deformation_creation headless_tests::business_authoring \
  headless_tests::animation_authoring

cargo test --manifest-path ../zipzi-2d/Cargo.toml --locked \
  -p zipzi-sdk --test integration resource_authoring::
```

结果：**53 项 core 集成、87 项 UI、19 项 SDK 资源测试，共 159 项通过**。新增 4 项 core、7 项 UI、1 项 SDK 回归，覆盖实际按钮与选择器、新建/导入取消及坏图像、单步历史导入、空纹理配置给 Mesh、缺失来源修复、依赖审阅与替换、锁定使用者、旧请求及文档切换、缓存纹理刷新、仅重命名复用帧、保存重开及 Undo/Redo。格式与两个仓库空白检查通过；未执行昂贵全套、原生截图或 Release 性能测量，完整验收仍随 U6。

### U5 收口范围核对

| 领域 | 本轮状态与实际入口 | 尚未接入的高级功能 |
| --- | --- | --- |
| 普通 MeshBinding / SceneBinding | 已接入：目标 Properties 创建/管理，Keyforms 轴与存储形态定位，画布编辑 | 更高级绑定种类不纳入普通绑定闭环 |
| Texture / ImageAsset | 已接入：Resources 创建/导入，Mesh Properties 选择，Texture Properties 替换/修复/删除及使用者导航 | 资源复制、任意重排 |
| Mesh 几何/UV/三角形、Warp 网格/Bezier | 保留：画布及现有专用内容编辑 | 拓扑元素通用管理列表与完整生命周期按钮 |
| 遮罩及其他已有引用 | 保留：Mesh Properties 等既有消费端配置和检查，纹理引用补齐 | 全领域统一引用管理器 |
| Document / Layout / ExportSettings | 保留：既有文档 Properties | 为覆盖率铺开的全量设置创作 |
| Attachment / sound / UserData | 保留：既有消费入口和对象/拥有者检查 | 完整附件库、声音及 UserData 配置闭环 |
| DrawOrderGroup / ModelHitArea / ModelParameterGroup | 保留：DrawOrder、Relations、拥有者及 Properties 中既有检查/编辑 | 绘制组完整管理、点击区域和行为集合新增创作 |
| BlendBinding / BlendKeyTable / BlendConstraint / DeltaKeyform | 保留：BlendShapes 及已有对象检查、诊断 | 完整 BlendShape 创作、共享表转换、约束与增量形态管理 |
| Glue / GlueBinding / GlueVertexPair | 保留：Glue、Properties 及既有内容编辑 | 绑定、顶点配对和转移的完整专用交互 |
| Offscreen 形态及映射 | 保留：U2 的树创建/归属管理及既有检查 | 合成形态、索引映射的专用创作和转换 |
| CdiCombinedSet | 保留：Relations、拥有者及既有诊断/检查 | 主动创建、完整成员管理 |
| SourceCanvas / 不透明扩展 / 编辑器状态 | 保留：来源检查、格式保留和既有会话状态策略 | 任意 JSON 编辑器或通用 New/Sort |

以上“尚未接入”均为本轮明确暂缓的 UI，不代表核心不支持。U5-A 与 U5-B 的必做功能已接入；U6 继续围绕已交付范围做清理、实际窗口、中英文原生截图和性能验收，不自动扩大为暂缓领域的新功能开发。


## U6：工程清理与非 UI 复核

2026 年 10 月 23 日：在当前 checkout 继续，SDK 保持 `e27230a`，没有修改相邻仓库。

- 删除 5 个已被共享移动计划替代的旧 `EditorCommand` 分支及未使用的移至根组方法，文档切换回归验证实际使用的 `execute_palette_move`。保留 Properties 联合事务及 core 测试仍需要的底层组织 API。
- 修复纹理删除使用者列表和绑定审阅报告被 flex 压缩的问题，列表有明确视口、完整内容可滚动；纹理使用者改为共享不可变身份数组与虚拟化行。256 使用者回归覆盖窗口缩小、可见行数与最后一行可达；空纹理删除及 Undo 保留身份。
- 绑定转换保证改为 typed `GridGuarantee` 的中英文说明。补齐 IME 组合期间 Mesh、Parameter、ParameterGroup 创建命令的输入边界回归。
- 跨阶段回归 989 项通过、2 项原有忽略；清理与字段/列表定向门禁 95 项通过，随后纹理/绑定 16 项及空纹理/IME 2 项通过，过滤集合重叠不累加。Release CPU 完成 5 对全链对照和 5 对 Opacity 定向复核，后者更新＋预览 p50 为 0.328→0.318 ms；完整数据、波动与证据边界见 [U6 记录](unified-object-authoring-u6-acceptance.md)。

用户确认 **跳过后续自动化 acceptance，由用户人工验收 UI**。新增 acceptance 场景和 README 入口已撤去，已有截图仅作人工参考；排队的 Timeline/实际窗口验收已停止。U6 工程部分完成，UI 最终状态待人工确认，不把阶段标记为全部验收通过。U5 暂缓领域继续保留原范围。
