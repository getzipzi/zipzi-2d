# U6 对象交互清理与验收

2026 年 10 月 23 日；对应 [UI 方案第 9 节](unified-object-authoring-ui.md#9-ui-验收与性能)。本轮在当前 checkout 实施，基线为 Editor `8b435dc`、SDK `e27230a`，没有修改相邻 SDK。U5 暂缓领域继续沿用第 8.1 节，不扩展为 U6 新功能。

**验收方式调整：用户确认跳过后续自动化 acceptance，由用户人工验收 UI。** 已执行的原生测试和截图保留为辅助证据，不替代人工验收；排队的 Release Timeline acceptance 和实际窗口 Properties 验收已停止，未产生对应的验收结论。CPU 属性链性能对照保留。

## 清理与验收发现

- 删除命令层已无 UI 调用者的 `ReorderTree`、`MoveOrganization`、`MoveParameter`、`MoveParameterGroupToRoot`、`ReorderParameterGroup` 分支及未使用的移至根组方法。工具栏、行菜单、主菜单和可配置快捷键继续使用共享捕获、规划与发布；文档切换拒绝测试改为验证实际使用的 `execute_palette_move`。
- 保留仍由 Properties 联合事务及 core 回归使用的底层组织 API。`sorting.rs::can_target` 仅处理投影命中、缓存和共享 core 规划；没有残留面板循环/锁/引用规则副本。现有面板没有继续拼装 Motion、Expression、Physics、Pose 或纹理资产。
- 本轮曾通过生产控件和命令生成中英文各 14 张对象创作截图。按最终人工验收边界，不保留新增 `authoring` acceptance 场景及 README 入口；已生成图片保留作人工检查参考。
- 截图发现纹理删除使用者列表会被 flex 压缩，少量条目也显示不全。改用明确高度、不参与压缩的虚拟列表；用户身份共享不可变数组，普通重绘只构造可见行。256 使用者回归验证滚动前后均少于 20 行参与布局，缩小窗口后最后一个使用者仍可定位；两条使用者的原生截图另检查两行全部处于视口内。
- 绑定影响列表同样固定视口高度，完整内容可滚动并有悬停说明；转换保证由 typed `GridGuarantee` 映射为中英文说明，保留插值吸附、降维和采样删除的含义。
- 补查空纹理删除与 Undo，以及输入法组合期间 Mesh、Parameter、ParameterGroup 创建命令不发布部分文本或额外历史。

## 场景与证据

| UI 方案验收项 | 行为证据与本轮补查 |
| --- | --- |
| 工具栏、右键、主菜单、快捷键 | `authoring`、`authoring_coordination`、`scene_creation_commands`、`palette_commands` 与 `menus` 回归；共享意图、捕获点击行、相同禁用原因 |
| 创建与空文件夹 | `parameter_creation`、`scene_creation`、`animation_authoring`；原生 `authoring-empty*`、`authoring-motion-empty-group*` 验证对象立即可见、隐藏 Properties 重新打开、Esc 保留对象 |
| 过滤与折叠 | `scene_creation`、`deformation_creation`、`parameter_navigation`；原生 `authoring-filtered-create*` 保留筛选文本并显露新对象与祖先 |
| Motion/Expression 激活 | `animation`、`animation_panels`、`animation_authoring`；既有重复点击、双击、暂停/播放触发和失败资源检查 |
| 注册身份、Clip 与覆盖 | `animation_management`、`animation_authoring`；原生 `authoring-motion-registration*` 检查分区、继承与覆盖显示 |
| 文本、数值、颜色及组合字段 | `properties`、`authoring`、`properties::form`、`slide_number_input`；原生 Properties 中英文范围与颜色截图 |
| IME、Save、Undo、上下文切换 | `properties`、`authoring`、`workspaces`、`command_targets`、`input_routing`；组合期间 New/Save/关闭不发布，完成组合后单次提交，旧事件不转写 |
| 移动、多选与锁定 | `sorting`、`tree_selection`、`palette_steps`、`palette_move_to`、保持形态审阅测试；悬停规划复用、600ms 展开、滚动、无效落点与整体拒绝 |
| Physics/Pose 稳定子项 | `business_authoring`、`physics`；原生 `authoring-physics-empty-group*`、`authoring-pose-member*` 与中英文 Physics 截图 |
| 删除、影响审阅与引用修复 | `animation_management`、`business_authoring`、`texture`；原生 `authoring-texture-delete-review*` 显示实际使用者并默认取消 |
| 普通绑定闭环 | `ordinary_binding`、core `ordinary_binding` / `binding_authoring` 与既有画布回归；原生 `authoring-binding-review*`、`authoring-binding-axis*`、`authoring-binding-keyforms*` |
| 纹理资源闭环 | `texture`、core `resource_authoring`；原生 `authoring-texture-empty*`、`authoring-texture-users*` 与依赖审阅；空内容、替换、修复、保存及历史保持 |
| 资源异步加载 | `texture` 覆盖取消、坏图像、旧请求、Undo 与切换文档拒绝；Attachment 保留原异步消费契约 |
| 范围边界 | [U5 收口范围核对](unified-object-authoring-ui-implementation.md#u5-收口范围核对)逐项保留；没有把暂缓 UI 入口标为核心不支持 |
| 渲染与缓存工作量 | `ui_performance`、`sorting`、`ordinary_binding` 和新增纹理依赖虚拟化回归；属性投影/选项缓存、Timeline 定向刷新与 hover 规划复用 |

上述 IME 检查向 GPUI 的真实输入控件注入组合事件；原生 Metal 截图使用无 OS 窗口的 renderer。实际窗口性能探针另行运行，三类证据分开记录，不宣称验证系统候选窗口或人工鼠标操作。

## 已执行验证与产物

以下记录已经执行的命令，不要求在人工 UI 验收前重跑 acceptance。

```sh
# 首轮跨阶段基线：989 passed，2 ignored（原有状态）。
./scripts/test.sh regression

# 清理、列表与字段边界的定向复核；计数不与首轮重复累加。
./scripts/test.sh regression -- sorting:: headless_tests::texture \
  headless_tests::ordinary_binding headless_tests::authoring \
  headless_tests::properties headless_tests::ui_performance
./scripts/test.sh ui -- headless_tests::texture headless_tests::ordinary_binding
./scripts/test.sh ui -- headless_tests::texture::new_texture \
  headless_tests::properties::properties_ime

# 实际模型、动画与原生 Metal。
env -u KASANE_DEV_SESSION_DIR cargo test --locked --workspace \
  --features kasane-editor/test-support --test acceptance \
  --test native_render --test animation_acceptance

# 当时截图版本含 authoring；最终已移除该新增场景。另跑了既有 Properties、Physics 与 compact。
env -u KASANE_DEV_SESSION_DIR \
  KASANE_SCREENSHOT_DIR="$PWD/target/u6-screenshots" \
  cargo test --locked --workspace --features kasane-editor/test-support \
  --test headless_screenshot
# 分别设置 KASANE_SCREENSHOT_SCENE=properties / physics / compact 后重复上一命令。

cargo fmt --all -- --check
git diff --check
```

日志：`target/u6-regression.log`、`target/u6-final-regression.log`、`target/u6-texture-binding.log`、`target/u6-final-boundaries.log`、`target/u6-native-acceptance.log`、`target/u6-screenshots*.log`。截图：`target/u6-screenshots/`；首次调试截图不作为最终证据。

Timeline 与实际窗口 acceptance 按用户要求跳过；UI 最终状态待人工验收。

## Release CPU 属性链对照

Apple M4（Mac16,10）、16 GiB；Mao 实际模型，Editor `8b435dc` 对比当前源码，SDK 同为 `e27230a`。先完成构建与其他测试，再交替执行 5 对 plain Release 进程；每进程 10 次预热、60 次手势、每手势 12 次更新，保留上一版只读索引。另一次 instrumented 进程用于阶段与工作量核对，不混入 plain 总耗时。每次运行验证修改、取消、历史与预览，源文件和模型哈希保持不变。

下表为每个进程自身分位数的中位数，单位 ms；更新与预览先按同一次输入配对相加，不相加独立分位数。测量仅包括同步 CPU 链，不包括 GPU、OS 输入或 compositor。

| 更新＋CPU 预览 | 基线 p50 | 当前 p50 | 基线 P95 | 当前 P95 |
| --- | ---: | ---: | ---: | ---: |
| Mesh Opacity | 0.380 | 0.451 | 1.739 | 2.162 |
| Mesh Multiply | 0.431 | 0.420 | 2.347 | 1.953 |
| Parameter Default | 0.821 | 0.797 | 2.549 | 2.698 |
| Motion Fade | 9.128 | 8.803 | 15.579 | 13.724 |
| Expression Fade | 8.319 | 9.075 | 14.475 | 14.207 |
| Physics Mobility | 10.256 | 9.893 | 15.545 | 14.795 |
| Timeline Value | 1.312 | 1.184 | 3.266 | 3.043 |

全部 10 个 plain 进程和 1 个阶段归因进程通过语义校验。不同进程存在明显波动：Opacity 的进程 p50 范围，基线 0.352–0.675 ms、当前 0.392–0.551 ms；汇总 p50 当前高 18.8%，因此补做独立定向复测。其他链同样保留每进程统计，不把分布重叠解释为等效性证明。

原始样本、阶段归因、逐进程分位数、两个版本的二进制、来源归档、差异及模型哈希位于 `target/u6-performance/`。这些结果不能替代用户的 UI 人工验收。

## 人工 UI 验收关注点

- 新建后显露对象、展开祖先、聚焦名称；过滤、折叠和隐藏 Properties 状态下仍可发现，Esc 保留对象。
- Motion/Expression 的单击、重复点击、双击和触发语义；改名、排序、移组保持注册身份与播放位置。
- 多选拖放、Alt+Up/Down、锁定和保持形态审阅；无效落点说明及默认取消。
- Physics/Pose 子项的稳定选择、引用修复与结构操作，以及普通绑定配置、关键值、形态编辑和保存/历史往返。
- 纹理新建/导入、替换/修复、依赖删除；重点复核本轮修复的使用者列表及绑定影响列表在窄窗口中的显示。
- IME、合法/非法文本失焦、数值/颜色取消、Save、Undo、文档与工作区切换的统一边界。

### Opacity 定向复核

5 对交替进程，仅执行 `mesh_opacity_drag`；每进程 5 次预热、20 次手势、12 次更新。更新＋CPU 预览 p50 为基线 **0.328 ms**、当前 **0.318 ms**；各进程 p50 范围分别为 0.316–0.488、0.297–0.455 ms，P95 中位数为 1.290、1.188 ms。定向场景未复现全链采样的偏高，不能将一次较高汇总值直接归因为新增 UI 适配。此次仅核查 CPU 探针实际依赖的 editor-core、SDK 与工作区清单，确认它们和已保存二进制的来源一致；UI acceptance 文件不进入该依赖链。

原始数据与逐进程结果：`target/u6-opacity-performance/focused.json`、`focused-*.jsonl.gz`。此前一次短复测因 acceptance 源码变动被来源守卫拒绝，其部分样本已丢弃，不混入任何结果。CPU 复核与工作量回归未显示稳定新增负担；这些观测不提供统计等效性或 UI 延迟保证。

## 收口状态

- 工程清理与修复完成。首轮跨阶段回归 **989 passed、2 ignored**；清理与字段/列表定向门禁 **95 passed**；随后纹理/绑定 **16 passed**、空纹理/IME 边界 **2 passed**。后续过滤集合与首轮有重叠，不累计相加。
- 用户调整前执行的真实模型/Metal 门禁为 **32 passed、3 ignored**，另有 3 个本地模型动画验收；生成 132 张截图，其中对象创作工作流 28 张。最终代码撤去新增 acceptance 场景，历史结果仅作参考。
- 后续 Timeline 和实际窗口 acceptance 已取消，未启动这些 UI 测量。UI 最终验收由用户执行，**U6 不标记为全部验收完成**。
