# GPUI 本地补丁清单

## 对比基线

仅保留 crates.io 发布的 `gpui-fast`、`gpui-fast-macos`、`gpui-fast-apple` **0.1.7**，作为 GPUI Kit 0.7.3 的 `gpui-fast` 后端。根目录 `Cargo.toml` 的 `[patch.crates-io]` 选择这些本地包；Kit 本身仍使用官方固定 commit。

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
- macOS renderer 选择、窗口生命周期、IME、字体、Kit 组件均未改动。输入事件仅在画布显式接管 Space 期间屏蔽其自动重复 KeyDown；首次按下和松开仍正常分发。

## 为什么还需要三个包

core 定义绘制与平台接口，macOS 负责窗口到 renderer 的转发，Apple 实现原生 Metal 合成，三者各有必要改动。Apple 包内的 `src/fast/release_gpui` 是发布包自带的 shader binding 生成输入，由其 `build.rs` 读取，不是第四个待删除的补丁包。

## 验证与后续维护

[原生合成测试](../crates/editor/tests/native_metal.rs) 验证 RGBA/BGRA、透明背景、文档与元素透明度叠加、裁剪、UI 遮挡、隐藏、旧帧保留、尺寸变化及缩放。中间不回读；只回读最终结果做断言。窗口交互由用户验证。

```sh
cargo test --locked --workspace
# 只核对完整补丁与当前源码是否一致，不修改文件：
git apply --reverse --check vendor/native-metal.patch
git apply --reverse --check vendor/cache-boundary.patch
git apply --reverse --check vendor/list-height-estimates.patch
```

补丁使用仓库根目录相对路径，可应用到按相同目录布局放置的原始 0.1.7 发布包。更新 GPUI 时，应以新发布包为基线重新核对上述三份补丁涉及的 14 个文件，检查上游是否已修复缓存边界及列表高度估计，重新生成仍需要的补丁并更新统计，运行合成测试、完整 UI 截图测试和两个应用构建。原生 Metal 集成细节见 [native-metal.md](../docs/archive/native-metal.md)。

实际窗口 benchmark 的计时约定、命令和限制见 [窗口 benchmark](../docs/archive/window-benchmark.md)。

帧延迟排查新增 trace-only drawable 数、显示同步和离屏分段探针；只在应用已安装 timing sink 时读取诊断选项。常规呈现默认值保持三缓冲、显示同步开启和直接绘制。具体对照见 [帧延迟排查](../docs/archive/frame-latency-diagnosis.md)。

大网格覆盖层 benchmark 增加 `submit_scene_for_benchmark()`，仅在 test/bench-support/test-support 构建中提供：返回已提交的 command buffer 供读取 GPU 时间，并按尺寸复用 headless 路径中间纹理。已有 `render_scene()` 转发到该入口。正式窗口绘制路径不受影响。
