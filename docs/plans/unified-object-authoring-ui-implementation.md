# 对象交互 UI 实施记录

对应[UI 方案](unified-object-authoring-ui.md)。2026 年 10 月 23 日开始实施，当前交付 **U1 的共享适配基础和 U2 的 Parameter 创建切片**，尚未完成 U1–U6 全部验收。

## 本轮实现

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
| U2 | Parameter/ParameterGroup 创建、嵌套空组、名称焦点和定位已接入 | 层级树创建、完整多选 UI、拖放/上下移统一、锁定反馈及过滤显露继续实施；现有排序路径尚未替换 |
| U3 | 保留 Motion/Expression 原预览路径，并在资源激活前结算输入 | 注册/Clip 分区、Expression 创建、空 MotionGroup 与列表排序尚未接入 |
| U4 | Physics 列表接入共享边界、能力与检查结果 | 组/子项默认内容、Pose 和稳定子条目控件仍需统一；本轮不声称移除了现有 Physics 默认数据拼装 |
| U5 | 未开始 | 绑定、场景内容、绘制组织、文档集合、资源和附件完整入口 |
| U6 | 未开始 | 清理剩余旧路径、完整中英文/原生截图验收和性能对照 |

## 验证

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
