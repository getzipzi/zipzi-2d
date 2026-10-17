# GPUI 本地补丁清单

## 对比基线

保留 crates.io 发布的 `gpui-fast`、`gpui-fast-macos`、`gpui-fast-apple` **0.1.7**，作为 GPUI Kit 0.7.3 的 `gpui-fast` 后端。根目录 `Cargo.toml` 的 `[patch.crates-io]` 选择这些本地包。Kit 固定在官方 commit，另通过 Git source 的 `[patch]` 覆盖下述 `gpui-component` 包。

## 拖动身份与定向取消

[drag-ownership.patch](drag-ownership.patch) 为 `gpui-fast` 增加 `ActiveDragToken`、`App::active_drag_token` 和 `App::stop_drag`。身份在内部拖动或平台文件拖入创建时生成，跨平台交接保留；弱引用快照不延长拖动资源寿命。取消验证同一次拖动，过期 token 不会停止后来开始的操作。原有无条件 `stop_active_drag` 保留。

Timeline 数值滑块直接持有此 token，替代编辑器全局 DragOwner。回归见 [input_routing.rs](../crates/editor/headless_tests/input_routing.rs) 的真实 GPUI 拖动替换与重复取消测试。`AnyDrag` 新增 crate 内部身份字段；直接构造该结构体的外部使用方需要适配。升级 GPUI 时在已有本地补丁之后重新核对并应用本补丁。

## Kit compact 菜单与 Tooltip

`vendor/gpui-kit/crates/component` 来自 Kit commit `507e82b8115ac73af92b7d3856501f9d7b73b2c9`，保留完整包、上游清单和 Apache-2.0 许可。精简的 `vendor/gpui-kit/Cargo.toml` 提供原清单继承的依赖与 lint；同仓库的 base、assets、macros 仍指向相同 Git commit，未整体 vendor Kit。额外保留 README 和上游测试通过相对路径引用的五份文件，它们不作为其他本地包编译。

工作区清单相对上述固定 commit 的完整差异见 [kit-workspace.patch](kit-workspace.patch)，涵盖成员裁剪、依赖继承与同版本 Git 引用。该补丁以原始 Kit 根目录 `Cargo.toml` 为基线，目标路径为 `vendor/gpui-kit/Cargo.toml`。

Kit 的菜单修改位于 `menu/popup_menu.rs`，完整差异见 [compact-menu.patch](compact-menu.patch)：

- 实现 `Sizable`，公开已有 Small 密度。
- Small 普通行保留 20px 基准，改为随 window rem 缩放；其余尺寸保持上游行为。
- Small 菜单项字体改为 `text_xs()`，在 100% UI scale 下为 12px。
- 子菜单在渲染时继承父菜单尺寸，同时覆盖 `submenu()` 和 `PopupMenuItem::submenu()` 两条构造路径，包括最后才设置父菜单尺寸的情况。

菜单水平 padding 继续使用 8px，圆角使用原 Small 的半径减半规则；自定义内容保留最小行高，项目标题与路径不会被固定为单行高度。编辑器通过 `ui::compact::menu` 和 `ui::compact::select` 统一入口；Select 使用原生 XSmall，将触发框、选项与搜索框字体一并缩小为 12px，触发框高度仍为 22px。XSmall 的选项垂直 padding 沿用 Kit 的 4px。字体跟随现有 UI scale。未重写交互逻辑或修改兄弟工作目录。

Tooltip 的修改位于 `tooltip.rs` 与 `root.rs`，完整差异见 [compact-tooltip.patch](compact-tooltip.patch)：文字与快捷键统一为 11px，行高 14px，水平/垂直 padding 为 6px/1px，文字与快捷键间距为 6px。尺寸均使用 rem 跟随 UI scale；共享浮层与 GPUI 原生 tooltip 两条路径复用此内容组件。共享浮层按窗口记录当前触发元素的完整 GPUI 身份；旧按钮的离开事件不会关闭新按钮的提示或取消其显示计时。身份在 prepaint 阶段取得并跨重绘匹配，避免依赖 hover 回调顺序。

Tooltip 回归见 [tooltips.rs](../crates/editor/headless_tests/tooltips.rs)，覆盖双向反复切换、首次显示前切换、停留超过隐藏宽限期、离开后的正常隐藏、点击关闭与取消待显示提示。外层 tooltip 通过 Kit 的标准 `test_support()` 接口供测试观察，正式构建中该观察接口为空操作。

回归见 [compact_popups.rs](../crates/editor/headless_tests/compact_popups.rs)，覆盖明暗主题、三档 UI 缩放、两种子菜单构造方式、多行内容，以及真实编辑器对话框中的普通/可搜索 Select。右键菜单动作继续由现有 Dock、Canvas 和参数管理测试覆盖。真实 Metal 截图使用 `KASANE_SCREENSHOT_SCENE=compact-popups`。

更新 Kit 时，将该包、README、五份测试引用文件及根工作区清单替换为新固定 commit 的对应内容，再适配并应用 `kit-workspace.patch`、`compact-menu.patch` 和 `compact-tooltip.patch`。更新精简工作区中的 Git revisions，重新核对补丁是否仍需要，以新基线重新生成三份补丁，并更新根目录依赖和 lockfile。

```sh
git apply --reverse --check vendor/kit-workspace.patch
git apply --reverse --check vendor/compact-menu.patch
git apply --reverse --check vendor/compact-tooltip.patch
cargo test --locked -p kasane-editor --lib headless_tests::compact_popups
cargo test --locked -p kasane-editor --lib headless_tests::tooltips
```

原生 Metal 集成涉及 **11 个文件（10 个源码文件 + 1 个依赖清单），新增 495 行、删除 13 行**，差异见 [native-metal.patch](native-metal.patch)。此外，Fast 缓存边界修复涉及 `fast/retained.rs` 和 `fast/splice.rs` 两个文件，独立保存为 [cache-boundary.patch](cache-boundary.patch)；虚拟列表高度估计修复涉及 `elements/list.rs`，保存为 [list-height-estimates.patch](list-height-estimates.patch)。合计修改/新增 14 个文件，其余保持发布版本原样。

`Cargo.toml.orig` 保留发布包附带的原始工作区清单，并非本地实际构建清单；构建使用标准化的 `Cargo.toml`。Apache-2.0 许可文件保留。

## Fast 虚拟列表缓存边界

`RetainedSubtree` 独立保存显式 `.cached()` 身份，创建缓存记录时标记，在跨帧复用和 splice 复制时保留；`is_cached()` 不再从可选的 `Rebuild` 推断。虚拟列表项没有父布局 key 时依然是缓存边界，其后代未通知的状态写入不会触发无谓的重复构建。通知、全量刷新及原有其他失效条件仍生效。正式代码没有实验环境变量或诊断日志。

回归位于 [view_cache.rs](../crates/editor/headless_tests/view_cache.rs)，覆盖 retention on/off、虚拟列表缓存行及其普通子视图、无通知更新、通知重绘、不变兄弟行复用和全量刷新。测试在修复前失败、修复后通过。性能和窗口延迟见 [正式修复验证](../docs/gpui-fast-cache-boundary-fix.md)。

## 虚拟列表尺寸变化

变宽后，`List` 仍把条目标记为未测量，但保留原高度或调用方提供的高度估计。可见行在新宽度下重新测量；未显示行继续贡献估计高度，避免滚动范围收缩、滚动条变短和无法直接滚动到尾部。回归见 [view_cache.rs](../crates/editor/headless_tests/view_cache.rs) 的 `variable_list_resize_preserves_scroll_estimates_and_remeasures_visible_rows`，真实 Motion 的末行可达性由 Timeline Metal 验收覆盖。

## 原生 Metal 逐文件修改

| 文件 | 修改的符号与行为 | 保留原因 |
| --- | --- | --- |
| [gpui-fast/Cargo.toml](gpui-fast/Cargo.toml) | 仅给 macOS 添加 `metal = "0.33"` 依赖。 | 核心接口需要持有原生 device、queue、texture 类型。 |
| [gpui-fast/src/platform.rs](gpui-fast/src/platform.rs) | 新增 `SharedMetalContext { device, queue }`；为 `PlatformWindow` 增加 `metal_context()`，默认返回 `None`，均限 macOS。 | 让应用取得渲染器实际使用的资源，避免创建不兼容设备或另一个提交队列。 |
| [gpui-fast/src/platform/test/window.rs](gpui-fast/src/platform/test/window.rs) | 测试窗口将 `metal_context()` 转发到离屏 renderer；`PlatformHeadlessRenderer` 增加同名默认接口，`MetalHeadlessRenderer` 返回其实际 device/queue。 | 完整编辑器截图复用正式应用的异步预览与 Metal 覆盖层，不创建原生窗口。 |
| [gpui-fast/src/window.rs](gpui-fast/src/window.rs) | 新增 `Window::metal_context()` 转发；现有 `paint_surface()` 参数由 `CFRetained<CVPixelBuffer>` 扩展为 `impl Into<SurfaceSource>`，记录 source 和当前 element opacity；可选记录输入/CPU draw 边界并给场景附加关联 ID；允许画布临时屏蔽 Space 自动重复事件；删除不再使用的直接 CVPixelBuffer import。 | 原有 CoreVideo 调用仍可使用，同时接收原生纹理并继承元素透明度；Space 拖动不会被无效重复键盘事件打断绘制。 |
| [gpui-fast/src/elements/surface.rs](gpui-fast/src/elements/surface.rs) | 新增 `SurfaceSource::MetalTexture(metal::Texture)`；绘制时读取纹理尺寸，沿用 object-fit 与 `paint_surface()`；将派生的相等判断改为手写实现。Metal 纹理按对象地址判断相等，CoreVideo 保留原有判断。 | 普通 RGBA/BGRA 纹理能成为 Surface 元素；`metal::Texture` 本身没有 Eq 实现。比较的是资源身份，不是像素内容。 |
| [gpui-fast/src/gpui.rs](gpui-fast/src/gpui.rs)、[frame_trace.rs](gpui-fast/src/frame_trace.rs) | 导出可选 timing sink、统一 host clock、进程内 ID 和嵌套输入 scope；未启用时不分配事件或读取时钟。 | 编辑器与平台 renderer 共用时钟和关联 ID，日志策略仍由编辑器拥有。 |
| [gpui-fast/src/scene.rs](gpui-fast/src/scene.rs) | `PaintSurface.image_buffer` 替换为 `source: SurfaceSource`，增加 `opacity: f32`；Scene 增加可选计时的 `frame_trace_id`。 | 场景持有纹理所有权，并在重放、批次排序和合成时携带透明度。 |
| [gpui-fast-macos/src/window.rs](gpui-fast-macos/src/window.rs) | 仅新增 4 行 `PlatformWindow::metal_context()` 实现，转发给当前原生 renderer。 | 把平台层接口连接到实际窗口渲染器。 |
| [gpui-fast-apple/src/metal_renderer.rs](gpui-fast-apple/src/metal_renderer.rs) | 新增 `shared_metal_context()`；增加 RGBA Surface pipeline；`draw_surfaces()` 对原生纹理检查设备身份与 ShaderRead usage，绑定纹理/opacity，沿用现有顶点、裁剪、实例索引和批次顺序；仅对新 pipeline 使用 premultiplied 混合因子。CoreVideo 分支从 source 中取回 image_buffer；可选记录 drawable 获取、合成提交、GPU 完成和系统显示时间。 | 不回读、不重新上传地合成 Kasane 纹理，避免重复乘 alpha；保留现有 YUV 分支与混合行为。 |
| [gpui-fast-apple/src/shaders.metal](gpui-fast-apple/src/shaders.metal) | 新增 `rgba_surface_fragment`，采样纹理后乘 element opacity。 | Kasane 输出已经预乘 alpha；shader 不再乘一次纹理 alpha。 |

## 接口约束与兼容性

- 传入纹理使用 premultiplied RGBA/BGRA，并与窗口的 `SharedMetalContext.device` 相同，具备 `ShaderRead` usage。
- 生产者必须先在 `SharedMetalContext.queue` 提交绘制，再把纹理交给 GPUI。共享 device 本身不提供跨队列同步。
- 应用发布不可变帧；GPUI 场景保留纹理句柄，以免后台线程改写仍在显示的上一帧。此策略位于 editor，未增加 GPUI 帧调度机制。可选 benchmark 钩子只观察边界，不更改调度或等待 GPU。
- 直接构造 `PaintSurface` 的外部代码需要适配字段变化；穷尽匹配 `SurfaceSource` 的代码需要处理新变体。因此这不是对所有下游源码完全无影响的改动。
- Fast 补丁不修改 macOS renderer 选择、窗口生命周期、IME、字体或 Kit 组件。输入事件仅在画布显式接管 Space 期间屏蔽其自动重复 KeyDown；首次按下和松开仍正常分发。Kit 的独立菜单补丁见上文。

## 为什么还需要三个包

core 定义绘制与平台接口，macOS 负责窗口到 renderer 的转发，Apple 实现原生 Metal 合成，三者各有必要改动。Apple 包内的 `src/fast/release_gpui` 是发布包自带的 shader binding 生成输入，由其 `build.rs` 读取，不是第四个待删除的补丁包。

## 验证与后续维护

[原生合成测试](../crates/editor/tests/native_metal.rs) 验证 RGBA/BGRA、透明背景、文档与元素透明度叠加、裁剪、UI 遮挡、隐藏、旧帧保留、尺寸变化及缩放。中间不回读；只回读最终结果做断言。窗口交互由用户验证。

```sh
./scripts/test.sh
# 检查全部补丁能否从当前文件反向应用，不修改文件：
git apply --reverse --check vendor/drag-ownership.patch
git apply --reverse --check vendor/native-metal.patch
git apply --reverse --check vendor/cache-boundary.patch
git apply --reverse --check vendor/list-height-estimates.patch
git apply --reverse --check vendor/kit-workspace.patch
git apply --reverse --check vendor/compact-menu.patch
git apply --reverse --check vendor/compact-tooltip.patch
```

反向检查不能单独证明没有遗漏改动。核对补丁覆盖完整性时，应在临时目录准备上述原始发布包和 Kit 保留文件，按相同 `vendor/` 路径布局应用全部六份补丁，再逐文件比对重建结果与当前依赖包目录；文件集合及内容均应一致。

补丁使用仓库根目录相对路径，可应用到按相同目录布局放置的原始 0.1.7 发布包。更新 GPUI 时，应以新发布包为基线重新核对上述三份补丁涉及的 14 个文件，检查上游是否已修复缓存边界及列表高度估计，重新生成仍需要的补丁并更新统计，运行合成测试、完整 UI 截图测试和两个应用构建。原生 Metal 集成细节见 [native-metal.md](../docs/archive/native-metal.md)。

实际窗口 benchmark 的计时约定、命令和限制见 [窗口 benchmark](../docs/archive/window-benchmark.md)。

帧延迟排查新增 trace-only drawable 数、显示同步和离屏分段探针；只在应用已安装 timing sink 时读取诊断选项。常规呈现默认值保持三缓冲、显示同步开启和直接绘制。具体对照见 [帧延迟排查](../docs/archive/frame-latency-diagnosis.md)。

大网格覆盖层 benchmark 增加 `submit_scene_for_benchmark()`，仅在 test/bench-support/test-support 构建中提供：返回已提交的 command buffer 供读取 GPU 时间，并按尺寸复用 headless 路径中间纹理。已有 `render_scene()` 转发到该入口。正式窗口绘制路径不受影响。
