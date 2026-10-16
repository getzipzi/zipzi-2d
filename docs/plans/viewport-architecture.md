# Viewport 与编辑器基础设施重构方案

日期：2026-10-16。状态：实施中（具体 viewport 与多实例呈现已落地）。

当前已落地：

- `CommandScope::Window` / `Viewport` 命名与独立的 `Timeline` scope；导航命令归入 viewport。
- `CommandState` 统一 enabled、checked 和 disabled reason，建模工具栏、原生菜单与 Timeline 用户命令使用共同状态查询；core 分发前再次检查。
- Timeline 删除、复制、粘贴、逐帧／关键帧导航和首尾跳转纳入统一命令目录、用户覆盖、冲突检查、帮助和重复按键过滤，领域候选操作继续留在 Timeline。
- host 选择可传入的 `KeyboardPolicy`，在 GPUI mapper 前展开 `secondary`；覆盖顺序、平台保留键、文本保护与 Windows 附加 Ctrl+Y 均通过纯逻辑测试。用户配置保留语义键位与可选 `platform`。
- core 测试覆盖三种平台策略及 Timeline 用户命令的历史、播放、只读和文档检查；headless 测试覆盖真实重绑定、旧键移除、文本输入优先与长按写入过滤。
- UI 分别注册 Modeling / Animation 的不透明 `ViewportId` 和固定 `ViewportKind`；core 保存绑定代数，在文档、workspace、可见性、保存和销毁／重建边界拒绝旧目标。UI 现在分别创建固定类型的 `ModelingViewport` 与 `AnimationViewport`；primary 仅用于产品默认槽位，附加实例通过显式目标独立工作。
- `CommandTarget` 区分窗口、文档、viewport 和 Timeline 上下文，`CommandSource` 记录调用来源。建模工具栏和建模上下文菜单保存所属实例目标；原生菜单保存调用快照，菜单缓存同时比较目标，GPUI action 保留与快捷键一致的命令等价关系。
- 建模指针和平移使用包含绑定身份的交互 token；滚轮动画校验绑定代数；旧取消和释放不能终止新 token，取消后首个新按下事件仍正常处理；docking 标签、遮挡与窗口可见性同步到绑定生命周期。尺寸重测只取消建模捕获，持续平移保持可用。Timeline 候选取消统一通过 DocumentId 与候选 edit ID 核验，导航捕获不取消 Timeline 写入候选。
- `EditingLifecycle` 提供上下文切换、阻塞 UI、窗口失焦、保存和关闭准备的共同策略入口。保存继续提交 Timeline 候选、取消建模手势；后台保存完成不使另一文档的上下文失效。
- `DocumentSession` 不再保存 workspace；建模工具与成分选择归入 `ModelingContext`，播放、动画资源与 Timeline 会话归入 `AnimationContext`。`EditingContext` 独立于布局选择，领域写入和动画目标检查使用该上下文；Animation 导航不再通过 `active_tool()` 伪造 Hand，也不改写文档的建模工具。
- 文档视图改为 `HashMap<ViewportId, ViewportState>`，每项包含 zoom、pan、首次 Fit 和独立 revision；viewport 注册项分别记录尺寸与 DPI。命令可用性、分发、滚轮动画、触控板、鼠标平移和缩放读数均核验并使用所属实例。同种附加实例具有各自的导航、呈现帧与命中几何；附加 Modeling viewport 可以从自己的已呈现帧开始编辑，另一视图的 hover、resize 和旧取消不能接管该手势。
- `FrozenDocument` 冻结整张实例状态映射；保存成功和失败均恢复原会话状态，保留保存期间新增实例，尺寸变化使返回后的旧帧失效。测试辅助入口显式注册 viewport，不为未注册实例提供隐式导航存储。
- 两种具体 viewport 直接实现 `Render`、`Panel`、`PanelHeader`，分别声明内容、工具、输入、菜单和建模装饰。共享 `ViewportSurface` 与 `ViewportNavigation` 不读取 workspace 选择行为；docking 工厂、标题、布局校验和截图入口使用具体 panel 名称。布局版本更新为 Modeling 5、Animation 6，旧 Canvas 布局恢复默认，不提供迁移。
- Modeling 求值与只读几何按文档共享，`FrameVersion` 携带完整 binding，各实例独立保存请求序号和已呈现几何。`AnimationIdentity` 只记录内容求值身份，导航与尺寸移入 `AnimationViewportIdentity`；一次动画求值结果可以供多个实例分别投影，导航不重新发起 SDK 求值。
- 单个 Metal 执行器的请求改为按 ViewportId 合并并按 owner FIFO 调度；结果 mailbox 同样按实例合并且不阻塞生产者。图片、错误和统计按实例保存，保存冻结图片也按文档／实例保留。绑定失效会释放命中帧、取消该实例排队请求，并拒绝旧结果；隐藏后重新打开会主动申请新帧。
- 窗口可见性与激活生命周期由 root 统一处理一次。按钮和菜单保存明确目标；局部快捷键仍采用 GPUI 焦点，但会解析实际获得焦点的实例，长按过滤与执行使用该实例。原生菜单保存这个上下文，失效的旧目标不转投另一视图。


第一批验证：`cargo test --locked -p editor-core`（255 passed，1 ignored）、`cargo test --locked -p kasane-editor --lib`（238 passed，1 ignored）和格式检查通过。`./scripts/test.sh` 的 Python、core、headless、截图与前置 Metal 检查通过，但本地 Mao 模型的 `timeline_acceptance` 在 Natural 手势阶段报告 `native dense drag did not change presented pixels`；修改前的 HEAD `0f56a72` 在相同依赖和模型上独立复现同一失败，完整测试尚未全绿。

第二批验证：core 264 passed（1 ignored），headless UI 与命令／菜单测试 242 passed（1 ignored）；Timeline overview、curves、候选与取消恢复的原生 Metal 截图通过。完整脚本再次通过 Python、core、headless、三份本地模型 Animation 验收、默认截图和前置 Metal 检查，最终仍停在上述已确认的 Timeline 像素断言失败。

第三批验证：core 270 passed（1 ignored），headless UI 243 passed（1 ignored），格式与 diff 检查通过。新增测试覆盖同种实例的命令目标、缩放边界、尺寸与 revision 隔离、独立文档恢复、保存成功／失败与冻结期间新增实例、编辑上下文独立于布局、另一实例 resize 不取消建模候选，以及主实例替换后拒绝旧帧。Timeline 专项 Metal 截图通过；完整脚本通过 Python、core、headless、三份真实模型 Animation 验收、默认截图与前置 Metal 检查，仍停在已确认的 Mao Timeline 像素断言失败。

第四批验证：core 275 passed（1 ignored），UI 247 passed（启用 test-support 后 249 passed，1 ignored），生产配置 `cargo check --locked -p kasane-editor`、格式与 diff 检查通过。新增 core 回归覆盖共享求值、实例请求与命中几何、慢实例接受中间候选帧、悬停／框选 overlay 的 binding 归属，以及无 UI token 的领域输入在隐藏／销毁时取消；headless 回归覆盖实际焦点目标和实例错误隔离。双 Modeling 与双 Animation 使用生产 worker 的原生 Metal 场景通过，移动左侧时右侧区域均为 0 像素变化，右侧 GPU 请求号不变；连续双实例导航最终均呈现最新投影。Timeline overview、curves、候选与取消恢复专项截图通过。最终完整脚本通过 Python、core、UI、三份真实模型 Animation 验收、默认及双视图截图、前置 Metal 与 overlay 检查，仍停在已独立确认的 Mao Timeline `native dense drag did not change presented pixels` 断言。产品布局未开放复制 viewport；悬停快捷键路由仍不属于本批已验证能力。

后续尚待完成：悬停路由、Timeline panel 实例身份、窗口捕获与参数对话框生命周期的进一步收敛，以及属性面板的显式检视来源。当前局部快捷键仍使用 GPUI 焦点上下文；多实例呈现通过测试专用场景接入生产的 surface、内容执行器和 Metal 执行器，产品仍保留各 workspace 的唯一主视图，不增加复制入口。坐标命名与网格对话框组件归并尚未完成；平台策略测试不代表 Windows / Linux 宿主及原生键盘验收已完成。

将当前随 workspace 改变行为的 `CanvasView` 拆成独立 viewport panel，由 workspace 组合面板。各 viewport 明确拥有自己的内容来源、工具和输入行为，共享绘制与导航基础组件。同时重构 Command、快捷键、编辑会话与生命周期，为后续扩展提供明确边界。

本方案规定现有 Modeling 与 Animation viewport 的命名、状态归属、输入路由、渲染隔离和实施顺序，范围限于现有功能与基础设施重构。项目尚未发布，内部接口与布局格式允许直接调整，无需旧配置迁移。

## 目标与范围

1. Modeling 与 Animation 使用不同的 viewport 类型，不再由同一个视图读取全局 workspace 来选择行为。
2. 视图状态、命令目标和渲染请求按实例隔离，使用测试场景验证多个 viewport 的独立性。
3. 工具声明、命令检查和输入处理使用明确的 viewport 编辑上下文。
4. 普通局部快捷键跟随鼠标悬停区域；文本输入和正在执行的交互有明确优先级。
5. 延续文档统一的 SDK 历史，以及候选预览、一次手势一次提交、取消恢复原状的编辑契约。

本次保留 Modeling 与 Animation 两种 workspace，各自使用对应的 viewport 实例。身份和状态模型支持实例隔离，多实例并行通过测试场景验证；产品中暂不增加新的 workspace、viewport 类型或复制 viewport 的入口。

## 当前代码与限制

| 当前实现 | 对本次拆分的影响 |
| --- | --- |
| [具体 viewport](../../crates/editor/src/viewports/mod.rs) 与共享 surface / navigation 已分离 | 内容与业务输入由固定类型声明，布局只负责组合 |
| [ToolbarRegistry](../../crates/editor/src/viewports/toolbar.rs) 为两种 viewport 分别提供完整工具列表 | 不再按 workspace 或工具字符串过滤 |
| [DocumentSession](../../crates/editor-core/src/session.rs) 已拆分编辑上下文与按 ViewportId 保存的导航状态 | 保留共享 SDK 历史与领域选择 |
| [呈现状态](../../crates/editor-core/src/viewport/presentation.rs) 按实例保存请求与命中帧 | 有序中间帧仍可接受；切换、隐藏与重建核验 binding |
| [ViewportPanels](../../crates/editor/src/docking/viewports.rs) 区分具体 panel 与内部 surface 的 entity 身份 | 注册工厂和标题使用具体 panel，测试可同时创建同种实例 |
| [预览渲染队列](../../crates/editor/src/preview.rs) 按 viewport 合并请求与结果 | 共享 Metal 执行器，实例之间不覆盖排队结果 |
| [快捷键](../../crates/editor/src/keybindings.rs) 使用 `KasaneViewport` 焦点上下文及实际 focus handle | 悬停路由仍待接入 |
| [Panel catalog](../../crates/editor-core/src/panels.rs) 与 [布局校验](../../crates/editor/src/docking/layout.rs) 声明各 workspace 的必需具体 viewport | 产品布局保持唯一主视图，旧版本恢复默认 |


现有 [PreviewViewport](../../crates/editor/src/ui/preview_viewport.rs) 是网格生成对话框使用的图片与线段预览组件，使用归一化内容坐标。抽取公共组件时评估其导航和绘制代码的复用，保留现有对话框行为。

## 统一命名

| 术语 | 固定含义 |
| --- | --- |
| `Editor` | 整个编辑器或完整编辑模块，沿用 `Editor`、`EditorView` |
| `Workspace` | 面板可用范围、组合、默认布局及布局恢复 |
| `Viewport` | 展示内容并提供局部交互的具体视图 |
| `Panel` | docking 中的面板角色，viewport 和属性列表都可以实现该角色 |
| `Canvas` | 内容画布及其尺寸、坐标等领域概念 |

具体 Rust 类型和 `PanelKind` 使用同一名称：

| 类型和 PanelKind | UI 英文名 | UI 中文名 | 内容及职责 |
| --- | --- | --- | --- |
| `ModelingViewport` | Modeling View | 建模视图 | 模型组合画面、建模选择、变形和 Glue |
| `AnimationViewport` | Animation View | 动画视图 | 动画求值结果、运行反馈和导航 |

这些类型直接实现 GPUI `Render` 和 docking `Panel`，名称不再附加 `View` 或 `Panel`。

共享组件使用 `ViewportSurface`、`ViewportNavigation`、`ViewportState` 和 `ViewportId`。现有对话框 `PreviewViewport` 在归并时明确命名为 `MeshGenerationPreview`，或由对话框组合共享组件实现。仅在实际抽取时完成该重命名。

逐项检查旧 `Canvas…` 名称：视图尺寸使用 `ViewportMetrics`，视图内逻辑像素点使用 `ViewportPoint`，文档画布概念保留 `Canvas`。现有 `CanvasPoint` 有多个坐标用途，应按调用语义拆分，避免统一替换后继续混用屏幕坐标、内容坐标与对象局部坐标。

## Panel 与 workspace 组合

沿用现有 workspace 及辅助面板组合，分别注册明确的主 viewport 类型。

| Workspace | 必需主视图 | 默认辅助视图 |
| --- | --- | --- |
| Modeling | ModelingViewport | 沿用建模属性、参数和对象面板 |
| Animation | AnimationViewport | 沿用 Timeline 与动画相关面板 |

每种 workspace 的主 viewport 保持必需；辅助面板沿用现有关闭与重新打开行为。

Workspace 决定面板是否可添加，具体 panel 的渲染和工具不查询 workspace 来推断自己的类型。首版可用范围按上表限定；以后扩大组合范围，只调整 panel catalog 和布局规则。

布局校验依据 workspace 声明的必需 `PanelKind`、数量和可用范围，不再统计名为 `Canvas` 的节点。布局切换时保存与恢复相应实例状态；不兼容的旧布局恢复为对应默认布局，更新布局版本，不编写旧 Canvas 到新类型的迁移器。

## 组件边界与状态所有权

```text
EditorView / DockWorkspace
  ├─ ModelingViewport
  └─ AnimationViewport
       各自组合 ViewportSurface 与 ViewportNavigation

DocumentSession
  ├─ Document / AuthoringSession
  ├─ Modeling 编辑上下文
  ├─ Animation / Timeline 上下文
  └─ 按 ViewportId 保存的文档视图状态
```

`ViewportSurface` 接受已选定的内容、变换和 overlay，负责裁剪、尺寸测量与呈现。`ViewportNavigation` 负责平移、缩放、Fit 和导航手势。具体 viewport 负责内容来源、命中测试、业务工具和上下文菜单。core 中的坐标变换与导航数学可以共用，GPUI entity、焦点和 Metal 资源留在 UI 层。

| 状态 | 所有者与生命周期 |
| --- | --- |
| 模型、mesh 数据、文档历史 | Document / AuthoringSession；按项目正常保存 |
| 建模试摆、选择与建模工具 | 文档的 Modeling 上下文；切换布局后保留 |
| 动画资源、时刻与 Timeline 编辑 | 文档的 Animation / Timeline 上下文 |
| 缩放、平移和首次 Fit 状态 | 文档下按 ViewportId 保存；切换文档分别恢复 |
| 可见性、实际尺寸、DPI、当前 GPU 图片 | viewport 实例的 UI 状态 |
| 悬停 panel、输入焦点、交互捕获 | 当前窗口的输入路由状态 |
| 布局 | 按 workspace 保存的用户设置 |

`ViewportId` 是由 UI 注册到 core 的不透明运行时标识，core 不依赖 GPUI `EntityId`。具体实例的 `ViewportKind` 注册后固定；切换文档会改变实例绑定的文档和绑定代数。首版视图状态仅在会话中保存，不要求将运行时 ID 持久化到项目。

首版关闭辅助 panel 时保留其逻辑实例和会话视图状态，释放捕获与呈现资源；重新打开时更新绑定代数并重新申请图片。若实际销毁再创建实例，可按该 workspace 的唯一 panel 槽位恢复导航状态，旧异步结果仍按旧实例身份拒绝。

共享文档选择仍服务属性与对象面板，建模成分选择和 Timeline 关键帧选择分别归属于对应编辑上下文。属性面板的检视来源显式绑定活动编辑上下文。所有跨视图引用携带 DocumentId 和稳定对象标识，不能用显示名称或列表位置识别对象。

同一文档同一时刻只有一个未提交的写入手势。开始另一项写入前，按已有提交或取消规则结束前项；只读预览可以持续消费候选快照。导航和选择不创建文档历史。

## 工具与命令

每种 viewport 声明完整工具列表，工具栏直接渲染该列表。注册项使用类型化工具或命令标识，显示逻辑不再识别 `tool_hand` 这类 UI 字符串。

| Viewport | 工具与操作范围 |
| --- | --- |
| ModelingViewport | 现有 Select、Glue、Hand、Generate Mesh 及建模 overlay；拆分阶段保持现有能力 |
| AnimationViewport | 导航；动画和 Timeline 操作通过自己的编辑上下文执行 |

Generate Mesh 是一次命令，不是持续活动工具。Hand 可以表现为导航工具，但 Animation 无需在文档全局伪造一个覆盖其他工具状态的 Hand 值。

局部命令携带 `DocumentId`、`ViewportId` 和对应编辑上下文；按钮直接提供自己的目标。命令可用性同时检查目标实例是否有效、所需能力、选中对象、锁定状态、写入目标以及保存或手势状态。core 在实际执行前再次验证，防止菜单、快捷键或直接调用绕过约束。

窗口和文档命令无需绑定 viewport。现有 `CommandScope::Canvas` 调整为 `Viewport`；`CommandScope::Workspace` 当前表达窗口级范围，统一命名为 `Window`，避免与 Modeling 等工作布局混淆。快捷键展示、覆盖设置、重复按键过滤和实际执行共同使用命令解析结果。

## 输入路由

采用悬停驱动的普通局部快捷键，并分别管理三种状态：

- `hovered_panel` 与区域：当前鼠标下的可交互内容，包含 viewport 内容区、工具栏、输入控件等。
- `keyboard_focus`：GPUI 的真实键盘焦点，用于文本、数值、菜单及可聚焦控件。
- `captured_interaction`：当前操作的发起 panel、viewport、文档、绑定代数和取消入口。

处理优先级如下：

1. 活动对话框和弹出菜单按自身规则消费输入；打开阻塞 UI 时先结束或取消不兼容的捕获。
2. 正在进行的拖动、框选或模态编辑继续接收相关按键、移动和释放事件，直到提交或取消。
3. 文本、数值与其他键盘控件处理自己的输入、导航和撤销。文本编辑期间移动鼠标不转移焦点。
4. 普通 panel / viewport 命令路由到鼠标所在的可见区域，且只在该上下文允许时执行。
5. 未被消费的窗口或文档命令进入相应全局入口；无人处理的事件保持未处理。

悬停变更只更新路由状态，不调用 `window.focus()`。局部快捷键解析应在窗口入口显式结合悬停上下文与真实控件焦点，避免仅靠 `KasaneViewport` 焦点谓词实现悬停，也避免同一个按键被 GPUI action 与手动路由执行两次。保留 GPUI 的输入法、控件键盘行为和可访问性焦点机制。

| 情况 | 预期行为 |
| --- | --- |
| 鼠标位于 ModelingViewport 内容区 | 执行建模视图支持的局部命令 |
| 鼠标位于 AnimationViewport 内容区 | 导航操作作用于动画视图；建模写入命令不可用 |
| 鼠标位于对象列表或 Timeline | 由对应 panel 处理，不回退到上次使用的 viewport |
| 鼠标位于 viewport 工具栏 | 空白及普通工具按钮归属该 viewport；控件自身输入优先 |
| 鼠标位于 docking 标题、标签、分隔条或窗口空白 | 处理该区域的 UI 行为及全局命令，不执行 viewport 编辑快捷键 |
| 鼠标离开窗口但窗口仍激活 | 清除普通悬停目标；窗口命令按系统投递继续处理 |
| 已开始拖动后鼠标离开原 viewport | 捕获继续归属发起者，释放事件返回发起者 |
| 窗口失焦、实例关闭、切换文档或 workspace | 取消未完成候选并释放捕获；清理无效悬停目标 |

Dock 重排、遮挡、标签切换和 popup 打开时重新计算可见命中区域，不能只依赖鼠标移动回调，否则旧悬停目标可能继续生效。布局校验与命令执行也要拒绝隐藏或已解绑的实例。

保留 Space 表示播放／暂停、中键平移以及显式 Hand 的现有约定。播放命令作用于 Animation 上下文，在支持播放的 workspace 中沿用现有可用范围；本次不把 Space 改成临时抓手。撤销键在 macOS 为 `Cmd+Z`，在 Windows / Linux 为 `Ctrl+Z`；输入框编辑时走控件历史，在普通编辑区域走文档历史；已有候选时先按候选取消契约处理。

Blender 的事件系统提供悬停区域、上层 handler 和模态操作上下文的参考，具体实现见 [wm_event_system.cc](https://github.com/blender/blender/blob/main/source/blender/windowmanager/intern/wm_event_system.cc) 与 [Context 文档](https://developer.blender.org/docs/features/core/context/)。以上规则是本项目的输入契约，尤其对窗口外、docking 标题和文本焦点给出了明确行为。

## 多 viewport 渲染

每个 viewport 拥有独立的 metrics、视图 revision、已呈现图片、命中几何与请求序号。文档求值结果、纹理及只读几何可共享；最终投影、渲染目标和屏幕空间 overlay 按实例计算。

渲染请求和结果携带至少以下身份信息：

```text
DocumentId + ViewportId + binding_generation
  + content_revision + view_revision + request_sequence
```

内容 revision 还应包含对应的动画求值身份或建模编辑候选身份。结果返回时核对目标仍然存在、文档绑定一致、内容与视图版本有效；关闭、切换和重建后返回的旧帧不能覆盖其他实例。

现有单一 `latest` 请求槽调整为按 viewport 合并的队列。同一实例可由新请求替换旧请求，不同实例保留各自待处理请求并公平调度。结果通道同样按实例分发或合并，不能让一张未消费图片阻塞全部视图。优先复用现有 Metal 执行器，不默认每个 panel 新建渲染线程。

命中测试只使用与该实例当前呈现图片匹配的几何。没有匹配帧时允许导航，禁止发起几何编辑；已有手势的帧接受规则继续维护候选身份和顺序约束。内容错误、加载状态和渲染失败也按实例保存。

隐藏 panel 停止提交新的呈现请求，释放无用渲染目标；恢复可见后按自身尺寸申请新帧。窗口失焦、切 workspace 与 Timeline 编辑引发的动画暂停规则继续由 Animation 上下文处理，不能由任意一个辅助 viewport 的显隐变化触发全局暂停。

## 配套基础设施重构

本次按状态和行为边界重构。以下优先级表示接入顺序，不要求一次性重写所有模块。

| 优先级 | 重构项 | 当前依据与实施结果 |
| --- | --- | --- |
| 必须先做 | Command 与快捷键上下文 | [Command](../../crates/editor-core/src/commands.rs) 仅携带命令 ID 和文档；增加明确的 panel / viewport 目标、统一状态查询及调用来源 |
| 必须先做 | 编辑会话与布局分离 | [DocumentSession](../../crates/editor-core/src/session.rs) 已移除 workspace；Modeling、Animation 编辑上下文与实例导航状态已分离 |
| 必须先做 | 交互归属与编辑生命周期 | [Modeling 输入](../../crates/editor/src/viewports/modeling.rs)、[Timeline](../../crates/editor/src/panels/timeline/lifecycle.rs) 和对话框分别维护取消规则；统一身份核验、捕获释放与提交取消边界 |
| 与实例隔离一起做 | 内容求值与视图呈现分离 | [AnimationIdentity](../../crates/editor-core/src/animation/provenance.rs) 已去除导航信息；AnimationViewportIdentity 与 Modeling FrameVersion 明确包含实例呈现身份，共享可复用求值结果 |
| 与实例隔离一起做 | 选择与检视来源 | [PanelContext](../../crates/editor-core/src/objects/context.rs) 同时维护对象选择、inspected 与导航；明确各编辑上下文的选择以及属性面板的检视来源 |
| 与实例隔离一起做 | Panel 注册与生命周期 | [DockWorkspace](../../crates/editor/src/docking.rs) 包含大量具体 entity 和类型分支；统一注册元数据、实例查询和可见性回调 |
| 与实例隔离一起做 | 保存期间的会话状态 | [FrozenDocument](../../crates/editor-core/src/workspace/entry.rs) 已冻结按 ViewportId 保存的状态映射；保存成功、失败与后台完成均覆盖恢复验证 |
| 随功能逐步收敛 | 变更通知和根视图职责 | [EditorView](../../crates/editor/src/lib.rs) 手动分发刷新并管理全部帧；提取具体职责，按变更种类通知受影响的实例 |

### Command 与快捷键契约

统一注册用户可调用命令。将 [Timeline 的局部快捷键](../../crates/editor/src/panels/timeline/input.rs) 纳入命令目录、用户重绑定、冲突检查和帮助展示，具体领域实现继续留在 Timeline。`EditorCommand` 和 `TimelineCommand` 保留为带参数的领域操作，不要求每次鼠标移动都经过用户命令分发。

命令状态统一提供 enabled、checked 和 disabled reason，工具栏、菜单与未来命令搜索共享查询。局部可用性根据目标能力与领域条件判断，移除“非 Modeling 即 Animation”的两模式假设。执行仍重新验证对象和版本。

普通快捷键采用悬停目标；工具栏绑定所属 viewport；右键菜单保存打开时的目标；原生菜单及命令搜索保存调用时的编辑上下文，鼠标移入菜单不会抹掉这个目标。执行前验证文档、实例和绑定代数；普通局部快捷键仍不回退到历史目标。

按键匹配、长按过滤和执行使用同一次上下文解析结果。互斥编辑区域可复用按键，重叠绑定按明确优先级诊断；匹配到局部但不可用的编辑命令不得悄悄改投另一视图。区分未匹配、已消费但不可用和执行失败，防止 Delete 等按键落到意外对象上。

### 跨平台快捷键策略

快捷键架构覆盖 macOS、Windows 和 Linux 的平台差异，当前应用运行与原生验收仍以 macOS 为基线。渲染后端和窗口宿主的其他平台移植属于独立工作，不能用键位映射通过测试代表整个应用已经跨平台可用。

当前 [默认命令键位](../../crates/editor-core/src/commands.rs) 和 Timeline 多处直接使用 `cmd-*`；[配置校验](../../crates/editor-core/src/settings.rs) 的系统保留键与文本保护规则也主要面向 macOS。已有 `PlatformKeyboardMapper` 处理键盘映射，GPUI 的 [Keystroke](../../vendor/gpui-fast/src/platform/keystroke.rs) 支持 `secondary`：macOS 映射为 Command，其他平台映射为 Control。`cmd` 本身表示平台键，与 `super`、`win` 共享解析语义，不会自动变成 Control。

将命令语义、平台默认绑定和用户覆盖分开。通用应用快捷键使用 `secondary`，真实 Control、Alt、Shift 以及平台专用动作保留明确含义。

| 命令或行为 | 方案默认规则 |
| --- | --- |
| 新建、打开、保存、关闭文档 | 使用 `secondary-n/o/s/w` |
| 撤销、复制、粘贴、剪切、全选 | 使用 `secondary-z/c/v/x/a`，按控件和编辑上下文路由 |
| 重做 | 通用绑定 `secondary-shift-z`；Windows 可额外提供 `ctrl-y`，按最终上下文检查冲突 |
| 文档切换 | 保留明确的 `ctrl-tab` 与 `ctrl-shift-tab`，不机械替换真实 Control |
| 工具字母键、Escape、Space | 保留命令含义和上下文限制，由平台键盘映射处理实际输入 |
| Hide、Hide Others、macOS 最小化绑定 | 只在支持的平台注册，不参与其他平台的默认绑定与保留键集合 |

选择追加、修饰键拖动等鼠标交互也要检查同样的平台语义。应用级 Command / Control 行为使用 `Modifiers::secondary()` 或统一平台策略；不能直接将 `modifiers.platform` 当成跨平台的 Control。领域确实要求真实 Control 的操作保持显式。

绑定构建顺序为：通用默认值、平台默认差异、用户通用覆盖、用户当前平台覆盖，最后展开修饰键并按当前键盘布局规范化。用户配置保存语义键位与平台条件，规范化后的运行时字符串不反向覆盖配置。空绑定列表表示明确解绑。冲突检查基于最终有效键位与上下文，能识别 macOS 下 `secondary-s` 与 `cmd-s`、Windows / Linux 下 `secondary-s` 与 `ctrl-s` 的等价关系。

将系统动作保留键和控件文本键保护规则移入平台策略，由 host 提供给校验器；core 不硬编码一份 macOS 字符串黑名单。操作系统或桌面环境可能截获部分组合，诊断只覆盖已知规则，不承诺应用可以覆盖所有系统快捷键。

菜单、工具提示、快捷键帮助和命令搜索从同一有效绑定生成平台显示，macOS 使用对应修饰键符号，Windows / Linux 使用对应键名。符号键、非美式布局和布局切换继续经过平台 mapper，避免手动比较展示字符串。重构保持现有键位等价规则，不额外引入物理扫描码绑定模式。

文本输入、IME 组合输入、dead key 和 AltGr 字符输入保留控件处理优先级，不能把字符生成误判为 viewport 命令。按键重复和按下释放配对沿用同一平台事件与命令解析结果。

将平台策略设计为可传入的纯数据，分别测试 macOS、Windows、Linux 默认映射、覆盖优先级、等价冲突和平台专用动作过滤。GPUI 的 `secondary` 解析包含编译目标条件，仅替换 DummyKeyboardMapper 无法模拟另一操作系统；跨平台纯逻辑测试在调用 GPUI 前展开策略。原生键盘布局、系统菜单、输入法与 AltGr 行为在目标平台实际可运行后分别验收。

### 操作归属与写入事务

定义最小的操作协调边界：谁持有指针捕获，谁持有文档写入候选，允许哪些结束方式。导航捕获与文档编辑候选分开记录，某个视图的导航操作不自动拥有或取消另一个编辑上下文的写入。

各操作保存发起者、DocumentId、绑定代数和自己的交互或编辑 token。取消只作用于匹配的 token；延迟关闭回调不能取消已经开始的新操作。切文档、切布局、保存、打开阻塞 UI 和窗口失焦经过共同的生命周期入口，再由具体领域按策略提交或取消。

SDK 继续负责候选与历史。协调层不复制撤销栈，也不把所有领域手势压入一个通用状态机。既有 Timeline candidate 身份和参数对话框版本校验应保留，并接入共同的所有权检查。

### 内容身份与呈现身份

内容身份描述文档版本、求值配方、动画时刻或编辑候选；呈现身份增加 ViewportId、绑定代数、尺寸和导航版本。一个内容结果可以供多个 viewport 投影，某个 viewport 平移只使其呈现失效。

统一失效屏障、实例解绑和结果分发的公共规则，同时保留领域接受策略。Timeline 与建模拖动当前允许同一次手势内有序的中间候选帧；不能为了统一而把所有结果都改成“必须严格等于最新请求”，否则连续输入可能长期无法显示任何完成帧。

渲染错误和进度随目标实例传递；文档加载保存错误保留文档或操作身份。`EditorView` 的单个全局 render error 逐步改为实例状态，防止一个实例的错误覆盖其他实例的状态。

### 选择与检视契约

区分文档对象选择、领域成分选择、主对象和检视目标。Modeling 的顶点与变形器控制点、Timeline 的关键帧分别由对应上下文维护；属性面板跟随明确的活动编辑上下文。选择来源变化不应通过多份全局字段互相覆盖。

删除、拓扑修改、Undo / Redo 与文档重载后，以对象及成分稳定身份统一触发 reconcile，再由各领域清理失效选择。共享选择服务不负责替领域解释 VertexId 或关键帧身份。

### 保存与面板生命周期

当前保存将整个 DocumentSession 移入任务，UI 从 FrozenDocument 读取部分数据。第一步保留这个保存所有权模型，但将冻结视图、图片和恢复逻辑改为按实例处理，测试保存期间切换 workspace、关闭辅助 panel 和完成回调。新增 viewport 不能继续扩展固定长度数组。

编辑器会话状态与 SDK 文档所有权进一步分离后，可以让 panel 状态留在 UI 会话中，保存只接管所需的数据。是否允许保存期间继续导航或写入是独立产品决策；本次不以引入快照保存或并发写入作为前提。

Panel 注册统一声明名称、可用 workspace、默认位置、是否必需和实例策略；实例接口提供绑定文档、可见性变化、停用和关闭的生命周期通知。具体 panel 仍使用强类型 entity，跨类型的布局操作通过注册 handle 完成，不引入动态插件加载。

### 变更通知与根视图职责

逐步将手写的 `redraw`、`redraw_canvas` 和指定 panel 通知归并为明确变更结果，例如文档内容、选择、检视目标、某 viewport 导航和布局变化。根据依赖只更新相关内容和面板：悬停 overlay 不触发模型求值；单个 viewport 平移只刷新自身图像；文档候选变化更新依赖该候选的可见视图和面板。

从 EditorView 提取呈现调度、输入路由和 panel 生命周期协调三个具体职责。GPUI 继续承担响应式通知，不另外引入全局事件总线或通用响应式框架。工作量回归测试验证实际求值、渲染与刷新次数。

## 实施步骤

### 前置阶段 命令与会话边界

确定命令目标、ViewportId、编辑上下文与操作 token 的边界，将 Timeline 用户命令纳入统一目录。先让现有单视图调用经过显式目标和共同生命周期入口，再分离实例状态。明确菜单调用上下文，补齐命令状态、平台键位策略和冲突诊断。

冻结状态和保存恢复、对象选择及检视来源在此阶段确定接口；具体多实例存储和 UI 替换与后续阶段一起完成。

### 第一阶段 拆分现有视图

新增 `viewports/` 模块，提取 surface 与 navigation，将现有行为分别放入 ModelingViewport 和 AnimationViewport。工具列表分开声明，保留现有建模与动画能力。

同步调整 panel catalog、docking 注册、标题工具栏、布局版本、截图定位与测试辅助入口。完成命名语义检查，去掉核心交互对 workspace 索引的依赖。按 viewport 注册文档视图状态，保持切 workspace 与切文档后的工具、视图和选择恢复行为。

### 第二阶段 支持实例隔离与输入路由

替换 `active_canvas()` 的隐式目标，将第一阶段的 ViewportId 贯穿显式命令目标、绑定代数和窗口输入路由。将 metrics、帧、候选命中与请求合并分配到实例，建立两个可见 viewport 互不覆盖的验证场景。

工具栏、菜单、快捷键和直接调用使用同一可用性检查；窗口入口统一解析按键和重复规则。完成捕获、弹窗、文本输入及生命周期清理，再进入集成验证。

### 第三阶段 生命周期与集成验证

完成保存冻结与恢复、panel 可见性变化、选择 reconcile、实例错误状态和变更通知的整合。收敛 EditorView 的呈现调度、输入路由及 panel 生命周期职责。

使用现有内容来源构造测试专用的双 viewport 场景，验证实例隔离与调度；产品布局保持 Modeling 和 Animation 两种 workspace。通过以下验收后完成本次重构。

## 验收标准

| 范围 | 必须覆盖的行为 |
| --- | --- |
| 现有功能 | Modeling 的选择、Glue、变形器和顶点手势，以及 Animation 的播放、Timeline 编辑保持可用 |
| 工具边界 | Animation 无法通过按钮、快捷键或直接命令入口执行建模写入 |
| 命令一致性 | Timeline 命令可重绑定并参与冲突诊断；按钮、菜单和快捷键查询相同状态；菜单目标不因移入菜单栏而丢失 |
| 平台键位 | 分别验证 macOS、Windows、Linux 策略的修饰键映射、用户覆盖、等价冲突和系统动作过滤；原生验收状态与纯逻辑测试分开记录 |
| 键盘输入 | macOS 原生验证菜单显示、布局切换、符号键和 IME；Windows / Linux 可运行后补充原生验证与 AltGr 测试，不以模拟策略测试替代 |
| 实例隔离 | 测试专用的两个 viewport 同时可见，各自平移、缩放、Fit、尺寸变化互不影响 |
| 事件路由 | 鼠标在 viewport、对象列表和 Timeline 间移动后，无需点击即可切换普通局部命令上下文；离开所有内容区域不使用旧目标 |
| 控件焦点 | 文本输入、数值输入、菜单和输入法不被悬停 viewport 抢占；一个按键至多执行一次业务命令 |
| 操作捕获 | 拖动跨越另一个 panel 后仍由发起者接收释放；失焦、关闭、切文档和切 workspace 正确取消 |
| 坐标正确性 | 不同画布尺寸、原点、DPI 和缩放下，现有建模操作的显示、命中与写入一致 |
| 选择与检视 | 重名对象、多选主对象、删除、Undo / Redo 和文档切换后，属性面板检视对象及领域成分选择正确 |
| 历史 | 一次手势产生一次历史记录；取消后依赖候选的视图恢复；选择和导航不改变文档历史 |
| 生命周期 | 辅助 panel 关闭和延迟回调不取消其他 owner 或更新 token 的操作；保存冻结与完成恢复保留各实例状态 |
| 异步结果 | 交错请求、慢帧、关闭重建、候选取消和切文档时，无跨实例图片、几何或错误串用 |
| 布局 | Modeling 与 Animation 分别恢复对应主 viewport 和辅助面板；旧布局按版本恢复默认 |
| 性能 | 未变化和隐藏的 viewport 不重复求值或渲染；一个连续刷新视图不饿死另一个视图 |

以 core 测试验证坐标、命令能力、候选历史和版本接受规则；以 headless UI 测试验证悬停、焦点、捕获、docking 与双视图独立性；以原生 Metal 截图或验收验证图片和 overlay 的配对。新增测试围绕上述可观察行为，避免仅断言类型名或注册项数量。

按 [README](../../README.md) 中的命令运行相关测试，并在集成阶段运行 `./scripts/test.sh`。性能验证优先复用现有 UI 工作量回归与原生拖动基线；发现回归后再增加针对性测量。
