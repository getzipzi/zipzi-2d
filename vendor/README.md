# GPUI 原生 Metal 补丁清单

## 对比基线

仅保留 crates.io 发布的 `gpui-pre`、`gpui-pre-macos`、`gpui-pre-apple` **0.3.6**，对应 Zed commit `bcf6582ce3500df93a8a39366640173e6786cea6`。根目录 `Cargo.toml` 的 `[patch.crates-io]` 选择这些本地包；Kit 本身仍使用官方固定 commit。

已逐文件与这三个发布包核对：**实际修改/新增 10 个文件（9 个源码文件 + 1 个依赖清单），新增 467 行、删除 18 行**。其他包内文件保持发布版本原样。完整差异见 [native-metal.patch](native-metal.patch)，不必从整个 vendor 目录中寻找改动。

`Cargo.toml.orig` 保留发布包附带的原始工作区清单，并非本地实际构建清单；构建使用标准化的 `Cargo.toml`。Apache-2.0 许可文件保留。

## 逐文件修改

| 文件 | 修改的符号与行为 | 保留原因 |
| --- | --- | --- |
| [gpui-pre/Cargo.toml](gpui-pre/Cargo.toml) | 仅给 macOS 添加 `metal = "0.33"` 依赖。 | 核心接口需要持有原生 device、queue、texture 类型。 |
| [gpui-pre/src/platform.rs](gpui-pre/src/platform.rs) | 新增 `SharedMetalContext { device, queue }`；为 `PlatformWindow` 增加 `metal_context()`，默认返回 `None`，均限 macOS。 | 让应用取得渲染器实际使用的资源，避免创建不兼容设备或另一个提交队列。 |
| [gpui-pre/src/window.rs](gpui-pre/src/window.rs) | 新增 `Window::metal_context()` 转发；现有 `paint_surface()` 参数由 `CVPixelBuffer` 扩展为 `impl Into<SurfaceSource>`，记录 source 和当前 element opacity；可选记录输入/CPU draw 边界并给场景附加关联 ID；允许画布临时屏蔽 Space 自动重复事件；删除不再使用的直接 CVPixelBuffer import。 | 原有 CoreVideo 调用仍可使用，同时接收原生纹理并继承元素透明度；Space 拖动不会被无效重复键盘事件打断绘制。 |
| [gpui-pre/src/elements/surface.rs](gpui-pre/src/elements/surface.rs) | 新增 `SurfaceSource::MetalTexture(metal::Texture)`；绘制时读取纹理尺寸，沿用 object-fit 与 `paint_surface()`；将派生的相等判断改为手写实现。Metal 纹理按对象地址判断相等，CoreVideo 保留原有判断。 | 普通 RGBA/BGRA 纹理能成为 Surface 元素；`metal::Texture` 本身没有 Eq 实现。比较的是资源身份，不是像素内容。 |
| [gpui-pre/src/gpui.rs](gpui-pre/src/gpui.rs)、[frame_trace.rs](gpui-pre/src/frame_trace.rs) | 导出可选 timing sink、统一 host clock、进程内 ID 和嵌套输入 scope；未启用时不分配事件或读取时钟。 | 编辑器与平台 renderer 共用时钟和关联 ID，日志策略仍由编辑器拥有。 |
| [gpui-pre/src/scene.rs](gpui-pre/src/scene.rs) | `PaintSurface.image_buffer` 替换为 `source: SurfaceSource`，增加 `opacity: f32`；Scene 增加可选计时的 `frame_trace_id`。 | 场景持有纹理所有权，并在重放、批次排序和合成时携带透明度。 |
| [gpui-pre-macos/src/window.rs](gpui-pre-macos/src/window.rs) | 仅新增 4 行 `PlatformWindow::metal_context()` 实现，转发给当前原生 renderer。 | 把平台层接口连接到实际窗口渲染器。 |
| [gpui-pre-apple/src/metal_renderer.rs](gpui-pre-apple/src/metal_renderer.rs) | 新增 `shared_metal_context()`；增加 RGBA Surface pipeline；`draw_surfaces()` 对原生纹理检查设备身份与 ShaderRead usage，绑定纹理/opacity，沿用现有顶点、裁剪、实例索引和批次顺序；仅对新 pipeline 使用 premultiplied 混合因子。CoreVideo 分支从 source 中取回 image_buffer；可选记录 drawable 获取、合成提交、GPU 完成和系统显示时间。 | 不回读、不重新上传地合成 Kasane 纹理，避免重复乘 alpha；保留现有 YUV 分支与混合行为。 |
| [gpui-pre-apple/src/shaders.metal](gpui-pre-apple/src/shaders.metal) | 新增 `rgba_surface_fragment`，采样纹理后乘 element opacity。 | Kasane 输出已经预乘 alpha；shader 不再乘一次纹理 alpha。 |

## 接口约束与兼容性

- 传入纹理使用 premultiplied RGBA/BGRA，并与窗口的 `SharedMetalContext.device` 相同，具备 `ShaderRead` usage。
- 生产者必须先在 `SharedMetalContext.queue` 提交绘制，再把纹理交给 GPUI。共享 device 本身不提供跨队列同步。
- 应用发布不可变帧；GPUI 场景保留纹理句柄，以免后台线程改写仍在显示的上一帧。此策略位于 editor，未增加 GPUI 帧调度机制。可选 benchmark 钩子只观察边界，不更改调度或等待 GPU。
- 直接构造 `PaintSurface` 的外部代码需要适配字段变化；穷尽匹配 `SurfaceSource` 的代码需要处理新变体。因此这不是对所有下游源码完全无影响的改动。
- macOS renderer 选择、窗口生命周期、IME、字体、Kit 组件均未改动。输入事件仅在画布显式接管 Space 期间屏蔽其自动重复 KeyDown；首次按下和松开仍正常分发。

## 为什么还需要三个包

core 定义绘制与平台接口，macOS 负责窗口到 renderer 的转发，Apple 实现原生 Metal 合成，三者各有必要改动。Apple 包内的 `vendor/gpui` 是发布包自带的 shader binding 生成输入，由其 `build.rs` 读取，不是第四个待删除的补丁包。

## 验证与后续维护

[原生合成测试](../crates/editor/tests/native_metal.rs) 验证 RGBA/BGRA、透明背景、文档与元素透明度叠加、裁剪、UI 遮挡、隐藏、旧帧保留、尺寸变化及缩放。中间不回读；只回读最终结果做断言。窗口交互由用户验证。

```sh
cargo test --locked --workspace
# 只核对完整补丁与当前源码是否一致，不修改文件：
git apply --reverse --check vendor/native-metal.patch
```

补丁使用仓库根目录相对路径，可应用到按相同目录布局放置的原始 0.3.6 发布包。更新 GPUI 时，应以新发布包为基线重新核对以上 10 个文件，重新生成补丁并更新统计，运行合成测试和两个应用构建。原生 Metal 集成细节见 [native-metal.md](../docs/native-metal.md)。

实际窗口 benchmark 的计时约定、命令和限制见 [窗口 benchmark](../docs/window-benchmark.md)。

帧延迟排查新增 trace-only drawable 数、显示同步和离屏分段探针；只在应用已安装 timing sink 时读取诊断选项。常规呈现默认值保持三缓冲、显示同步开启和直接绘制。具体对照见 [帧延迟排查](../docs/frame-latency-diagnosis.md)。
