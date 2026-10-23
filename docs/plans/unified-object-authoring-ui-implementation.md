# 对象交互 UI 实施记录

对应[UI 方案](unified-object-authoring-ui.md)。2026 年 10 月 23 日开始实施，当前交付 **U1 的共享适配基础，以及 U2 的 Parameter/组织树创建、共享移动、树选择与移动快捷键切片**，尚未完成 U1–U6 全部验收。

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
| U2 | Parameter 与组织树创建、组织树新建项过滤显露；Parameter/文件夹/Part 组织树的 core 移动规划、跨组/嵌套、树多选拖放、过滤排序、拒绝原因与单对象右键上下移已接入；变形树同父级排序使用相同规划入口；树 Shift/Cmd 选择、焦点导航及可配置单对象上下移已接入 | 变形树创建、变形父级保持形态移动、多选上下移、移到指定容器菜单、Parameter 文件夹选择导航与其余创建命令入口仍待实施 |
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
