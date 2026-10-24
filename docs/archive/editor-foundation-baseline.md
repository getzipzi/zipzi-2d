# 工作区基础实现：CPU 与原生 Metal 性能记录

2026-09-28。包含工作区 CPU 初始基线及完整实现后的无窗口原生 Metal 基线。
不是改动前后对照；不包含真实输入到屏幕显示的完整端到端性能验收。

## 环境与输入

- Apple M4，arm64，16 GiB；macOS 27.0 / 26A428。
- rustc 1.98.1（Homebrew）；工作区 dev profile，debug=0，未启用 release 优化。
- 无窗口的 `zipzi-editor-core` example；HUD 关闭，显示倍率/画布尺寸/GPU 均不适用。
- 模型：`models/local/mao/runtime/mao_pro.model3.json` 与
  `models/local/Shirousagi/Shirousagi.model3.json`，相对仓库根目录；运行时 canonicalize 为绝对路径。
- 每轮创建空工作区，依次导入 Mao、求首场景、导入 Shirousagi、求首场景，随后交替切换 10 次。
  共 5 轮，CPU `Instant` 墙钟计时；每次切换包括取得已缓存场景，不含 GPU 提交/首帧显示。
- 首样本是本次进程第一次调用；未清操作系统文件缓存，不能称为磁盘冷启动。

## 结果

| CPU 操作 | 样本数 | 首样本 ms | 中位数 ms | 最小–最大 ms |
| --- | ---: | ---: | ---: | ---: |
| Mao 导入与资源解码 | 5 | 1479.667 | 1438.597 | 1433.198–1479.667 |
| Mao 初次场景求值 | 5 | 7.442 | 7.550 | 7.438–7.642 |
| Shirousagi 导入与资源解码 | 5 | 232.750 | 232.073 | 225.375–253.768 |
| Shirousagi 初次场景求值 | 5 | 1.476 | 1.459 | 1.414–1.509 |
| 双文档切换并取得缓存场景 | 50 | 0.001125 | 0.000375 | 0.000333–0.001333 |

非常短的缓存操作包含计时器噪声；不能据此推断实际标签切换到可见帧的延迟。
导入样本较少，报告中位数与范围，不将其包装成稳定的 p95/p99 预算。
原始逐样本数据见 [CSV](benchmarks/2026-09-28-workspace-cpu.csv)。

Mao 输出 260 个 drawables、解码 RGBA 67,108,864 字节；Shirousagi 输出 24 个 drawables、
解码 RGBA 16,777,216 字节。这是 CPU 像素缓冲大小，不是进程或 GPU 内存。
`/usr/bin/time -l` 对本次完整 CPU example 测得最大 RSS **202,391,552 字节**、
peak memory footprint **197,607,952 字节**，总墙钟 **8.87 秒**。
它是整个五轮双文档流程的高水位；未分别采集单模型/双模型/关闭后的稳定占用，
也不代表 GPUI 编辑器进程的占用。

## 复现

从仓库根目录执行：

```sh
cargo build --locked -p zipzi-editor-core --example workspace_baseline
/usr/bin/time -l target/debug/examples/workspace_baseline \
  models/local/mao/runtime/mao_pro.model3.json \
  models/local/Shirousagi/Shirousagi.model3.json 5
```

## 原生 Metal 工作负载

同一 Apple M4 / 16 GiB / macOS 27.0 / Rust 1.98.1，dev profile 未优化、HUD 关闭。
`zipzi-editor-render/examples/workspace_gpu_baseline.rs` 使用一个 Metal device/queue，
逻辑画布 640 × 400、倍率 2、物理输出 1280 × 800、RGBA8Unorm，无窗口、无像素回读。
该尺寸为基准显式参数；真实 UI 使用 CanvasView 实测 bounds。

顺序：导入 Mao，首帧后 100 次缩放绘制；保留 Mao 并导入 Shirousagi，首帧后 100 次缩放绘制；
两个已缓存场景交替切换并绘制 100 次；关闭当前标签；释放工作区和渲染器。
每帧用 autoreleasepool，末尾在同一 queue 提交 barrier 并等待完成，仅用于测量。
计时包含 CPU 场景获取/必要求值、提交和 GPU 完成；生产显示路径不等待 GPU。
每种首次导入/首帧各 1 个样本，进程此前未上传这些纹理，但未清系统文件缓存。
热操作采用 nearest-rank 分位数；切换样本混合两个模型各 50 次。

| 操作 | 样本 | p50 ms | p95 ms | p99 ms |
| --- | ---: | ---: | ---: | ---: |
| Mao 热缩放绘制 | 100 | 16.044 | 18.354 | 18.674 |
| Shirousagi 热缩放绘制（Mao 同时打开） | 100 | 1.488 | 3.038 | 5.217 |
| 双文档激活到 GPU 完成 | 100 | 4.254 | 16.946 | 18.114 |

Mao 导入 1446.179 ms、随后首帧 1387.497 ms；Shirousagi 导入 230.878 ms、随后首帧 338.391 ms。
首帧包含 CPU mipmap 生成、上传、首次求值与渲染，仅报告单次值，不推导稳定尾延迟。
GPU 完成时间包含该进程以外 GPU 工作的潜在竞争，不是纯 GPU kernel 时间。

| 采样状态 | RSS MiB | Metal allocated MiB | 纹理缓存（含 mip）MiB | 累计绘制 / 上传 |
| --- | ---: | ---: | ---: | ---: |
| 仅 Mao，热绘制后 | 213.61 | 87.30 | 85.33 | 101 / 1 |
| Mao + Shirousagi，后者热绘制后 | 252.06 | 108.81 | 106.67 | 202 / 2 |
| 双模型切换后 | 252.11 | 108.81 | 106.67 | 302 / 2 |
| 关闭当前 Shirousagi 标签后 | 252.11 | 108.81 | 106.67 | 302 / 2 |
| 释放全部文档与渲染器后 | 252.08 | 1.28 | 0 | — |

RSS 通过 `ps -o rss` 读取，Metal allocated 为 `Device::current_allocated_size()`；
缓存字节为实际 RGBA/mipmap payload，两者不可当成同一种内存或相加。
这里没有 UI 合成、长期保留的屏幕输出纹理，也没有单独采集 Shirousagi 单文档进程。
RSS 在释放后仍高，反映进程分配器/运行库保留，不能据此断言文档或纹理泄漏。
关闭标签后保留近期 GPU 缓存是预期策略；所有渲染器释放后 Metal 分配明显回落。
原始数据：[Metal CSV](benchmarks/2026-09-28-workspace-metal.csv)。

### 可执行缓存预算

实测两个完整模型的纹理含 mip 共 **111,848,104 字节（106.67 MiB）**，
采用 **128 MiB** 近期纹理预算，可同时保留这组基准，并为后续模型限制闲置缓存增长。
超额时按最近使用顺序回收不属于当前绘制的纹理；活动模型本身超过预算时允许超额。
这是纹理缓存预算，不包含输出帧、mask、CPU 解码图像或其他 Metal 内部资源，也不是进程内存上限。
本次 302 次绘制只有 2 次上传，证明此工作负载的缩放/切换未反复上传。
核心计数测试另外验证悬停/选择不重新求值、缩放/平移/尺寸变化复用求值场景；
原生像素测试验证同 asset ID、不同内容隔离，以及同内容不同 ID 共享。

复现：

```sh
cargo run --locked -p zipzi-editor-render --example workspace_gpu_baseline -- \
  models/local/mao/runtime/mao_pro.model3.json \
  models/local/Shirousagi/Shirousagi.model3.json 100
```

### 未测量项及复现条件

真实 AppKit/GPUI 首帧可见延迟、输入到匹配帧显示、Dock 调整与多屏 DPI、IME/指针交互
尚未人工测量。它们不由上述无窗口结果推断。人工测量应记录实际 bounds、屏幕倍率、
相同构建/HUD 配置，至少 10 次独立首次显示与各 100 次缩放/切换，并报告 nearest-rank 分位数。
当前数据未建立交互帧率达标结论；Mao 在未优化构建的 p95 已超过 16.67 ms，
后续性能优化应先以 release 构建复测，再定位 CPU 提交与 GPU 工作的分摊。
