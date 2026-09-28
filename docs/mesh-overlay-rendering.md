# Editor 的 Metal 网格覆盖层

正式 canvas 默认将边线与顶点标记一起绘制为透明 BGRA8 纹理，由 GPUI 在模型纹理之上、浮动工具栏之下合成。专用实现位于 `editor-render/src/overlay.rs` 和 `overlay.metal`，canvas 仅负责传入快照、主题与窗口坐标。

边线使用每边两个三角形的实例化绘制、1 个逻辑像素宽度、4× MSAA；标记在 resolve 后用单采样 pass 按原始顺序绘制。普通/悬停标记半径为 3/4 逻辑像素，选中状态和颜色来自实际 `OverlaySnapshot` 与主题，不使用 benchmark 的固定索引图案。标记位置沿用 GPUI 的半像素向零取整；边线保留亚像素坐标。纹理原点向物理像素下取整，记录画布原点偏移，避免非整数布局导致网格整体偏移。

资源与交互约束：

- 同一 GPUI device/queue 上先提交覆盖层，再合成纹理；UI 线程不等待 GPU、不回读像素。
- 每次变化生成独立目标纹理和输入缓冲，Metal command 保留输入，GPUI Scene 保留已发布纹理。不会用 benchmark 的单份可变缓冲覆盖仍在显示的旧帧。
- 缓存最近的几何、标记状态、主题、原点、尺寸和缩放，相同内容直接复用纹理；画布隐藏/无覆盖层时不绘制，空覆盖层清除缓存。新 device 重建 pipeline，新 queue 丢弃旧缓存并在新队列重新提交。
- 继续通过 `overlay_for_frame` 对齐实际显示的模型帧版本。命中测试、选择、拖拽、撤销、工具切换和指针捕获仍由既有 editor-core/canvas 逻辑处理。
- shader/device 初始化失败时记录一次错误并回退 GPUI；同一失败设备不会每帧重新编译 shader。

这是把已求值的画布坐标交给 GPU。CPU 仍构造覆盖层快照与实例数组；本轮没有迁移模型求值、边索引生成或命中测试。生产方案包含不可变资源分配和真实选择状态，不能直接套用 benchmark 中缓存静态 GPU 顶点的绝对耗时。

追踪时显式设置 `KASANE_TRACE_BATCH_MESH_EDGES=1` 选择 GPUI 合批基线，`=0` 选择 GPUI 逐边基线。不设置时默认完整 Metal，trace 元数据新增 `metal_mesh_overlay`（表示请求的路径；初始化失败仍可能回退）。`KASANE_TRACE_HIDE_MESH_EDGES=1` 继续仅隐藏边线。

原生回归测试 `crates/editor/tests/native_overlay.rs` 覆盖 1×/1.5×/2×、非整数原点、裁剪、半透明重叠、普通/选中/悬停标记、退化边、缓存复用、实际状态与主题更新、尺寸变化、队列切换和旧帧不被改写。使用 headless Metal，不打开窗口、不启动 benchmark 采样。

现有窗口追踪的 `gpui_gpu_*` 只表示 GPUI 合成 command，新覆盖层 producer 的 GPU 时间尚未单独打点；不能将该指标当成整个覆盖层的 GPU 总耗时。本次不追加性能采样。
