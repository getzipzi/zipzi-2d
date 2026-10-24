# Compact UI 调研与重构建议

日期：2026-10-15。状态：Compact 运行时改造已完成；首轮与完整实现记录见第 8、9 节。

目标：以 Blender 的密集编辑器界面为参考，让 Zipzi 在更小的空间里保持清楚的操作入口、属性结构和工作区边界。默认采用 Compact 密度。

## 1. 调研范围与现状

已阅读 README、现有 Dock 设计文档、编辑器主题、Dock skin、工作区工具栏、属性面板、参数行、Timeline、数字输入，以及锁定版本 GPUI Kit `507e82b` 的相关实现。

视觉参考使用已有截图 `target/dock-ui-final/dock-workspace.png` 和 `target/dock-ui-timeline/timeline-overview.png`。这些是历史产物，本次没有重新运行截图测试，不能作为当前 HEAD 的视觉验收结果。

| 区域 | 当前实现 | 设计判断 |
| --- | --- | --- |
| 主题 | `theme.rs` 主要定制蓝色交互色，保留 Kit 中性色与尺寸 | 需要编辑器自己的表面、边界、密度规则 |
| Dock | 自定义 `EditorDockSkin`，单行 header 为 28；外框、分割容器与 resize 大多委托 Kit | 可以延续现有结构，首先在 skin 层调整 |
| 工具栏 | 大量 `.ghost()`；Small、XSmall 和固定像素混用 | 静止状态下的按钮边界不够明确，控件高度不一致 |
| 搜索 | header 中使用 Small Input | 在当前默认 rem 下约 24 高、14 字号，与 XSmall 按钮不统一 |
| 字体 | Kit 基准为 16，很多正文显式使用 `text_xs()`，另有 `text_sm()` | 现状不能简单概括为“所有文字都是 16”；需要按角色统一 |
| 树与参数 | 常见行高 26；参数行有左右 12 padding、名称/轨道/数值分栏 | 可以收紧行高和横向留白，但需保留参数轨道的操作空间 |
| 属性 | 属性名与值上下堆叠，每项带底部分隔线 | 单行双列是比单纯缩字更大的密度收益 |
| Timeline | 轨道行高 26、标尺高 28；头部有较多独立工具 | 应按播放、编辑、视图分组并明确窄宽度优先级 |
| 工作区 | 模式栏 38、状态栏 28 | 有进一步收紧的空间；原生窗口控制区域单独处理 |

源码入口：

- [主题](../crates/editor/src/theme.rs)
- [Dock skin](../crates/editor/src/docking/skin.rs)、[Header](../crates/editor/src/docking/header.rs)
- [PanelToolbar](../crates/editor/src/panels/toolbar.rs)
- [工作区模式栏](../crates/editor/src/workspace/top_bar.rs)、[画布工具栏](../crates/editor/src/canvas/toolbar.rs)
- [属性面板](../crates/editor/src/panels/properties.rs)、[参数行](../crates/editor/src/panels/parameters/row.rs)
- [数字输入](../crates/editor/src/ui/slide_number_input.rs)、[Timeline](../crates/editor/src/panels/timeline/mod.rs)

## 2. Blender 中值得借鉴的设计

Blender 将编辑器划分为主区域、header、工具栏、侧栏等区域；header 承载该编辑器的菜单和常用操作。Zipzi 现有单行 Dock header 已经符合这个方向，可以继续强化。[官方 Regions 文档](https://docs.blender.org/manual/id/4.5/interface/window_system/regions.html)

Blender 的数字字段兼具直接输入、拖动调整等操作，文本字段有明确的圆角边界。Zipzi 的 `SlideNumberInput` 已实现点击编辑、横拖、Shift 精调和 Escape 恢复，优先统一外观与反馈即可。[官方 Input Fields 文档](https://docs.blender.org/manual/en/latest/interface/controls/buttons/fields.html)

Blender 提供 UI 缩放、线宽和区域边界宽度设置。紧凑视觉与易命中的操作区域可以同时设计。[官方 Interface Preferences](https://docs.blender.org/manual/en/4.5/editors/preferences/interface.html)

以下尺寸、配色和按钮方案是面向 Zipzi 的建议值，并非 Blender 的精确测量值。Blender 的区域分割交互不在第一轮照搬范围内，现有 Dock 分组、拖放和最大化已能支持目标布局。

## 3. 建议的 Compact 尺寸基线

单位为 GPUI 逻辑像素；Retina 截图的物理像素不能直接当成这些尺寸。

| Token / 用途 | 初始建议 |
| --- | --- |
| 正文、树节点、输入文字 | 12 |
| 辅助信息、标尺、状态栏文字 | 11 |
| 分组标题 | 12，中等字重 |
| 普通 icon | 14；复杂工具图标允许 16 |
| Icon button / 小按钮 / input 高度 | 22 |
| 特别密集的 header 控件 | 20，优先复用已经为 20 的 XSmall 按钮 |
| 树、参数、Timeline 普通行高 | 22；关键帧密集场景可采用 24 |
| Dock header | 26 |
| 控件圆角 | 3 |
| Dock panel 圆角 | 5 |
| Panel border | 1 |
| Panel 间隙 / 工作区外侧留白 | 4 / 4 |
| 同组控件间距 / 组间距 | 2 / 6 |
| 表单内容 padding | 6 |
| 工作区模式栏 / 状态栏 | 30 / 22 |

多数现有 `text_xs()` 已约为 12，因此首轮主要把较大的输入、菜单和正文统一下来。可以用 11.5 做后续对照样本，但不建议未经中文实机阅读验证就把全局默认压到 11。

26 高的列表改为 22 后，相同内容高度理论可显示约 18% 更多行；这是纯行高计算，不是整窗有效面积的实测提升。新增 panel gap 会消耗少量面积，应一起测量。

预留独立 UI Scale，后续提供 100%、110%、125%；缩放须涵盖字体、行高和控件尺寸，不能仅放大文字。第一轮不必增加多个密度模式。

## 4. 视觉与交互规则

### 面板

- 工作区底层使用深灰，panel 使用稍亮的灰，header 再略亮一级。通过表面层级区分区域，避免大块近黑面板与灰色画布的强烈割裂。
- 每个可见 Dock group 有一圈完整边框和小圆角；header 与内容共用外框。
- Panel 间保留 4 的暗色间隙，嵌套 split 的交叉处也应等宽。
- 间隙同时承载 resize 操作。目标命中宽度可为 6–8，但不能侵占相邻输入、滚动条或画布手势；需要实测。
- 激活面板只轻微提高边界或 header 识别度；键盘焦点、拖入预览另设明确状态。
- 圆角只定义区域轮廓；画布、树与时间轴内容不加无意义的整圈内部 padding。

初始暗色试样：工作区 `#1F1F1F`、面板 `#292929`、header `#303030`、控件 `#3A3A3A`、边界 `#484848`、正文 `#D8D8D8`。这些仅为候选，需验证文字、输入边界、disabled 与 selected 的实际对比。

### 按钮

- 独立 icon 按钮默认有低对比底色、细边框和 3 圆角，静止状态就能辨认点击区域。
- 相邻且同类的工具使用连体分组，共用外边框，内部使用细分隔线；减少每个图标一个方框带来的噪声。
- 明确区分默认、hover、pressed、selected、disabled 和 keyboard focus。Selected 用蓝色底色或边界持续表示，不能与 hover 相同。
- 动作按钮与状态切换按钮保持可辨别反馈；播放、循环、吸附等 toggle 应保留持久状态。
- 图标优先沿用现有一致的 icon 系统；先改善按钮载体与分组，后续再为网格、变形器、参数等领域概念补充专属 SVG。
- 所有 icon 按钮保留可访问名称与 tooltip，有快捷键时显示快捷键。树中的展开箭头属于结构控件，使用行内状态；可见性、锁定等动作保持明确点击区域。

### 属性与数字输入

- 简短属性改为“左标签、右字段”的单行布局，标签列对齐；多行文本、复杂属性和极窄 panel 可退回上下布局。
- 数值右对齐，单位固定位置，优先采用等宽数字特性；避免数值变化导致左右跳动。
- 数字字段默认有凹入感底色与边界；hover 提示可拖动，编辑态显示明确焦点。
- 保留现有点击输入、拖动、Shift 精调、Escape 恢复语义，不能因为换皮破坏输入状态或撤销。
- 参数轨道含关键点编辑语义，不能无条件被通用数字字段替换；优先压缩左右空白，保留轨道和数值。
- 按对象属性类型组织折叠分组，减少每一项横线。只读值与可编辑字段需要不同呈现。

### 信息组织

- 模式切换栏压到约 30，使用紧凑分段控件；文档标签关闭和窗口拖动仍需足够命中范围。
- 保留现有面板选择器；宽 panel 可考虑显示短名称，窄 panel 使用图标与 tooltip，最终由原型对照决定。
- Header 先保留关键操作，再压缩文字和搜索，最后将低频操作移入 overflow。当前已有窄 header popover，继续利用。
- 搜索折叠后必须显示过滤生效状态，并保留输入内容，避免用户误以为对象丢失。
- Timeline 分成播放、编辑、视图三组；播放和当前时间优先可见，时间范围、视图设置等低频项进入 overflow。
- 树的 hover 使用轻微灰色，selection 使用克制的蓝色；沿用现有对象类型色和未保存的琥珀色语义。
- 状态栏后续可补充当前工具的操作提示；减少常驻说明大段占用面板内容的情况。

## 5. 实现方式与需先验证的点

### 统一主题和基础组件

建议新增编辑器尺寸 token 与共享样式入口，例如 `ui/metrics.rs`、`ui/button.rs`、`ui/field.rs`、`ui/property_row.rs`。这些名称仅为建议，功能目标是让密度和状态规则有唯一来源。

保留 Kit/Base 已有的焦点、菜单、禁用、可访问性和输入行为，封装编辑器样式。普通控件优先封装 Kit；Dock selector 当前使用 Base 以支持拖动，需要单独套用相同视觉规则。

**不能只把 `Theme.font_size` 从 16 改到 12。** 锁定版本 Kit 的 Root 会调用 `window.set_rem_size(theme.font_size)`；XSmall 按钮的 `size_5()`、Small input 的 `h_6()`、`text_xs()` 和 `gap_2()` 都可能随 rem 缩放，而 `px(26.)` 等行高保持不变。结果可能是原本 12 的文字缩到 9，按钮进一步缩小而容器不变。

首轮可保留 Kit 的 rem 基准，在编辑器组件里显式设置文字角色、逻辑像素高度和 padding；同步检查弹窗与菜单。若要改变根基准，必须一次性整理 rem 与固定 px 的混用，并验证 Kit 内部样式覆盖顺序。

### Dock 外观与几何

现有 `EditorDockSkin`、`EditorTabGroupSkin` 是主要落点，优先验证 frame、content_frame、split_frame 和 resize hook 能否实现期望间隙。

目前 Dock 的部分尺寸、裁剪与布局由 Base 拥有，不能假设在 split 上简单加 `gap(4)` 就能与已分配的 pane 尺寸一致。应先做两栏、嵌套 split 和底部 dock 的小原型，核对尺寸总和、最小宽度、拖放区域和 resize 命中位置。只有 hook 不足时才考虑对 Kit 做最小扩展；不要修改 Cargo checkout 作为交付方案。

特别检查：

- Bottom dock 当前依赖 29 的折叠范围，header 改高、加边框和外侧留白后仍需完整显示。
- 圆角裁剪能否覆盖真实 Metal canvas，是否出现角落漏底或错误遮罩。
- Panel 拖放预览、菜单和浮层是否被外框裁剪。
- 不破坏 Parameters 已有行级缓存边界，以及 header 与 panel 的独立观察关系。

## 6. 推荐执行顺序

1. **真实组件试样**：在现有 component playground 中放入中英文标签、按钮所有状态、搜索、数字输入、属性行和两个带间隙面板；比较 12 字号和候选密度。
2. **统一基础规则**：落地 tokens、按钮、输入与属性行样式，确定暗色和浅色状态表。
3. **工作区与 Dock**：面板框、间隙、header、模式栏、状态栏和 resize，先完成建模工作区。
4. **高密度内容**：树、参数、属性，再处理 Timeline；确保绘制、虚拟列表和命中测试共享行高。
5. **补齐与验收**：弹窗、菜单、窄面板、中文长标签、浅色模式和缩放；更新相关文档与基线截图。

第一轮最适合评审的切片是完整建模工作区：左侧层级树、中间画布工具栏、右侧属性与参数。它能同时验证 panel 风格、按钮辨识度、输入密度和实际画布收益。

## 7. 验收建议

- 同一窗口尺寸和缩放下，记录可见树/参数行数、画布净面积、header 溢出数量；截图必须注明逻辑尺寸与 scale factor。
- 检查普通与窄窗口、中英文、暗色和浅色，以及后续 UI Scale 档位。
- 字体基线、输入光标、中文输入法、数字截断、tooltip、selected/disabled/focus 都清楚且无裁剪。
- 验证 Dock 拖放、合并、折叠、最大化、布局恢复、边界 resize 和圆角下的 Metal 内容。
- 数字拖动取消、列表滚动、Timeline 关键帧命中保持正确；行高变更需同步绘制和交互坐标。
- 复用现有 Dock、PanelToolbar、数字输入与 Timeline headless 覆盖；截图走 README 中的真实 Metal 流程。只有交互和性能敏感区域发生变化时再运行对应专项基准。

以上为实现前的调研记录，首轮落地范围如下。


## 8. 首轮实现（2026-10-15）

本轮先落地基础规则与建模工作区，未改变 Kit 的 16px rem 基准。

- 新增 `ui/compact.rs`，由编辑器与 component playground 共用尺寸、深浅主题、按钮、输入和属性行样式。正文 12、辅助文字 11、控件 22、header 按钮 20、控件圆角 3。
- Playground 默认打开 Compact 试样：中英文、普通/选中/禁用按钮、连体工具组、搜索、真实数字拖动输入、宽窄属性行，以及两个带 4px 间隙的面板。hover、pressed 和焦点使用真实交互检查。
- Dock group 增加完整边框与 5px 圆角，header 改为 26；模式栏与状态栏改为 30 / 22。Header 与画布工具按钮增加静止状态下的边界，选中工具保留蓝色状态与 toggle 语义。
- 属性面板使用对齐双列；空间不足时值换到下一行。长标签单行省略，tooltip 显示完整名称；100px 窄布局有自动验证。
- 树、参数和共享折叠行改为 22，同步调整参数轨道容器和树拖动自动滚动计算。参数行减少留白与逐项分隔线，数字字段右对齐并使用等宽数字字体；点击输入、横拖、Shift 精调、Escape 恢复沿用原实现。
- Kit 的 sized toolbar child 会强制使用 ghost 样式，因此动作按钮通过 toolbar group 的 content 接口保留编辑器外观；方向键导航与输入焦点仍由原 toolbar 行为处理。

验证入口：

```sh
cargo check --locked -p zipzi-editor -p component-playground
cargo fmt --all -- --check
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked --workspace
env -u ZIPZI_DEV_SESSION_DIR ZIPZI_SCREENSHOT_SCENE=dock cargo test --locked -p zipzi-editor --test headless_screenshot
```

验证结果：全工作区测试通过；最后的按钮选中状态修正后，185 项 headless 交互测试通过（1 项跳过），Dock Metal 截图重新生成并检查；格式检查通过。

本机截图：修改前 `target/compact-ui-before/`，首轮结果 `target/compact-ui-final/`。工作区截图逻辑尺寸为 1100 × 740、scale factor 为 2，PNG 为 2200 × 1480；参数与弹窗的局部截图尺寸随裁剪区域变化。已检查建模工作区、参数分组、数字拖动、窄 header、中英文及深浅主题；本轮未报告整窗有效面积或性能提升百分比。

后续范围：4px 工作区外侧留白和真实 Dock split 间隙、resize 命中扩展与圆角下 Metal 内容的专项像素验证；按钮分组的进一步推广；Timeline 工具分组与密度；UI Scale 与更多菜单/弹窗的统一。当前 4px 面板间隙只在 playground 原型中提供，未修改依赖 checkout 或 split 尺寸分配。


## 9. 完整实现与验收（2026-10-15）

实现继续使用官方锁定的 GPUI Kit `507e82b`。没有新增或保留 GPUI Kit / Base 的 vendor 副本，没有修改依赖 checkout；`Cargo.toml`、`Cargo.lock` 和原有 `vendor/` 均无变更。

### Dock 与工作区

- 每个叶面板在自身 border box 中预留半个间隙，工作区再预留半个间隙：普通和嵌套 split 均形成 4px 间隙与 4px 外侧留白，split 分配的尺寸总和不变。UI Scale 会同步缩放留白。
- Header 与内容分别绘制完整外框的上、下部分。内容末尾的局部角落遮罩在真实 Metal 内容之后、菜单与拖放浮层之前绘制，没有命中区域，也不增加内容 padding。
- 保留 Kit 的 resize 行为、鼠标命中带和持久化。当前 split 命中宽度沿用 Kit 的 9px，外侧 dock 为 5px；没有为了凑 6–8px 的候选值引入依赖扩展。已有拖放、合并、最大化、折叠、滚动、画布导航和原生 dock resize 验收继续通过。
- Bottom dock 保留 Kit 的 29px 折叠范围；折叠 header 限高，125% 时仍能完整显示控件。
- 最后一个面板拖离 dock 后，复用已有空 dock 清理逻辑释放旧区域，避免侧置 Timeline 下方留下大块空白；加入了回归测试。

### 控件与内容

- 画布、面板动作和 Timeline 使用共享连体工具组。各按钮保留自己的动作/toggle 语义、焦点与回调，不通过工具组回调取代原有键盘行为。
- Timeline 行高为 24、标尺为 24。列表投影、绘制缓存、滚动、框选、关键帧命中和曲线坐标转换共用缩放后的行高；绘图字体与命中半径随 UI Scale 调整。
- Timeline 按播放、编辑、视图分组。窄 header 将次要动作放入 overflow；很窄时保留播放、当前时间和 overflow。循环、预览模式、吸附、平移与 inspector 保留持久选中反馈。
- 属性按字段命名空间组织折叠组，短标签与只读值双列对齐，窄面板自动换行。层级树移除名称旁多余的弹性空白，长名称获得完整可用宽度。
- Header 折叠时，过滤生效状态仍通过蓝色工具按钮表示；搜索内容继续保存在原 panel 的 InputState 中。
- 参数管理、数字输入、共享表单 footer 与网格生成操作使用 Compact 控件。菜单保留 Kit 的焦点、快捷键、禁用和关闭行为；浅色截图改为走编辑器主题入口，确保验收的是实际配色。

### UI Scale

右上角工作区菜单提供 100%、110%、125%，保存到 `appearance.json`。保留 100% 时 16px 的 rem 基准；更高缩放同时调整 rem、明确的控件尺寸和参与绘制/命中的像素计算，不单独放大文字。Playground 提供同样三档试样切换。

### 验收产物

- 完整工作区测试、编辑器 218 项测试（188 项交互 + 30 项其他测试）、真实模型动画与 Timeline 验收通过；新增缩放、属性组折叠和空 dock 清理覆盖。截图验收关闭过渡动画，避免把工作区切换或缩放的中间帧当成基线。
- `target/compact-ui-complete/` 保存建模、Timeline、窄布局、表单、中英文和深浅主题截图。
- `ZIPZI_SCREENSHOT_SCENE=compact` 对三档缩放分别生成深色英文、浅色中文截图，并把模型放大到完全覆盖画布，对不透明 Metal 的底部圆角作像素断言。
- `compact-metrics.json` 记录逻辑窗口、display scale、UI scale、画布净面积、树行容量、header bounds 和 overflow 数量。测试窗口为 1100 × 740，display scale 为 2。

| UI Scale | 画布逻辑尺寸 | 画布净面积 | 树可容纳完整行数 | Header overflow |
| --- | --- | --- | --- | --- |
| 100% | 470.5 × 619 | 291239.5 | 12 | 0 |
| 110% | 470.5 × 611.5 | 287710.75 | 10 | 0 |
| 125% | 469 × 597.5 | 280227.5 | 9 | 0 |

容量来自实际列表可用高度与对应行高，fixture 本身只有两行，不能当作实际显示了 12 个对象。更高缩放会合理减少可见内容；新增区域间隙也有面积成本，本轮不宣称整窗画布面积提升。

发布版专项基准使用 600 次滚动采样，覆盖真实 Motion（134 tracks / 779 keys）和密集数据（134 tracks / 34438 keys）、拖动候选、播放、快速 seek、画布平移与 dock resize。结果保存在 `target/compact-ui-benchmark-final/results.json`，包含取消像素恢复、一次拖动对应一次撤销、SDK 结果与几何一致性、保存重开等检查。该基准不包含系统输入送达和 compositor 呈现；没有同条件修改前基线，不报告性能提升百分比。


## 10. 细节微调与最终收敛（2026-10-15）

针对首轮落地中暴露的控件散落、长标签截断、英文文案溢出及高缩放折行等细节问题，完成了系统性收敛与对齐。

### 细节优化项

1. **属性行标签与高缩放折行修复**：
   - `compact::property_row`: 标签预留宽度从 `rem(112.)` 扩大至 `rem(132.)`，使长标签（如 `missing_attachments`、`canvas.pixels_per_unit` 等 19 字符内标签）在标准面板宽度下无需截断省略，直观可见；
   - 相应调整右侧值的 `flex_basis` 从 `rem(110.)` 至 `rem(70.)`，避免在 125% UI Scale 档位下因内宽不足导致双列布局意外换行为两行，恢复标准 22px（高缩放 27.5px）单行高度；在 180px / 100px 极窄场景下仍保持正确的响应式折行。
2. **属性可读性提升**：
   - 属性分组折叠标题增强为中等字重 (`FontWeight::MEDIUM`) 并使用正文强调色；
   - 对纯数值属性应用等宽字体 (`.AppleSystemUIFontMonospaced`) 并统一右对齐，提升密集属性列表的结构感与对比度。
3. **英文本地化按钮截断微调**：
   - `crates/editor/locales/en.toml`: 将 `timeline.inverse` 从 `"Inverse stepped └"` 缩短为 `"Inv. Stepped └"`，消除 94rem 宽度插值按钮在英文环境下的末尾省略号截断（`Inverse st...`），与 `"Stepped ┐"` 保持对称。
4. **遗留工具栏与弹窗尺寸规范化**：
   - `deformer_bar.rs`: 将原散装 `.small().ghost()` 按钮改造为连体 `compact::group`，统一度量为 22px 高度、3px 圆角与静止边界；
   - `hierarchy_tree/visibility.rs` / `locking.rs`: 触发按钮与菜单尺寸从固定像素统一迁移至 Compact Token (`rem(compact::HEADER_CONTROL)`、`rem(compact::CONTROL)`、`rem(compact::DOCK_HEADER)`)；
   - `mesh_generation.rs` / `parameters/form.rs`: 移除覆盖 `compact::button` 的多余 `.small()` 与 `.ghost()` 修饰；
   - `timeline/inspector.rs`: 统一关闭、列表跳转、资产循环与插值单选按钮为 `compact::header_button` / `compact::button` / `compact::toggle`；
   - `timeline/controls.rs`: 编辑与视图两组工具间距设为 `compact::rem(compact::GAP * 3.)` (6px)，边界划分更加清晰。
5. **Playground 覆盖补全**：
   - `component-playground` 直接集成并演练 `compact::group` 控件，覆盖普通、选中与禁用态。

### 验证结果

- 编译检查：`cargo check --locked -p zipzi-editor -p component-playground` 零警告。
- 代码格式：`cargo fmt --all -- --check` 完全通过。
- 自动化测试：`env -u ZIPZI_DEV_SESSION_DIR cargo test --locked --workspace` 全工作区通过（218 项 editor 单元/集成测试、全部模型动画验收及真实 Metal 测试均绿）。
- 截图与度量验证：`env -u ZIPZI_DEV_SESSION_DIR ZIPZI_SCREENSHOT_SCENE=compact cargo test --locked -p zipzi-editor --test headless_screenshot` 生成 100%、110%、125% 深色英文与浅色中文截图，不透明 Metal 底层圆角断言与 `compact-metrics.json` 保持一致。



## 11. Panel 与 Canvas 组件调整（2026-10-15）

本节覆盖前文关于 panel 外框、select 菜单与缩放栏位置的设计。

- Panel 移除独立 more 按钮；tab 右键菜单承接面板动作、添加/拆分、最大化/还原和关闭。单 tab 也可右键操作和拖拽。
- 只有含多个可见 tab 的组显示 select，下拉内容仅用于切换已有 tab，不提供添加面板。单 tab 使用普通图标 tab。
- 按最终确认，Panel 保留完整边框与圆角，header 与内容使用同一背景；只有 topbar/statusbar 移除外框线并使用工作区背景。面板间隙通过 padding 实现，保留 Metal 圆角遮罩和原有 dock resize 行为。
- Header 左右内边距与居中控件的上下留白一致：100% 时为 2px（含外框为 3px），由 header 与控件高度推导并随 UI Scale 调整；topbar/statusbar 同步保持内容边界对齐。
- Canvas 自己持有工具配置并渲染工具栏；工具栏、变形上下文工具和缩放工具从 workspace 模块移入 canvas。缩放栏使用真正的 Toolbar，位于 canvas header 右侧并垂直居中，statusbar 不再包含缩放控件。
- Motions / Expressions 的资源行统一为 22px，随 UI Scale 缩放；建模模式的对应文档行同步紧凑显示。
- 继续使用原有 GPUI Kit 依赖，不添加 vendor 副本，不修改 Cargo 依赖文件。

本轮验收：`cargo test --locked --workspace` 与 `cargo fmt --all -- --check` 通过；编辑器 220 项测试通过（另 1 项忽略），三档 UI Scale × 深浅主题的 Metal 截图及圆角检查通过，Timeline 截图、候选/取消像素恢复与窄布局通过。最新截图和度量保存到 `target/compact-ui-revisions/`，`chrome-pixel-checks.json` 核对 panel 边框恢复、topbar/statusbar 无边框及 header 无溢出。


## 12. Header 一致性与右键菜单修正（2026-10-15）

- Topbar/statusbar 移除各自散落的水平留白，共用 `compact::chrome_inset`，与 panel header 的内容左右边界对齐。该值包含工作区留白、panel 外框和 header 内边距，并随 UI Scale 调整。
- Header 的 select、搜索输入、动作按钮、清除按钮、dock 折叠/最大化按钮统一使用 20px 高度。PanelToolbar 移除额外内边距及 sized child 包装，与 select 共用垂直中心；Timeline transport 同步对齐。
- 多 tab 的 select 恢复静止边框，菜单仍仅用于选择已有 tab。
- 面板菜单移到整个 header 的右键入口；tab 与空白内边距均可打开添加/拆分/最大化/关闭菜单，输入框自身的文字编辑菜单保留。
- Timeline 上方额外的线来自底部 dock resize handle 的静止 hairline。仅在该外侧像素绘制背景遮罩，保留 panel 外框、header 分隔线、原有 resize 命中带/回调及延迟绘制的 hover 提示。
- 更新实际交互验收：三档缩放下对照 select/input/button 高度与中心、topbar/statusbar/panel 的内容边界，以及从 header 空白处添加面板；Timeline Metal 截图增加额外细线的像素断言。

验收完成：`cargo test --locked --workspace`、`cargo fmt --all -- --check` 与 `git diff --check` 通过；编辑器 220 项测试通过（另 1 项忽略）。最新建模/Timeline 截图保存到 `target/compact-header-fixes/`，三档缩放 × 深浅主题均无 header 溢出，select 边框像素检查与 Timeline 额外细线像素断言通过。原生 dock resize、拖放、搜索输入和菜单回调验收保留。


## 13. 一体式缩放控件（2026-10-15）

Canvas header 的 zoombar 改为独立 `ZoomControl` 组合组件：缩小、百分比、放大、适配共用一圈外框、圆角和背景，操作区连续排列，适配区使用内部细分隔线。整组保持 20px 高度、右对齐及垂直居中，统一动作回调与可用状态，保留 Toolbar 的键盘导航、tooltip 和各操作区焦点提示。触发时重新核对弹窗、modal 与命令可用性，避免陈旧的 header 状态触发操作。截图保存到 `target/compact-zoom-group/`。


Topbar/statusbar 朝向 Dock 一侧不再叠加 padding，实际可见间距由 Dock 原有的 4px 外侧留白提供。左右内容继续与 panel header 对齐。验收直接比较控件、panel 外框与状态文字区域的实际 bounds，覆盖三档 UI Scale。


## 14. Header 右侧留白、整行拖拽与数字输入框（2026-10-15）

- Header 没有面板动作时不渲染空 toolbar，消除其产生的额外 gap。树搜索框、Canvas 缩放控件的右侧留白与上下留白一致；三档 UI Scale 均检查实际 bounds。
- 拖拽入口扩展到整行 header，沿用原生 Dock 的拖放回调、拖拽预览和布局规则。Tab 仍可拖拽，搜索、按钮与菜单保留各自交互。回归用例覆盖普通面板和 Canvas 从 header 空白处拖到另一组。
- 共用 SlideNumberInput 改为 18px 高、14px 文字行高，随 UI Scale 缩放。在 22px 参数行内上下各留 2px，保留直接输入、相对拖动、Shift 精调、限值与撤销行为。
- 截图 fixture 加入实际参数行，便于同时查看 header 和数字输入框；三档缩放 × 深浅主题截图保存在 `target/compact-header-drag-number/`。

本轮验证：`cargo test --locked --workspace` 通过（编辑器 221 项通过、1 项忽略），包括真实模型/Metal 渲染、Timeline 操作与原生 Dock resize 验收；`cargo fmt --all -- --check` 和 `git diff --check` 通过。Timeline 深浅主题及窄布局截图、候选取消像素恢复检查通过。

## 15. 工作区改动审查与回归修复（2026-10-15）

- **Header 拖动误触**：拖选搜索文字或从按钮开始移动鼠标会启动 Dock 拖动。将鼠标按下隔离放在已组合好的控件外层，保留 popover 接收按下事件、菜单打开、键盘导航和 header 空白处拖动。补充搜索、过滤按钮及缩放按钮的回归检查。
- **缩放后的参数行重叠**：行内容使用 rem，行缓存却固定为 22px，导致虚拟列表步进与内容高度不一致。缓存现在使用同一缩放后的行高；新增三档缩放和列表首尾滚动检查。
- **参数标记偏移**：关键点和滑块命中区的大小随 UI Scale 增长，负 margin 仍为固定 px。居中偏移改为与尺寸相同的单位，并验证端点和当前值的中心坐标。
- **生成网格弹窗溢出**：预览先按窗口尺寸适配、再乘缩放，会超过可用空间；弹窗宽度还遗漏了两侧边框。预览改为在归一化尺寸下适配，表单宽度接口明确返回已缩放的 Pixels。回归覆盖三档缩放及 800×480、900×620、1100×740 窗口。
- **长属性值截断**：统一只读属性值组件，限制在值列内，以省略号显示长值并提供完整 tooltip。Playground 复用此组件，窄面板布局测试使用长标识符。

验证结果：`cargo test --locked --workspace` 通过，编辑器 226 项通过、1 项忽略；真实模型动画、Timeline 输入/取消/播放与原生 Dock resize 验收通过。Playground 编译检查、格式与 diff 检查通过。三档缩放 × 深浅主题的 Compact Metal 截图和圆角像素断言通过，结果位于 `target/compact-review-after/`。本轮未提交代码。
