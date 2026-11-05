# DeepSeek viewport 首帧：原生 Metal 测量

2026-11-05，Apple M4（10 核 GPU），macOS 27.0.1 arm64，Rust 1.99.0，Release
默认 features，生产 Metal 预览及 GPUI Fast 原生窗口。viewport 为 **941 × 1312 物理像素**，
每个样本的首帧均有 518 次模型 draw call。
对照 `77f1383b` 与此前 I/O 优化分支，同轮交替运行各 5 次。
完整数据、指纹及显示环境见 [JSON](2026-11-05-deepseek-viewport-first-frame.json)。

## 首帧时间

计时从测试窗口发起模型加载前的 `load_begin` 开始。GPU 完成指首个实际呈现的模型
预览所用 Metal command buffer 的 `GPUEndTime`；首次窗口呈现另以对应 drawable 的
`presentedTime` 为准。均不含应用初始化或文件选择器时间。

每格为 **中位数（最小–最大）**，单位秒。

| 路径和终点 | 原版 77f1383b | 优化后 | 中位数变化 |
| --- | ---: | ---: | ---: |
| model3 导入 → viewport 首帧 GPU 完成 | 4.886（4.794–5.509） | 4.316（4.262–4.529） | −11.7% |
| 打开工程 → viewport 首帧 GPU 完成 | 4.263（3.970–4.408） | 3.468（3.356–3.628） | −18.7% |
| model3 导入 → 首次窗口确认呈现 | 4.912（4.824–5.535） | 4.342（4.295–4.561） | −11.6% |
| 打开工程 → 首次窗口确认呈现＊ | 4.295（4.026–4.434） | 3.518（3.415–3.695） | −18.1% |

＊打开工程的呈现值包含下面记录的**测试器对同一已完成图片的重绘等待**，不能当作
未加重绘处理的生产流程一定会在这些时间内显示图片。`presentedTime` 是系统呈现回调
提供的时间，不是外部设备测得的屏幕扫描/发光时间。

## 优化版的阶段成本

以下为同一轮采样各阶段的中位数，单位毫秒。阶段存在嵌套/并行，不能直接相加。

| 阶段 | 导入 | 打开 |
| --- | ---: | ---: |
| LoadTask worker（解码、纹理读取、对象索引） | 3066.3 | 2181.9 |
| 加载开始 → 首个 CPU 场景就绪 | 3282.9 | 2406.2 |
| CPU 场景就绪 → viewport 首帧 GPU 完成 | 1033.6 | 1046.4 |
| 首次纹理准备和上传 API | 940.1 | 975.8 |
| 模型 GPU command 执行 | 81.8 | 66.0 |
| 含模型图像的 UI 帧 draw | 5.0 | 7.8 |
| 窗口合成 GPU command 执行 | 8.4 | 5.0 |
| 模型 GPU 完成 → 窗口确认呈现 | 31.8 | 59.5 |

首次纹理阶段在
[MetalSceneRenderer](../../../crates/zipzi-editor-render/src/metal.rs)
中包括 `straight_rgba_mipmaps`、纹理分配、预乘及 `replace_region` 上传调用。
它是渲染 worker 的 CPU 墙钟阶段，不能称为“GPU 上传耗时”。模型 GPU 时间来自硬件
command buffer 的 start/end 时间。本轮没有分别消融上述纹理子阶段。

模型 GPU 执行的样本范围为导入 34.9–104.3 毫秒、打开 61.7–79.9 毫秒；首帧冷状态
包含模型/遮罩初始化，不能据此推断稳态 FPS。

## 首次合成丢帧

V2 正式样本全部保留，包括所有重绘。发生情况为：

| 版本 / 路径 | 首次合成丢帧后需重绘的样本 | 重绘请求总数 |
| --- | ---: | ---: |
| 原版导入 | 0 / 5 | 0 |
| 优化版导入 | 0 / 5 | 0 |
| 原版打开 | 1 / 5 | 1 |
| 优化版打开 | 4 / 5 | 5 |

这些情况下，模型渲染 command 已完成，窗口合成 command 也完成，但 drawable 回调
返回 `presentedTime == 0`，追踪为 `present_dropped`。
测试器调用 `window.refresh()` 重绘保留的图片，不使文档/CPU 场景失效，也不生成新的
模型 GPU 帧；报告强制要求重绘前后的 preview ID 相同。首次呈现仍必须有真实的
`presented` 事件，不能使用提交或“帧已收到”来代替。

初版测试器没有该重绘处理：在 `1-77f1383b-open` 中，第一次合成丢帧后 60 秒内没有
进一步呈现，试跑判失败。该次追踪及 7 个已完成试跑保存在
`target/viewport-first-frame/paired/`，不混入 V2 的完整正式采样。

这是**新进程、新窗口测试中的观察**。本轮没有确定窗口遮挡、焦点、系统呈现调度或
Editor 生命周期各自的影响，也没有统计常驻 Editor 正常文件打开流程的发生率。
因此 GPU 首帧完成与“用户一定已看到图片”需要分开报告。本轮没有修改生产呈现流程
去解决这个问题；重绘处理仅在首帧测试模式内启用。

## 方法与验证

- 输入为 `models/local/DeepSeek/大肥鱼.model3.json`，以及此前生成并保留的同一个
  `0-77f1383b.zipzi`。原版和优化版打开完全相同的归档字节。
- 每个样本新建进程、空工作区、独立配置/cache/Runtime 目录，普通空选区。
  不做网格选择、交互或预览预热，纹理缓存首次未命中。系统文件及驱动缓存未清空。
- 两边使用同一份运行时探针。先各运行导入/打开预热，再按 BA、AB、BA、AB、BA
  交替采样；A 为原版，B 为优化版。共 20 个正式进程、4 个预热进程。
- 编译及测试在正式计时前完成，运行冻结的二进制副本。机器未隔离 CPU/I/O 或用户
  输入；测试器自动激活窗口。主显示器报告 120 Hz，并镜像到另一块报告 60 Hz 的屏幕。
  不把配置中的 `input_hz=120` 当作实测显示刷新率。
- 关联链为加载 → Scene → accepted preview texture label → `compose_surface` →
  两个成功完成的 GPU command → 第一个真实 drawable 呈现。空窗口呈现、被丢弃的
  preview、失败的 GPU command 和丢失追踪事件均不能冒充首帧；已有更早真实呈现时，
  不选择后续重绘代替。逐次检查确认，丢帧重绘到首个实际呈现之间没有再提交模型 GPU 帧。
- 24 份完整追踪均通过校验，`lost_events=0`；物理 viewport 尺寸及 draw call 数一致。
  2 个 Rust 关联状态测试、6 个 Python 报告校验测试通过，shell 语法和 diff 检查通过。

新增入口为 `ZIPZI_BENCH_FIRST_FRAME=1`，见
[复现说明](../../../tools/benchmarks/README.md#viewport-first-frame)。
探针位于 [first_frame.rs](../../../crates/zipzi-editor/src/benchmark/first_frame.rs)，
报告工具为 [first-frame-trace-report.py](../../../tools/benchmarks/first-frame-trace-report.py)。
原始 V2 追踪、每次报告及汇总保存在 `target/viewport-first-frame/paired-v2/`，驱动脚本为
`target/python/scratch/viewport_first_frame.py`。

本轮未测 alpha4 原生首帧、Windows 或已缓存模型切换，亦未进行外部屏幕采样。
