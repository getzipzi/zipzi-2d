# DeepSeek viewport 首帧纹理优化

2026-11-05，Apple M4（10 核 GPU），macOS 27.0.1，Rust 1.99.0，Release 默认
features，Metal。此次只优化、测量同一个固定场景的首帧渲染，**不再比较导入与打开工程**。
完整样本及指纹见 [JSON](2026-11-05-first-frame-texture-optimization.json)。

## 固定场景结果

相同的 DeepSeek 原生归档，941×1312 物理像素，518 次模型 draw call。
715 个纹理别名对应 490 张唯一图片，源 RGBA 共 585,517,924 字节；两版均上传
490 次，包含 mipmap 的 GPU 纹理共 780,216,416 字节。

计时起点为 `MetalSceneRenderer::submit_transformed_traced` 调用前，场景已就绪。
加载、renderer 创建、参考算法校验及 GPU 回读均不计时。每个样本使用新 renderer，
纹理缓存为空。原版为精确 `77f1383b`，加入相同的测试 example；优化版包含本分支更改。
每版各 3 个新进程，顺序 A/B、B/A、A/B；A 为原版。

单位毫秒，数值为 **中位数（最小–最大）**。

| 阶段 | 77f1383b | 优化后 | 中位数变化 |
| --- | ---: | ---: | ---: |
| CPU 纹理准备、分配和上传调用 | 985.2（952.4–995.2） | 330.5（313.5–346.4） | −66.5% |
| 首帧提交到 Metal 队列 | 988.8（955.7–1001.5） | 334.5（319.0–350.3） | −66.2% |
| 首帧 GPU 完成，含前述 CPU 工作 | 1053.6（1001.6–1101.9） | 375.2（359.6–422.3） | **−64.4%，2.81 倍快** |

第一行是 CPU 墙钟范围，包含 mipmap、预乘、分配和 `replace_region` 调用，不能称为
纯 GPU 上传耗时。最后一行通过 command buffer 完成等待确定终点，包含调度/等待开销。
该离屏实验在计时前运行逐字节校验和 CPU mipmap 探针，因此源图片已被读取；不能称为
CPU/系统缓存全冷。新进程也不清空文件、驱动或系统缓存。编译和本轮测试均在采样前结束；
机器没有隔离其他应用或用户操作。

## 真实 viewport 验证

另外启动 3 个全新原生 Editor 进程，读取同一份固定归档，在空选区、独立配置及
Runtime/cache 目录中检查生产 Metal 渲染和 GPUI Fast 窗口。
以下时间从 CPU `scene_end` 起算，排除加载；没有重跑导入/打开的性能对照。

| 阶段 | 优化后中位数（最小–最大），ms |
| --- | ---: |
| CPU 场景就绪 → 模型 GPU 首帧完成 | **424.9（413.6–439.9）** |
| CPU 场景就绪 → 首次窗口确认呈现 | **459.5（444.4–466.4）** |
| 首次纹理准备和上传调用 | 351.8（348.5–368.7） |
| 模型 GPU command 执行 | 64.9（59.6–67.9） |

三个样本均为 941×1312、518 次模型 draw call，追踪事件无丢失，两次 GPU command
成功完成，所选 preview 确实进入第一个实际呈现的窗口合成。三个样本的
`present_dropped` 和测试器 retained redraw 请求均为 **0**。
`presentedTime` 是系统呈现确认，不是屏幕发光/扫描时间；三次未丢帧不能证明此前观察到的
偶发合成丢帧已被修复。本轮没有修改生产窗口呈现流程。

## 改动

- `zipzi-render::texture` 针对 mipmap 的半尺寸降采样：偶数尺寸使用等价的整数 2×2
  加权滤波；奇数尺寸复用预计算的轴向覆盖权重，保持原浮点运算及舍入顺序。
  通用面积滤波及缩略图/取消路径保持独立，滤波版本不变。
- 增加预乘原地转换，对完全透明/不透明像素跳过无须执行的乘除。先生成完整的直通 RGBA
  mip 链，再转换工作缓冲，避免改变下一层的滤波输入或作者图像。
- Metal Editor 缓存未命中的纹理按内容 key 排重后分批准备；最多 4 个 CPU worker，
  按图片字节数平衡负载。估计的基图、mipmap 和权重表临时空间每批限制为 64 MiB，
  单张超限图片独立处理。该上限不包含已有场景/GPU 缓存等常驻资源。
- `upload_premultiplied_rgba8` 接收准备好的像素，保留尺寸和 mip 校验，去掉上传线程的
  再次转换/分配。Metal API 调用和缓存 revision 按原顺序执行，已缓存帧不创建准备线程。

## 正确性与复现

- 490 张真实图片的每一级 mipmap 均与原通用面积滤波逐字节一致。
- 两版共 6 份最终 GPU 回读，直接比较 **4,938,368 字节全部相同**，SHA-256 为
  `d510ecf3f12f6a19c1f26d711562b873a721156f9b693aebf1a2cabac39fd3e4`。
- CPU 纹理测试 10 个通过，包含 1–33 的所有宽高组合、较大尺寸、奇偶/单像素轴及
  随机、全透明、全不透明、低透明度数据的各级参考对照。
- Metal Scene 测试 4 个通过，覆盖批量内存估算、并行结果顺序、内容别名去重、缓存淘汰及
  真实 GPU 输出；另一个需要 Mao 本地资源的既有可选测试未运行。
- Metal pipeline/texture 测试 8 个通过，包含全部 256×256 通道/alpha 组合、奇数行宽、
  各级 mip 的两种上传 API 回读对照及错误尺寸/mip 校验。
- 3 份真实窗口追踪均通过首帧关联校验。改动文件的 rustfmt 和 `git diff --check` 通过。
  全 workspace 格式检查仍报告本轮未修改文件的既有差异，包括 runtime protocol、Editor
  settings 和 editor-core 部分测试；未批量改写这些文件。

固定场景入口及测量范围见
[zipzi-editor-render README](../../../crates/zipzi-editor-render/README.md)。

```sh
cargo build --locked --release -p zipzi-editor-render --example first_frame_benchmark
target/release/examples/first_frame_benchmark /absolute/path/to/deepseek.zipzi 3 target/deepseek.rgba
cargo test --locked -p zipzi-render --lib texture::
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-render --lib metal::tests::
cargo test --locked -p zipzi-render-metal --test metal_smoke pipeline_textures::
```

此次原始记录、GPU 回读及原生窗口追踪位于 `target/viewport-first-frame/textures-final/`，
驱动为 `target/python/scratch/first_frame_textures.py`，所有二进制和相关源码的 SHA-256
均记录在 JSON 中。原生窗口测试方式见
[首帧追踪说明](../../../tools/benchmarks/README.md#viewport-first-frame)。
本轮不推断稳态 FPS、Windows 性能或其他模型的收益。
