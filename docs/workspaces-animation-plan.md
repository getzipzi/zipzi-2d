# Modeling 与 Animation Workspace 实施计划

日期：2026-10-12。状态：待实施。

将现有编辑界面确立为 Modeling workspace，新增用于已有动画资源预览与验证的 Animation workspace。首版完成 model3 模型包导入、motion 播放、expression 应用与组合预览，并保证播放不会污染建模状态和文档历史。本计划不包含动画创作功能。

## 已确定的产品约束

1. 保留当前布局作为 Modeling 的默认布局。允许面板调整，两个 workspace 分别保存布局；首版不提供自定义 workspace 创建或管理。
2. 快捷键的键位和命令含义不随 workspace 改变。Space 统一表示播放／暂停；移除 Space 临时抓手，中键拖动统一平移画布。保留触控板导航和显式 Hand 工具，供没有中键的设备使用。
3. Animation 默认展示运行时效果，不继承 Modeling 的编辑器隐藏标记。模型本身的可见性、透明度、Part、Pose、Offscreen 和遮罩关系仍参与求值与合成。
4. 两个 workspace 共享文档、资源和编辑历史；建模参数试摆、动画播放状态和视图状态隔离。
5. 项目尚未发布，允许直接调整内部边界和配置格式，不做旧 layout 配置迁移。

下文给出首版的具体实施规则。其中命令可用范围、切换后的暂停行为和表情清除方式是本计划采用的产品默认值，实施时按同一套规则完成 UI、核心状态和测试。

## 首版范围

### 导入与资源

以现有 `.model3.json` 导入流程为入口，加载其引用的 moc3、纹理、motion、expression，以及已有的 physics / pose。单独的 moc3 不包含动作和表情资源，首版不增加裸 moc3 的资源关联向导，也不扫描目录猜测未注册的资源。

导入后保持用户当前 workspace，不自动开始播放。首次启动默认为 Modeling；窗口可以恢复上次使用的 workspace。保存、重新打开 Kasane 项目后，同样可以预览项目已有的动作和表情。

Motions 按 model3 的 group / entry 展示。不能仅按 clip UUID 去重，因为同一个 clip 的不同注册项可能有不同的淡入淡出配置。项目中存在但未注册到 group 的 clip 放在“未分组”中，使用 clip 自身配置。Expressions 使用资源名称并保留稳定的内部标识。

资源为空、引用文件缺失、动作目标未解析和求值失败分别提供明确状态。没有 motion 但有 expression 的模型允许单独预览表情；两者均为空时仍允许观察和导航模型。

### 播放与检查

- 同时选择一个 motion 和一个 expression。运行时允许表情切换所需的短暂淡入淡出重叠，不提供多表情叠加编辑。
- 提供播放、暂停、回到开头、单个 motion 的时间定位，以及清除表情和重置预览。
- 遵循 motion 自身的循环标志、曲线和淡入淡出定义；首版不增加循环覆盖开关、播放队列或动作间混合编辑。
- 同一预览运行 Motion → Expression → Physics → Pose，显示最终模型效果。
- 提供当前资源信息和运行诊断。可选的参数观察只读，不复用可修改建模参数的滑条行为。

暂不包括：关键帧与曲线编辑、多轨道编排、多模型场景、自动眨眼／呼吸等额外驱动、口型音频驱动、Sound 播放、视频导出、动画资源单独导入及 workspace 插件系统。motion 中的事件仅作诊断展示，不执行外部副作用。含 Sound 的资源明确提示当前不播放音频。

## 界面与统一输入

在全局文档标签栏旁增加 Modeling / Animation 切换入口；文档标签在两个 workspace 中保持同一层级。切换 workspace 不创建或复制文档。

| 区域 | Modeling | Animation |
| --- | --- | --- |
| 左侧 | 保留当前部件、绘制顺序、变形树布局 | Motions 与 Expressions 两个可同时观察的区域 |
| 中央 | 建模画布和建模工具 | 运行时预览画布，保留导航工具 |
| 右侧 | 保留当前属性与参数布局 | 当前资源信息、诊断；按需打开只读参数观察 |
| 底部 | 保留现有安排 | 播放控制、时间和单 motion 进度尺 |

Animation 首版不显示网格编辑、变形器操作柄或关键形写入工具，不增加空的轨道编辑器。Motions / Expressions 使用专用播放面板；现有通用 Document 面板仍可用于 Modeling 中的资源检查，不把整个通用对象浏览器改造成播放器。

命令注册只有一份快捷键表，workspace 切换不卸载、替换或重新解释按键。命令的可用性由当前上下文决定：首版只在 Animation 中启用播放；Modeling 中 Space 对应的仍是播放命令，但处于不可用状态，不恢复抓手语义，也不自动切换 workspace。进入 Animation 后沿用同一个绑定。未来若允许 Modeling 内播放，只放宽命令可用条件。

Space 不抢占文本输入、模态对话框或已有控件的必要键盘行为，长按不重复切换。播放按钮、菜单和快捷键必须调用同一个命令入口并使用同一可用性判断。中键平移在两种 workspace 中完全一致；释放按钮、窗口失焦、切文档后均清理拖动捕获。

## 状态所有权与持久化

| 状态 | 所有者 | 保存策略 |
| --- | --- | --- |
| 当前 workspace | 窗口的 Editor 状态 | 用户设置，默认 Modeling |
| 每种 workspace 的 docking 布局 | UI 布局状态 | 按 workspace 分开保存 |
| 模型、动画资源、编辑历史 | 现有 Document / AuthoringSession | 原项目保存流程 |
| Modeling 的选择、参数试摆、活动工具 | 每个 DocumentSession 的建模状态 | 沿用现有策略；播放不写入 |
| 每种 workspace 的缩放和平移 | 每个 DocumentSession | 首版在打开的会话中保留 |
| motion 选择、expression 选择、时刻、运行状态 | 每个 DocumentSession 的 Animation 状态 | 仅内存，不写入项目和 SDK 历史 |
| SDK MotionPreview、seek 缓存、几何缓存 | 动画预览执行器 | 可丢弃；关闭文档时释放 |

引入最小的 `WorkspaceKind::{Modeling, Animation}`。现有 `editor-core/src/workspace.rs` 管理窗口内的文档和任务，不等同于新的工作模式；保留职责或按需改名，避免让 workspace 同时指布局、文档容器和播放状态。现有 `CommandScope::Workspace` 表示窗口级命令范围，同样不能被误用成 Modeling / Animation 模式。

布局配置建议使用带 workspace 键的新配置文件，写入任务必须捕获具体 workspace，避免延迟写入覆盖另一个布局。Reset Layout 只重置当前 workspace。缺失或无效配置只恢复对应默认值；旧 `layout.json` 不导入、不编写迁移器。

## 播放交互契约

### Motion 与时钟

| 操作或事件 | 首版行为 |
| --- | --- |
| 选择 motion | 暂停，切换资源，时间归零并求值；保留当前 expression 选择，在零时刻重新激活 |
| Play | 从当前位置继续；首次从零开始；非循环动作已结束时从零重播 |
| Pause | 冻结整个预览时钟，包括 expression 过渡和物理状态 |
| 回到开头 | 暂停并定位到零；保留资源选择和本轮表情触发记录 |
| 非循环 motion 播放结束 | 停在 duration 对应的求值帧并暂停，不自动恢复建模姿态 |
| 循环 motion | SDK 时钟持续向前，物理不在每圈重置；界面同时标识圈数和当前圈内时间 |
| 拖动进度尺 | 开始时暂停，合并请求，定位到所选时刻；释放后保持暂停 |
| 无 motion，仅有 expression | 使用持续预览时钟驱动过渡和物理；进度尺禁用，仍可播放、暂停和归零 |
| 重置预览 | 暂停，清除 motion、expression 和临时触发记录，重建零时刻运行时基线；不清除 Modeling 状态 |

单 motion 进度尺使用该 clip 的 duration。循环播放时，拖动只定位第一圈 `[0, duration]`，并重建该轮预览；所选 expression 在零时刻重新激活，界面回到第一圈。首版不提供跨多圈的时间编辑。暂停／继续不触发这种重建。

零长度、非有限时长或无法定位的资源不得引起除零或无限 seek；资源信息和诊断给出原因，禁用不适用的进度尺或播放操作。

### Expression

Expression 是持续生效的参数效果，不人为赋予一个 motion 式的结束时长。首次选择或切换表情时，在当前预览时刻安排激活；播放中继续运行，暂停时保留暂停，由用户播放以观察过渡。界面分别显示“已选择”和运行时实际活跃的表情，避免淡入尚未发生时误认为选择无效。

同一轮预览在内存中记录表情激活时间和顺序，用 SDK 的 schedule / seek 语义重放。非循环 motion 中向前、向后 seek 应重现这些切换；时间定位到旧位置后再触发表情，将该时刻之后的旧表情触发记录丢弃，形成新的预览分支。点击已经选中的表情不重复安排激活。

“清除表情”首版采用立即移除的确定语义：清空本轮所有表情触发记录，重建不含表情的相同 motion 预览，定位到当前时刻，恢复原播放／暂停状态。因此物理也重新求值，画面可能立即变化；不承诺清除时的淡出。清除后再次 seek 不会把旧表情恢复回来。

切换表情遵守资源的淡入淡出和 Add / Multiply / Overwrite 混合。清除需要重建，因为 SDK 当前 `reset()` 保留 schedule，不能将其当成清除 API。连续点击和清除造成的重建请求使用最新目标覆盖，不积压旧操作。

### 文档、workspace 与编辑生命周期

- 离开 Animation、切换文档、窗口失去激活、打开模态对话框或开始保存时暂停；回来不自动续播。保留选择和时刻，清除未完成的指针捕获。
- 首次进入某文档的 Animation，从参数默认值建立预览，不继承 Modeling 的参数试摆。首次视图适配模型，之后保留各自的缩放和平移。
- 切 workspace 前取消未完成的画布拖动候选；已经完成的编辑保留。文本编辑使用现有提交／取消规则完成，不把半完成事务交给动画快照。
- Animation 首版不开放模型写入工具。全局 Undo / Redo 仍表示共享文档历史；执行时暂停并使动画快照失效，不能撤销一次播放或表情选择。
- 模型内容改变或 Undo / Redo 后重新创建 SDK 预览，保留仍有效的资源选择；时长缩短则钳制定位，资源消失则清除对应选择并说明原因。变化后保持暂停。
- 关闭文档后取消求值与渲染请求并释放该文档预览。异步返回不能改变另一个文档的画面、错误或播放状态。

## 实现边界与代码入口

### 核心播放控制

新增 `editor-core/src/animation.rs`，承载资源投影、纯数据的预览描述、播放状态转换和 SDK 适配。将每个文档的控制状态挂在 `DocumentSession`，通过 Editor 暴露带 DocumentId 校验的命令。拟定命令包括选择 motion、选择／清除 expression、播放／暂停、归零、seek 和重置预览。

通过 [Document](../crates/editor-core/src/document.rs) 暴露创建 detached `MotionPreview` 所需的受控接口，不把可写 AuthoringSession 暴露给 UI。SDK 预览持有独立快照；播放求值不调用 `SetPreviewParameter(s)`，不提交 EditSession，不累积历史、不改变 modified。

model3 注册项使用 `schedule_motion_entry(group, index, time)`；未分组 clip 使用 `schedule_motion`。保存内部选择时，同时保留注册位置与 clip 标识用于校验，文档变化后不能仅凭旧 index 播放另一个条目。

快照失效使用能覆盖动画资源、模型内容和资源变化的单调 generation。不能只依赖几何 evaluation revision，也不能在 Undo 后复用旧 generation。选择、导航和时间推进不改变模型 generation。首版允许已提交的纯编辑器元数据修改导致保守重建，但不能把这些元数据应用到运行时可见性。

### 求值与帧交付

新增编辑器侧动画执行器，例如 `crates/editor/src/animation.rs`：后台独占一个活动的 `MotionPreview`，接收核心生成的播放描述和目标时间，返回不可变的动画帧与状态快照。UI 不执行长时间 seek；非活动文档保留描述和时间，可释放求值实例。

控制命令按序处理，时钟和拖动产生的目标时间可以合并。不要合并掉尚未生效的选择／清除操作。长 seek 使用 SDK `seek_with_progress` 检查取消和最新请求；新 seek、切换 workspace、切文档、关闭和快照失效均使旧任务失效。

播放以单调时钟计算目标时间，按与 SDK canonical seek 相同的 60 Hz 步进规则推进；不能直接把不稳定的 UI 帧间隔当成物理步长。显示帧可合并，必要的模拟步不能任意跳过。暂停或窗口失焦后重设墙钟锚点，避免恢复时追赶失焦期间的时间。SDK 可重用的步进适配若不足，先补小范围 API 和一致性测试，不复制一份动画算法到编辑器。

每个异步结果至少携带 DocumentId、workspace、模型 generation、预览 generation、请求序号和实际求值时间。渲染结果沿用同一身份；拒绝其他文档、旧 workspace、旧预览或时间倒退的过期帧。连续播放允许同一 generation 内晚到但比已显示帧更新的结果，不能要求每一帧都等于最新请求而造成持续拒帧。暂停、seek、重置时提高接受下界，阻止之前的播放帧覆盖目标帧。

当前 [DocumentSession::scene](../crates/editor-core/src/session.rs) 同时构造画面与建模 picking 几何。Animation 只需运行时画面，需拆出共享渲染描述和可选的建模交互数据，避免每帧重建对象树、缩略图、拾取几何和建模覆盖层。保留现有 Metal 渲染器及 [preview worker](../crates/editor/src/preview.rs) 的请求合并和 GPU 背压。

播放面板读取动画 snapshot；建模面板仍读取文档投影。画面及需要精确对应的参数读数使用实际求值／呈现时间，不能拿新的时钟值标记旧画面。错误和 coverage 诊断去重后展示，不逐帧追加通知。

### 运行时画面与透明度

Animation 的 [SceneSnapshot](../crates/editor-core/src/scene.rs) 使用 SDK 运行时帧，不经过 Modeling 的 `viewport_frame` 隐藏过滤。不修改或清除文档中的 editor hidden / locked 元数据，切回 Modeling 后保持原来的编辑可见性。

将 `MotionSnapshot::model_opacity` 纳入共享渲染描述，Modeling 使用默认值 1。阶段二先固定其兼容语义：相邻 SDK observer 当前在 `apply_model_opacity` 开启时将该值乘入各 drawable 的 opacity；最终模型合成后再整体乘透明度是另一种效果，两者在重叠和混合时不能互换。使用有重叠、遮罩和 Offscreen 的 fixture 对照现有 observer 与项目采用的运行时参考，确定一致的应用位置后再接入，不能仅根据字段名称选择合成方案。

默认复用经验证的 SDK 运行时语义，确保该因子只应用一次。若对照发现 observer 或底层 renderer 存在缺口，在相邻 crate 补齐并更新依赖说明；不要让编辑器单独形成一套未验证的透明度规则。

复用 SDK 已求值的 Part / Pose 透明度，避免重复相乘。编辑器隐藏的遮罩源和 Offscreen 在 Animation 中恢复运行时行为，但 runtime 本身隐藏或禁用的对象仍按模型规则处理。

### UI 与配置

- [docking.rs](../crates/editor/src/docking.rs)：两种默认布局、workspace 对应的面板校验和独立布局读写；共享一个中央画布实体，避免重复注册 Panel identity。
- [ui.rs](../crates/editor/src/ui.rs)、[canvas.rs](../crates/editor/src/canvas.rs)：移除 Space 临时抓手，保留现有中键平移；按 workspace 提供画布内容和工具入口。
- [commands.rs](../crates/editor-core/src/commands.rs)、[keybindings.rs](../crates/editor/src/keybindings.rs)、[menus.rs](../crates/editor/src/menus.rs)：统一命令注册和可用性，提供播放入口与状态；不制作两套快捷键配置。
- `crates/editor/src/workspace/`：切换控件和播放控制条；`panels/` 新增 motion、expression 和预览信息面板。
- `crates/editor/locales/`：中英文名称、空态、禁用说明、资源及播放错误文案。

## 分阶段实施与退出条件

各阶段按顺序推进，每阶段完成后可独立验证；不以通用 workspace 框架重构作为前置项目。

### 阶段一：Workspace 边界与输入整理

- [ ] 添加 WorkspaceKind、切换入口和每 workspace 的视图／布局状态。
- [ ] 保留现有 Modeling 默认布局，提供 Animation 最小画布和播放区域。
- [ ] 移除 Space 临时抓手，播放命令使用统一绑定；中键、Hand 工具和触控板导航继续可用。
- [ ] 定义模式切换、手势取消、命令可用性和独立布局保存。

退出条件：往返切换不修改文档、不丢失建模试摆和视图；独立布局可恢复；旧模式事件无法继续操作新模式；Space 在任何 workspace 都不进入抓手。

### 阶段二：单 motion 播放闭环

- [ ] 建立 AnimationSession 与 SDK detached preview 适配，按 group / entry 选择动作。
- [ ] 打通后台求值、时钟推进、运行时帧、整体透明度和现有 Metal 输出。
- [ ] 实现播放、暂停、归零、自然结束及资源自身循环。
- [ ] 完成文档／workspace 身份与 generation 检查，以及保存、失焦和关闭时的暂停与清理。

退出条件：真实模型包中的一个 motion 连续播放正确；播放中可导航；切回 Modeling 恢复原状态；历史和 modified 不变；旧异步帧不覆盖新画面。此阶段必须验证整体透明度，不能仅验证参数发生变化。

### 阶段三：Expression 组合与确定性定位

- [ ] 实现 expression 单独预览、切换、临时触发记录和立即清除。
- [ ] 实现 motion / expression / physics / pose 的同一预览链路。
- [ ] 实现进度尺、后台 seek、合并与取消，以及循环定位规则。
- [ ] 对齐播放步进与 seek 结果；资源或模型变化后重建预览。

退出条件：同一预览描述在同一时刻的结果可重复；向前／向后定位正确重放表情和物理；清除后不复活旧表情；快速拖动不阻塞 UI、不呈现过期结果。

### 阶段四：资源面板与完整反馈

- [ ] 完成 motion 分组列表、expression 列表、资源信息及必要的只读状态展示。
- [ ] 完成空资源、缺失文件、未解析目标、不支持通道、Sound 未播放等诊断。
- [ ] 把现有对象投影中“playback tools not implemented”等硬编码文案改为准确的资源说明；运行状态来自 AnimationSession。
- [ ] 完成菜单、工具提示、本地化、布局重置和重启恢复。

退出条件：用户能从导入一路完成选择、组合、暂停、定位、清除和回到建模，不需要通过日志判断播放是否生效。

### 阶段五：回归与首版验收

- [ ] 完成下述核心、headless 和 Metal 验证。
- [ ] 在实际模型上验证循环、非循环、表情、Physics / Pose、透明度、遮罩及 Offscreen。
- [ ] 记录求值和渲染耗时、请求积压及长 seek 响应，修复持续拒帧、无界队列和持续增长的缓存。
- [ ] 更新 README 的功能入口与 DEPENDENCIES 中实际新增的 SDK / renderer 要求，并记录实施结果。

退出条件：通过完整用户场景；声明不支持的能力有明确反馈，不将部分兼容呈现为完整兼容。目标为代表性模型在记录的测试机器上争取稳定 60 Hz 播放；未达标时依据 CPU / GPU 数据决定优化，不能通过破坏物理步进来掩盖性能问题。

## 验证计划

### editor-core

新增 `workspaces`、`animation` 测试模块，使用小型确定性 fixture 覆盖：

- workspace 切换、每文档状态和两套视图隔离；播放、表情和 seek 不改变 modified、模型内容与历史。
- group 注册淡入淡出优先级、单 motion 结束／循环、表情混合、切换和清除、无 motion 的表情预览。
- 模型默认值与 Modeling 试摆隔离；editor hidden 不参与 Animation，runtime visibility 仍生效。
- 同一目标时间的连续固定步进与冷 seek／缓存 seek 对照，包含物理、Pose 和表情切换。
- Undo / Redo、资源删除或重排、时长变化、保存中和关闭后的状态协调。
- 异步结果身份校验、连续播放不饿死呈现、seek 后拒绝旧播放帧；失败操作不破坏最后有效状态。

### GPUI headless

新增 workspace 和 animation 交互测试，复用现有 setup：

- 同一 Space 绑定、长按抑制、文本输入和模态保护；Modeling 中 Space 不平移、不自动切模式。
- 两种 workspace 的中键／触控板导航与手势清理；保持现有 Hand 工具可用。
- 列表选择与实际播放分离、播放按钮／菜单／快捷键一致、进度尺暂停规则。
- 切 workspace／文档和异步加载、暂停保存、关闭任务的交叉操作。
- 两种默认布局的截图、面板重开、独立配置和小窗口下播放控件可达性。

### Metal 与实际模型

新增针对重叠 drawable、遮罩和 Offscreen 的 model opacity 图像验证；opacity 为 0、0.5、1 时与阶段二固定的运行时参考比较，覆盖会区分逐 drawable 应用与最终合成后应用的场景。验证 editor hidden 与 runtime hidden 的区别，保持 Modeling 渲染回归。

完整手动场景：导入模型包 → 在 Modeling 调整试摆并临时隐藏一个对象 → 切 Animation → 播放 motion → 切换 expression → 暂停并前后定位 → 清除 expression → 切换另一文档 → 返回 → 切回 Modeling。以进入 Animation 前的历史和 modified 状态为基线，确认运行时画面正确、返回后建模姿态和隐藏状态保留、动画操作没有新增历史或改变 modified，后台不会继续播放非活动文档。

执行仓库要求的检查：

```sh
cargo test --locked --workspace
cargo fmt --all -- --check
```

若修改相邻 SDK 或 renderer，额外运行对应 crate 测试和格式检查。纯 CPU 动画语义优先由确定性 fixture 验证；实际模型截图与性能数据用于补充，不替代状态和数值测试。

## 已核实的基础与实施风险

- 当前导入链路为 `LoadRequest::Model3` → `Document::import_model3`；已有 motion / expression 对象投影，但播放工具尚未接入。
- `AuthoringSession::motion_preview()` 已创建独立预览。`MotionPreview` 已提供 motion entry 调度、expression 调度、组合求值、seek 和 seek cache；不需要另起动画运行时。
- `MotionPreview::reset()` 保留 schedule；清除表情须使用上述重建方案，不能仅调用 reset。
- `MotionPreview::evaluate_drawables()` 已包含运行时 Part 透明度，但整体 `model_opacity` 独立存在，必须在渲染出口处理。
- 当前 canvas 已支持中键平移；此次输入改动重点是删除 Space 抓手分支并统一播放命令。
- 当前 scene 缓存与建模 picking 几何耦合，且现有帧接受规则围绕建模交互；动画需要独立失效和呈现规则，不能仅靠每帧全窗口 redraw 接入。

相邻实现依据：`../kasane-2d/modules/kasane-sdk/src/session.rs`、`modules/kasane-animation/src/motion_preview.rs`、`expression.rs` 及 `tests/motion_preview.rs`。以上为已有 API 能力；本计划中的 UI、状态边界和接入行为仍待实施。
