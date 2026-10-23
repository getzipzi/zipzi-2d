# 对象交互 UI 实施记录

对应[UI 方案](unified-object-authoring-ui.md)。2026 年 10 月 23 日开始实施，当前交付 **U1 的共享适配基础，以及 U2 的 Parameter/组织树与变形树创建及命令入口、共享移动、树选择、Parameter 文件夹导航、多选移动快捷键与“移到…”切片**，尚未完成 U1–U6 全部验收。

## 首轮实现：创建与输入边界

- `editor/src/authoring.rs` 统一对象操作的文档/工作区上下文、输入结算和结果检查。延迟菜单同时捕获 SDK 文档 revision，拒绝已删除再 Undo 恢复的旧目标；完成输入结算后才调用类型化 core planner。能力显示区分 Unsupported（不展示）与 Unavailable（禁用并保留原因），先接入 Physics 复制、删除和排序按钮。
- `editor/src/editing.rs` 从 Save 提取统一输入边界，接入 Parameter 管理、Physics 列表操作、Motion 创建、资源行激活、文档/工作区切换、Open/Import、New、GenerateMesh 和文档 Undo/Redo。合法文本提交，非法文本恢复并显示原因；数值、颜色、Timeline 与画布的未完成候选取消。候选取消先于 Redo 可用性查询；原生文本的快捷键和菜单路由继续优先。没有用全局暂停替代输入结算，因此纯展示操作不会因边界本身暂停普通播放。
- 结算后的字段值与 revision 同步回控件，阻止排队的 Blur/Change 或迟到 Release 重复提交。保存/关闭后台文档不清理当前文档的输入捕获。输入法组合未完成时阻止依赖输入结算的操作。
- 新建后通过 `OperationResult.suggested_selection` 检查结果、显示 Properties，并在延迟回调中复核文档、上下文、revision 和检查对象后聚焦名称。资源检查使用独立检查接口；共享适配不把重排伪造成一次预览点击。Parameter 的祖先展开和滚动沿用现有定位器。
- `editor-core/src/authoring/creation.rs` 统一 Parameter/ParameterGroup 创建默认值、内部 ID、runtime ID 与稳定插入位置。名称可以由 UI 本地化；关系、名称校验、身份、锁和最终提交继续由 core/SDK 负责。
- Parameter 工具栏、命令和行菜单共用创建路径。普通参数之后插入；单个文件夹内追加；空组可嵌套；同容器多目标按完整文档顺序确定末尾，跨容器回到根集合。右键使用点击行上下文，延迟命令不重新读取全局选择。创建与后续名称编辑分开进入历史，Esc 不删除新对象。

## 阶段状态

| 阶段 | 当前状态 | 后续内容 |
| --- | --- | --- |
| U1 | 共享输入边界、操作上下文、能力显示与结果检查基础已接入上述入口 | 所有集合和旧入口的完整能力映射与统一结果定位尚未收口 |
| U2 | Parameter 与组织树创建、组织树新建项过滤显露；Parameter/文件夹/Part 组织树的 core 移动规划、跨组/嵌套、树多选拖放、过滤排序与拒绝原因已接入；树 Shift/Cmd 选择、焦点导航及可配置上下移已接入；组织树和 Parameter 可通过可搜索“移到…”选择指定容器；右键和快捷键支持多选分段上下移，变形树限同一实际父级；Parameter 文件夹单击检查、独立折叠与列表键盘导航已接入；场景及 Parameter 创建主菜单、场景可配置创建快捷键已接入；变形树 Mesh/Warp/Rotation 创建、双投影位置与显露已接入 | 变形父级保持形态移动仍待实施 |
| U3 | 保留 Motion/Expression 原预览路径，并在资源激活前结算输入 | 注册/Clip 分区、Expression 创建、空 MotionGroup 与列表排序尚未接入 |
| U4 | Physics 列表接入共享边界、能力与检查结果 | 组/子项默认内容、Pose 和稳定子条目控件仍需统一；本轮不声称移除了现有 Physics 默认数据拼装 |
| U5 | 未开始 | 绑定、场景内容、绘制组织、文档集合、资源和附件完整入口 |
| U6 | 未开始 | 清理剩余旧路径、完整中英文/原生截图验收和性能对照 |

## 首轮验证

Editor 与相邻 `kasane-2d` 都在独立 worktree 中。依赖固定为 `kasane-2d` 的已提交版本 `4bb2fee`；本轮没有修改该仓库产品源码，也没有使用原 checkout 的未提交更改。

```sh
# 受影响 UI、属性表单和输入路由
# 多个过滤词由测试 harness 取并集，不执行无关全套。
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --lib -- \
  headless_tests::authoring headless_tests::parameter_creation \
  headless_tests::parameter_groups headless_tests::parameter_management \
  headless_tests::parameters headless_tests::properties headless_tests::physics \
  headless_tests::animation_panels headless_tests::workspaces \
  headless_tests::command_targets headless_tests::input_routing \
  headless_tests::ui_performance properties::form::tests

# 创建规划和组织关系
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core \
  --test integration organization_authoring::

cargo fmt --all -- --check
git diff --check
```

新增回归覆盖创建后的名称选区、独立改名历史、命令来源与完整集合插入、合法/非法文本跨创建边界、Save 取消数值拖动与迟到 Release、候选存在时 Redo、折叠父组内创建空子组、工作区往返后的过期操作，以及核心嵌套/多目标/跨容器创建与原 ID Undo/Redo。

验证结果：受影响 UI / 表单 / 输入路由 / 虚拟化工作量共 **112 项通过**；组织创建核心回归 **7 项通过**；格式和空白检查通过。专项回归与此前运行有重叠，不重复累计。

本轮未运行昂贵全套、原生截图或新的 Release 性能测量。


## 第二轮实现：共享移动规划与交互

2026 年 10 月 23 日：依照用户要求，直接在已搬回的当前 checkout 继续，不再创建 worktree。本轮依赖 `kasane-2d b86b13b`，没有修改该仓库。

- 新增 `editor-core/src/authoring/moving.rs`：`PaletteMove` 捕获投影、源对象集合、稳定目标和 Before/After/Inside；规划转成现有 Organization/Scene SDK 计划。原生集合提供顺序，SDK 负责祖先归并、锁、循环、整体预校验、no-op、过期计划拒绝和单条历史。`plan_palette_step` 用同一适配生成菜单上下移的计划，按完整集合定位相邻项。
- `panels/sorting.rs` 移除 UI 的父级表、循环遍历和同父级/组嵌套限制。命中与释放使用同一 core 规划入口，释放时重规划并校验上下文；不再调用原来各自分散的排序 `EditorCommand`。旧公共命令仍保留给其他调用者，未宣称完成全部旧入口清理。
- 文件夹支持进入另一个文件夹，Before/After 支持合法跨父级移动；Part 组织移动可携带现有树多选集合。SDK 归并已选祖先的后代，并保持来源投影顺序；锁定成员导致整个计划失败。多选拖放不伪造单击，也不更改当前选择。
- 手动排序视图在搜索/类型过滤下仍可拖放，通过完整集合的稳定对象锚点确定位置，不清除筛选、不用可见行号写模型。保留现有虚拟列表、边缘滚动、Esc、失活和文档切换取消行为。
- 参数、文件夹和层级树行菜单增加单对象上移/下移。多选状态明确禁用这些单对象菜单项并说明使用拖放，避免只移动多选集合中的点击项；完整多选菜单与可配置快捷键尚未完成。
- 合法移入显示容器边框，排序显示插入线；无效落点在拖动预览中显示核心原因，锁定和循环原因提供中英文说明。600ms 连续悬停展开，离开后再进入重新计时；合法 Inside 落点在释放后立即展开目标。
- 一次拖动只缓存当前落点的规划，不为每个经过的行保留文档计划，也不在 begin 克隆整棵树建立父级表。重复悬停输入及重绘复用同一规划，释放后重新校验。

变形树本轮只迁移同父级排序。改变实际变形父级仍需独立的保持形态转换计划，不提供未经转换的 Inside 落点或静默 KeepLocalData 替代。层级树创建、键盘和完整选择行为继续按 U2 推进；U3–U6 状态不变。

第二轮验证：

```sh
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --lib -- \
  headless_tests::sorting headless_tests::hierarchy headless_tests::authoring \
  headless_tests::parameter_creation headless_tests::parameter_groups \
  headless_tests::parameter_management headless_tests::parameters \
  headless_tests::properties headless_tests::physics headless_tests::animation_panels \
  headless_tests::workspaces headless_tests::command_targets \
  headless_tests::input_routing headless_tests::ui_performance properties::form::tests

env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --test integration -- \
  palette_moves:: organization_authoring:: sorting::
```

受影响 UI 回归 **149 项通过**，包含新增的 8 项移动 UI 回归；其中悬停规划工作次数测试验证连续输入和重绘不重复构建计划。新增 core 移动专项 5 项通过。core 受影响过滤门禁共 **21 项通过**，包含新增移动专项 5 项；格式与空白检查通过。本轮未运行完整套件、原生截图和 Release 性能基准。


## 第三轮实现：组织树创建与筛选显露

前两轮代码已提交为 `65dd84a`。依照用户“commit 后继续”的要求，本轮在当前 checkout 接续 U2；依赖仍为 `kasane-2d b86b13b`，没有修改相邻仓库。

- 新增 core `SceneObjectType` / `SceneObjectCreationPlan`：统一 Part、空 Mesh、Warp、Rotation、Offscreen 的创建规划。UI 只提供本地化名称与捕获的上下文；core 分配 UUID、按拥有者避重默认名称、确定完整集合插入锚点，SDK 生成数据默认值、验证归属/锁并发布单条历史。
- 单个 Part 内追加；普通行同级之后创建；同拥有者多选在完整顺序的最后选中项之后创建；跨拥有者选择回到根集合。根部 Part 与未归属对象按各自显示分支确定顺序。组织树中的变形器行只表示组织归属，创建不会意外改变实际变形父级。
- 组织树工具栏提供新建菜单，行菜单捕获点击行，空结果/根行提供根集合菜单。菜单打开前结算输入，一次规划后捕获版本；切换文档/工作区或修改文档后拒绝旧菜单。锁定与 Offscreen 缺少 Part 归属保留禁用原因和中英文说明。
- 创建成功后复用共享结果检查，选中对象、展开祖先、滚动、显示 Properties 并聚焦名称。后续改名与创建分别撤销。
- 搜索或类型排除时，临时显露新对象及其祖先，标记“新建”；不扩散到兄弟或后代，也不改写筛选条件。已有清除筛选按钮仍可用；修改筛选、选择其他对象或切换文档会清理临时显露。

变形树创建尚未接入。当前 SDK 的 `SceneEdit::Create.before` 定位组织集合，变形顺序仅追加，不能原子表达在完整变形同级顺序的指定位置插入；需补充类型化创建位置契约后再接 UI，详见[核心方案第 6.1 节](unified-object-authoring-core.md#61-ui-u2-发现的后续创建位置契约)。本轮没有用第二条移动历史补齐位置。

第三轮定向门禁：

```sh
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --lib -- \
  headless_tests::scene_creation headless_tests::hierarchy headless_tests::sorting \
  headless_tests::authoring headless_tests::properties headless_tests::panel_toolbar \
  headless_tests::command_targets headless_tests::ui_performance \
  panels::hierarchy_tree::filter

env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --test integration -- \
  scene_creation:: scene_lifecycle:: palette_moves:: organization_authoring:: sorting::

cargo fmt --all -- --check
git diff --check
```

受影响 UI/筛选/工作量回归 **92 项通过**（包含新增的 6 项创建 UI 回归和 1 项筛选显露单测）；core 创建、场景生命周期、移动和组织回归 **31 项通过**（包含新增的 5 项创建回归）。格式与空白检查通过。没有运行完整套件、原生截图或 Release 性能基准。U2 仍未全量验收；主菜单/快捷键创建、变形树创建和阶段状态表中的剩余内容继续待接入。


## 第四轮实现：树选择与可配置移动快捷键

第三轮组织树创建已提交为 `5359c2d`。本轮继续在当前 checkout 工作，没有创建 worktree 或修改相邻 `kasane-2d`。

- 新增层级树 `selection.rs`，以稳定对象引用保存 Shift 锚点，以稳定行键保存键盘焦点。Cmd 切换成员；Shift 以当前可见行作范围；Cmd+Shift 合并范围。反向范围仍以点击项作为 primary，重复 Shift 保留原锚点。过滤和折叠不会自动清除模型中的既有选择；范围不纳入隐藏行，也不把行号当对象身份。
- 普通上下键移动行焦点、滚动到焦点，Enter 才更新选择；Shift+上下扩展选择，左右键折叠/展开或进入子级/返回父级，Home/End 定位首尾。箭头和 Enter 不截获搜索框输入；父级折叠保留隐藏成员的选择。文档或工作区 token 改变后重置光标和锚点，删除对象后校正锚点。
- 新增稳定命令 `palette.move-up` / `palette.move-down` 和 Palette 快捷键作用域，默认 Alt+Up/Down，可通过现有 `keybindings.json` 改键。组织树、变形树同父级和 Parameter/文件夹的单对象移动调用现有 `plan_palette_step` / `execute_palette_move`，不新增领域排序规则。右键上下移显示同一命令的快捷键。
- Palette 命令按当前可见面板的精确列表焦点解析目标，树中使用焦点行，Parameter 使用检查目标；移动不伪造选择或激活。文本、搜索框、弹出控件、隐藏面板和活动拖拽不会触发这一入口。匹配的 Palette 快捷键优先于悬停画布的同键映射，其他窗口/文档/画布路由保持现有规则；冲突检查把 Palette 与 Document 视为重叠作用域。
- Parameter 名称和文件夹检查按钮在选择前结算 Properties 输入，并将键盘焦点放回列表。树选择也使用统一输入边界，防止旧字段在选择改变后补交。
- 单对象键盘移动与原菜单一致：多选集合中任一项聚焦时拒绝单项移动并说明使用拖拽；锁定、边界 no-op、输入结算及错误仍由共享适配和 core/SDK 处理。多选上下移和“移到…”菜单尚未实现，不声称已经覆盖全部移动入口。

第四轮定向验证：

```sh
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --lib -- \
  headless_tests::tree_selection headless_tests::palette_commands \
  headless_tests::hierarchy headless_tests::sorting headless_tests::scene_creation \
  headless_tests::parameters headless_tests::parameter_groups \
  headless_tests::parameter_management headless_tests::authoring \
  headless_tests::properties headless_tests::panel_toolbar headless_tests::command_targets \
  headless_tests::input_routing headless_tests::ui_performance \
  keybindings::tests input_routing::policy

env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --test integration -- \
  commands:: keyboard:: sorting:: palette_moves::

cargo fmt --all -- --check
git diff --check
```

受影响 UI/路由/工作量回归 **140 项通过**，含新增 5 项树选择 UI、3 项移动命令 UI 和 1 项路由优先级单测；既有 keymap 单测补充作用域隔离断言。core 命令/键盘/移动回归 **33 项通过**，含新增的改键和作用域冲突回归。格式和空白检查通过。本轮未运行完整套件、原生截图或 Release 性能基准；U2–U6 仍按阶段状态表继续。


## 第五轮实现：指定容器移动

第四轮树选择和移动快捷键已提交为 `412c7ee`。继续使用当前 checkout，依赖 `kasane-2d b86b13b`；本轮没有修改相邻仓库。

- core 新增轻量 `PaletteMoveDestination` 与 `palette_move_destinations`。根集合、Part 和 ParameterGroup 按原生容器顺序提供稳定身份与完整祖先路径；空容器也可选择，同名路径保留不同身份。使用迭代遍历，不复制整棵场景树及非容器对象，也不为每个容器生成移动计划。
- 组织树和 Parameter/文件夹行菜单增加“移到…”；点在现有树多选内时冻结整个集合，点在集合外时只捕获点击对象。目标选择器可搜索完整路径；同名路径附加内部 ID 以区分。根集合是明确选项，成功移动追加到目标的完整顺序末尾。
- 新增 `palette.move-to` 命令，使用既有 Palette 作用域和改键系统，无默认快捷键；列表焦点与行上下文沿用第四轮策略。菜单与快捷键共用同一目标选择器，实际操作调用 `plan_palette_move` / `execute_palette_move`。SDK 决定祖先归并、来源顺序、锁、循环、Offscreen 归属和整体原子性。
- 搜索和重绘不规划。选定目标后只保存一个当前计划以显示校验结果，确认时重新规划、复核文档/编辑 token 与 revision，再执行一条历史。打开前结算输入并结束阻塞编辑，然后捕获对话框上下文；修改文档后及时禁用提交。取消、无目标和 no-op 不消费历史或 redo。
- 不合法目标保留可读原因，禁用移动；补齐 Part 的原生 `RELATION_CYCLE` 映射，以及 Offscreen 必须归属 Part、拥有者已存在合成与需要转换的中英文反馈。对话框复用 `FormDialog` 的单次提交生命周期，选中目标的 Enter 只确认下拉选项，不同时提交移动。
- 成功后不伪造单击、不改变检查或原多选集合。组织树展开结果路径并滚动，Parameter 定位器可按显式对象定位而不改写参数焦点。筛选下临时显露移动对象及祖先并标记“已移动”，保留查询/类型条件，沿用清除筛选入口；延迟定位也复核执行后的上下文。

变形树不展示“移到…”菜单；变形父级改变仍需保持形态转换计划。多选上下移、Parameter 文件夹选择导航、其余创建命令入口，以及 U3–U6 仍未完成，不把本轮交付表述为 U2 全量验收。

第五轮定向门禁：

```sh
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --lib -- \
  headless_tests::palette_move_to panels::sorting::destination::tests \
  headless_tests::tree_selection headless_tests::palette_commands \
  headless_tests::hierarchy headless_tests::sorting headless_tests::scene_creation \
  headless_tests::parameters headless_tests::parameter_groups \
  headless_tests::parameter_management headless_tests::parameter_creation \
  headless_tests::authoring headless_tests::properties headless_tests::panel_toolbar \
  headless_tests::command_targets headless_tests::input_routing \
  headless_tests::ui_performance keybindings::tests input_routing::policy \
  panels::hierarchy_tree::filter

env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --test integration -- \
  palette_moves:: organization_authoring:: sorting:: keyboard:: commands::

cargo fmt --all -- --check
git diff --check
```

受影响 UI/路由/筛选/工作量回归 **160 项通过**，其中新增 10 项“移到…” UI 回归和 1 项规划工作量测试。工作量验证搜索及 8 次连续通知重绘不规划，目标选择后 1 次，提交前重规划累计 2 次。core 移动、组织、命令和快捷键回归 **43 项通过**，含新增 3 项目标元数据/空容器/重名身份回归。格式与空白检查通过。本轮未运行完整套件、原生截图和 Release 性能基准。


## 第六轮实现：多选上下移动

第五轮指定容器移动已提交为 `25bf0f9`。本轮继续在当前 checkout 实施；依赖仍为 `kasane-2d b86b13b`，没有修改相邻仓库。

- 新增 core `plan_palette_steps`，单对象 `plan_palette_step` 也委托此入口。按完整原生集合识别连续选区，每段跨过一个未选邻居，保留段内顺序和选区间隔；边界选区不动。向上从前往后、向下从后往前规划，并按稳定对象锚点解析候选顺序，不使用过滤后的行号。
- 组织树与 Parameters 可同时处理多个拥有者集合；根 Part/未归属对象、根变形器/根 Mesh 和文件夹/参数按各自显示集合处理。原生组织子树查询归并被选祖先内的后代，锁检查覆盖全部源子树，包含已经到达边界的源。锁定成员使整个操作失败，避免只移动其他选区。
- 规划只克隆一次原生 COW 文档，在候选上逐段执行原生 Organization/Scene 计划；执行通过 SDK 单个 `edit` 组合所有意图，使用捕获版本，原子提交一份 receipt 和一条历史。没有增加独立撤销栈。全选或边界 no-op 保留 redo；过期计划即使经过 Undo 恢复相同内容也拒绝执行。
- 右键上下移与可配置快捷键冻结同一源集合：点击/焦点在选择内使用全体，否则只使用该行。菜单回调不重新读取选择，提交前复核上下文并重规划。移动保留检查与多选；成功后复用带上下文保护的展开、滚动与过滤显露定位。
- 变形树只支持同一实际父级内的多选上下移；跨父级选择显示中英文原因。此入口不改变实际变形父级，也不使用 KeepLocalData 替代保持形态转换。原生规划错误以类型化错误码提供给 UI，继续复用锁定等本地化反馈。

第六轮定向门禁：

```sh
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --lib -- \
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

env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --test integration -- \
  palette_steps:: palette_moves:: organization_authoring:: sorting:: keyboard:: commands::

cargo fmt --all -- --check
git diff --check
```

受影响 UI/路由/筛选/工作量回归 **166 项通过**，包含新增 6 项多选上下移 UI 回归；core 受影响门禁 **51 项通过**，包含新增 8 项分段移动、祖先归并、多拥有者、混合参数/文件夹、变形同级、边界与锁、过期计划及 Undo/Redo/保存回读回归。格式与空白检查通过。本轮未运行完整套件、原生截图或 Release 性能基准。

U2 仍未完成：变形树创建的位置契约、保持形态改父级、Parameter 文件夹选择导航与其余创建命令入口继续待实施；U3–U6 状态不变。


## 第七轮实现：Parameter 文件夹选择与键盘导航

第六轮多选上下移已提交为 `24601bc`。本轮继续使用当前 checkout，没有创建 worktree 或修改相邻 `kasane-2d`。

- Parameter 文件夹名称单击检查文件夹，箭头独立展开/折叠；原检查按钮复用同一激活路径。参数仍使用既有 `FocusParameter`，文件夹使用 `Navigate`，保留场景选择、参数预览值和参数焦点。空文件夹也可以检查，折叠不清除隐藏对象的检查身份。
- 新增仅用于展示的稳定对象光标。上下键及 Home/End 只移动焦点并滚动，Enter 才激活；左右键展开/折叠、进入首个可见子行或返回父行。空文件夹不会跳入下一个根组；“未分组”有独立展示身份，可以折叠和导航，但不会提交对象检查或移动。
- Palette 的上下移与“移到…”快捷键使用当前可见焦点行，避免浏览到文件夹后仍操作旧的检查对象。重排后仍保持该稳定光标及原检查目标；删除或隐藏光标行后校正，Undo 不复活已失效光标。文档或工作区 token 改变后重建导航上下文。
- 单行检查与焦点分别显示背景、边框和选择元数据；只在状态变化时通知受影响的缓存参数行，保留原滑块与数值控件实体。普通预览值更新不重新协调整份导航样式。文件夹箭头补齐中英文可访问名称。
- 激活先通过共享输入边界结算旧 Properties 文本；模态、忙状态、拖拽、输入法字符输入和文本控件焦点不会触发列表导航。本轮延续 Parameter 的单行检查策略，没有新增 Parameter 的 Shift/Cmd 多选或批量属性编辑。

第七轮定向门禁：

```sh
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --lib -- \
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

第七轮 Parameter 文件夹导航已提交为 `db58fce`。本轮继续使用当前 checkout，依赖仍为 `kasane-2d b86b13b`；没有创建 worktree 或修改相邻仓库。

- 命令目录新增 `scene.create-part`、`scene.create-mesh`、`scene.create-warp`、`scene.create-rotation`、`scene.create-offscreen`。均为 Modeling 文档命令，无默认键位、不可按住连发，可通过现有 `keybindings.json` 配置；文本、搜索框、输入法字符输入和模态继续由既有路由隔离。原 `tool.generate-mesh` 保留其内容生成用途。
- “编辑 → 创建”主菜单列出五种场景创建，以及已有 Parameter 和 ParameterGroup 创建命令。场景命令在调用时捕获与组织树工具栏相同的场景选择，使用组织投影的完整集合位置，不因焦点处于变形树就隐式指定实际变形父级。右键继续使用显式点击行上下文。
- 新增共享 `authoring/scene_creation.rs`，组织树工具栏、右键、原生菜单与命令调用同一执行及结果协调路径。创建成功后显露结果和祖先、定位树、打开 Properties、选中名称；隐藏面板可重新打开。Properties 最后显示，支持树与 Properties 共用标签组的布局。延迟定位复核执行后上下文及检查对象；新建会重置树的范围选择锚点，移动仍保留原锚点。
- core 提供轻量创建上下文可用性查询，复用创建拥有者解析、SDK 集合能力和原生 Offscreen 校验；查询不分配对象身份、不计算避重名称、不枚举插入顺序、不生成或保存文档计划。实际创建才解析完整集合锚点并规划，继续由 SDK 校验最终数据、锁及事务。
- 原生菜单明确禁用锁定容器、缺少 Offscreen 拥有者或已有 Offscreen 的创建动作，并显示中英文原因。菜单缓存纳入创建原因，避免可用性一直为 false 时保留旧原因。过期文档目标、Animation、忙状态或模态不发布创建；输入结算和后续独立改名历史沿用共享边界。

第八轮定向门禁：

```sh
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --lib -- \
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

env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --test integration -- \
  scene_creation:: scene_lifecycle:: organization_authoring:: palette_steps:: \
  palette_moves:: sorting:: keyboard:: commands::

cargo fmt --all -- --check
git diff --check
```

受影响 UI/命令/移动/创建/输入路由/虚拟化门禁 **184 项通过**，包含新增 9 项创建命令 UI 回归：原生菜单真实动作、各命令来源、实际改键与文本/重复按键隔离、输入结算及过滤显露、禁用原因缓存更新、过期目标与工作区/模态拒绝、隐藏面板恢复、共用标签组名称聚焦、新建后的范围锚点。core 定向门禁 **63 项通过**，包含新增 2 项命令可用性/原生归属、历史与 redo 保留、稳定命令 ID、改键及冲突检查回归，并扩展已有忙状态断言。格式与空白检查通过。本轮未运行完整套件、原生截图或 Release 性能基准。

U2 仍未全量验收：变形树创建需要核心方案第 6.1 节的双投影插入位置契约；改变实际变形父级需要保持形态转换交互。U1 的全入口收口与 U3–U6 状态不变。


## 第九轮实现：变形树创建与双投影位置

第八轮创建主菜单和快捷键已提交为 `6db8b70`，没有包含其他任务已暂存的 SDK 设计文档。本轮在当前 checkout 继续；`kasane-2d` 以 `b86b13b` 为基础补充原生契约，Editor 与 SDK 本轮改动均尚未提交，没有创建 worktree。

- 补齐[核心方案第 6.1 节](unified-object-authoring-core.md#61-ui-u2-发现的后续创建位置契约)：原生 `SceneEdit::Create` 增加独立可选的 `deformation_before`，组织集合仍由 `part_id` / `before` 确定，实际变形集合由 `parent_id` / `deformation_before` 确定。创建 planner 校验两个拥有者、锁和稳定锚点，在同一创建 delta 内发布两份顺序；不使用创建后再移动的第二条历史。Offscreen 和 Glue 拒绝变形锚点。SDK 的版本校验、事务回滚、双集合变化摘要与 Undo/Redo 沿用原生路径。
- core 新增按明确投影创建的入口，计划携带投影供结果定位；原组织创建入口委托此入口。变形树只支持 Mesh、Warp、Rotation。单个变形器内追加，单个 Mesh 同父级之后插入，同父级多选按完整顺序的最后成员之后插入；跨实际父级选择回到根集合。根变形器与根 Mesh 保留各自展示集合顺序。
- 组织归属独立于实际变形父级：单个上下文沿用该对象的组织拥有者，同父级但组织拥有者不同的多选在组织根集合追加，同时保留共同实际父级。变形投影创建不顺带重排组织成员；名称继续按组织拥有者避重，数据默认值与最终校验由 core/SDK 提供。
- 变形树增加创建工具栏、行右键和根集合菜单，复用共享创建执行、Properties 名称聚焦、过滤下临时“新建”显露、展开和滚动。行菜单捕获点击行，不改用菜单打开后的全局选择；锁定目标禁用并显示原因，过期菜单整体拒绝。Part/Offscreen 不展示误导性的变形树创建项。
- 新增 `deformation.create-mesh`、`deformation.create-warp`、`deformation.create-rotation` 三个明确投影的文档命令，无默认键位，可通过现有改键系统配置；主菜单“编辑 → 创建 → 在变形树中新建”提供同一动作。命令目录使用可区分组织/变形创建的中英文名称，面板内菜单保留简短名称。已有 `scene.create-*` 继续明确表示组织投影，不从当前焦点暗中切换领域语义。
- 创建可用性继续使用轻量原生查询；补充原生集合验证入口，使变形集合能力查询只校验父级及锁，不枚举完整同级集合或构建逐项 SDK handle。实际计划解析完整顺序，过滤和折叠不影响稳定锚点。补充 Python API 文档；JSON 命令入口可直接传入新锚点，无需新增面板资产拼装逻辑。

第九轮定向门禁：

```sh
# kasane-editor
env -u KASANE_DEV_SESSION_DIR cargo test --locked -p kasane-editor --lib -- \
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

env -u KASANE_DEV_SESSION_DIR cargo test --locked -p editor-core --test integration -- \
  deformation_creation:: scene_creation:: scene_lifecycle:: organization_authoring:: \
  palette_steps:: palette_moves:: sorting:: keyboard:: commands::

# ../kasane-2d
cargo test --locked -p kasane-sdk --test integration -- \
  scene_creation_placement:: scene_lifecycle:: scene_content:: scene_authoring:: \
  organization_authoring:: hierarchy_order:: authoring_acceptance::

# 两个仓库分别执行
cargo fmt --all -- --check
git diff --check
```

受影响 UI/命令/创建/移动/路由/虚拟化门禁 **191 项通过**，包含新增 7 项变形创建 UI 回归；core 定向门禁 **69 项通过**，包含新增 6 项投影位置、默认归属、多选、根集合、不可用/过期与命令能力回归；SDK 受影响门禁 **78 项通过**，包含新增 6 项双锚点、双集合发布、三种数据默认值、错误集合/缺失/自身锚点、两个拥有者的锁、事务回滚、JSON、过期计划、单条 Undo/Redo 与保存回读回归。既有快捷键回归扩展稳定 ID 与冲突断言。测试集合与前轮有重叠，不累计相加。

两个仓库格式与空白检查通过。本轮未运行完整套件、Python 绑定构建/门禁、原生截图或 Release 性能基准。U2 尚未全量验收，剩余重点是改变实际变形父级时的保持形态转换交互；U1 全入口收口与 U3–U6 状态不变。
