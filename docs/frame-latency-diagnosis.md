# 实际窗口帧延迟排查

本文记录首次诊断对照；后续已默认启用边线合并，见 [正式优化与复测](mesh-edge-optimization.md)。

日期：2026-09-28。模型 Mao，Release，主屏配置 120 Hz，目标输入频率 120 Hz；每组平移、缩放、透明度各预热 30 次、正式输入 300 次。八组条件共 7,200 次正式输入，追踪日志无丢失，报告完整性检查通过。[所有对照摘要与本地归档说明](benchmarks/2026-09-28/window-diagnosis/README.md)。

## 已确认的主要瓶颈：网格边线导致路径批次过多

画布将选中网格的 **152 条边分别创建为 152 个 Path**。GPUI 的场景排序把这些路径与顶点等其他图元排列成 **49–56 个路径批次**；最常见的是 54 批。

相关代码：

- `crates/editor/src/canvas.rs`：逐条创建 `PathBuilder::stroke`，调用 `window.paint_path`。
- `vendor/gpui-pre/src/scene.rs`：按绘制顺序和图元类别构成 `PrimitiveBatch::Paths`。
- `vendor/gpui-pre-apple/src/metal_renderer.rs`：每个路径批次结束主 encoder，进入全窗口尺寸的 MSAA 中间纹理清理/绘制/resolve，再恢复主 encoder 合成路径。

因此一帧有约 **99–113 个 render encoder**（初始 1 个，每个路径批次增加 2 个）。大量小路径引发了重复制备中间纹理和切换 render pass 的成本；并非 Kasane 模型的三角形绘制本身占据了这些时间。这里确认的是整个路径绘制链路的成本，尚未用 GPU 硬件计数器细分 resolve 带宽与其他 pass 成本。

用只在追踪时生效的诊断开关，把全部边线保留为一个具有独立子路径的 Path：路径批次变成 **1 批**，对应 3 个 render encoder。模型、顶点、场景求值、命中几何及视口变换仍走原流程。

### 同一组负载的结果

| 指标 | 路径计数基准 | 合并边线路径 |
| --- | ---: | ---: |
| GPUI GPU 中位耗时（三种操作范围） | 7.23–7.58 ms | 1.02–1.15 ms |
| GPUI GPU P95 | 8.02–8.11 ms | 1.39–1.74 ms |
| 预览在 GPU 队列中等待，中位数 | 5.18–7.35 ms | 0.47–0.96 ms |
| 预览结果交付到 UI，中位数 | 7.44–8.07 ms | 0.011–0.013 ms |
| 实际输入频率 | 102.82–114.64 Hz | 119.98–120.00 Hz |

这些是各阶段分别统计的分位数，不能相加。

| 操作 | 输入到显示 P95：基准 → 合并 | 显示请求：基准 → 合并 |
| --- | ---: | ---: |
| 平移 | 48.30 → 39.08 ms | 300/300 → 289/300 |
| 缩放 | 56.23 → 39.39 ms | 300/300 → 297/300 |
| 透明度 | 56.31 → 29.07 ms | 290/300 → 261/300 |

合并组有 53 个请求在 UI 接收后、进入合成前被后续状态取代。其延迟分位数仅基于显示成功的请求；输入负载实际上也更接近目标 120 Hz。因此 **不能仅凭延迟降低就宣称所有输入都能逐帧显示或稳定 120 fps**。原始 CSV 保留了全部请求与预热。

## 瓶颈如何传到主线程和模型预览

补充计时基准（`default`）中：

- 预览 GPU 排队区间约 **98%** 与前面的 GPUI 合成命令 GPU 区间重叠。两个渲染器共享同一 Metal command queue，沉重的 GPUI 合成把后续模型预览也拖慢了。
- 预览发布后到 UI 收到的等待区间约 **70–73%** 与主线程的 `nextDrawable()` 阻塞重叠。该 API 在 drawable 不可用时等待，主线程期间也无法及时处理输入和接收结果。
- 合并边线后，GPUI 命令缩短，预览排队和 UI 交付等待随之明显下降，验证了这条传导关系。

这些重叠比例按原始时间区间求交集计算；重叠区间先求并集，避免重复计数。它们是时间上的证据，不等于给每个 CPU/GPU 周期确定了唯一原因。

## 已排除的简单调整

| 对照 | 观察 | 结论 |
| --- | --- | --- |
| 原始绘制：drawable 3 → 2 | 平移中位延迟约 48 → 64 ms，输入间隔趋近 16.7 ms | 直接减少缓冲会加剧主线程等待 |
| 原始绘制：关闭显示同步 | 平移中位延迟仍约 49 ms；等待从提交后转移到更早阶段 | 不能靠关闭同步解决，也未作为产品设置启用 |
| 相同 UI 先画到私有纹理，再复制到 drawable | UI 绘制仍约 7–8 ms，复制命令约 0.2 ms；端到端没有可靠改善 | GPU 大头在界面绘制，不是单纯复制模型纹理 |
| 只隐藏网格边线 | GPUI GPU 约 1 ms | 支持边线绘制是主要耗时来源；这只是排除实验 |
| 合并边线后再使用双缓冲 | 提交后等待由约 32 降到 24 ms，但更早阶段等待增加；三种操作 P95 仍约 41 ms | 相比合并路径＋三缓冲没有进一步改善整体延迟 |

离屏对照改变了分配和提交结构，只用于区分工作阶段，不是性能优化候选。它的 `offscreen_gpu_ms` 才是界面绘制；`compose_gpu_ms` 是复制/呈现命令，不能直接与正常组的同名字段比较。

## 还没有解决的延迟

合并边线后三缓冲组，从窗口提交到系统 drawable 显示时间仍有约 **26–32 ms**。双缓冲能缩短这段，但把等待前移，总延迟没有改善。原始各轮的端到端延迟也会相差一个显示周期，不能把 48 ms 当作固定常数。

这部分已经定位到提交后的显示链路，但尚不能进一步断言是 WindowServer 的哪一级缓冲或具体调度机制。本机没有 `xctrace`，尚未做系统级 Metal/显示调度追踪。后续应检查显示节拍、在途呈现数量与 drawable 获取方式；增加场景求值线程不是本次数据支持的优先方向。

## 本次代码与验证范围

- 增加精确的 `drawable_wait_begin`、合成 scheduled 回调时间、路径批次数，以及提交/完成到显示的延迟字段。
- 报告新增 UI 交付与 drawable 阻塞、预览排队与 GPUI GPU 执行的区间重叠统计。
- 增加五个仅在追踪模式启用的诊断选项，这八组实验期间应用默认使用逐边路径、三缓冲、开启显示同步、直接绘制。
- 这八组实验验证了保留全部线段时的性能收益，当时尚未默认启用。后续已补上原生 Metal 像素检查、大网格分块和更多模型采样，并正式启用；详见上述优化记录。UI 视觉验收仍由用户手动完成。
- Workspace 69 项测试、报告工具 8 项测试通过；格式、差异空白和 GPUI 补丁一致性检查通过。

## 重现

共同命令（输出目录必须不存在旧 trace）：

```sh
./scripts/window-benchmark.sh \
  models/local/mao/runtime/mao_pro.model3.json target/recheck 300 120
```

给命令添加以下环境变量可重现对照；它们只在 `KASANE_FRAME_TRACE` 安装追踪 sink 后生效：

| 变量 | 默认/可选值 | 用途 |
| --- | --- | --- |
| `GPUI_TRACE_DRAWABLES` | 默认 3，可选 2/3 | drawable 池大小 |
| `GPUI_TRACE_DISPLAY_SYNC` | 默认 1，可选 0/1 | 显示同步 |
| `GPUI_TRACE_OFFSCREEN` | 默认 0，可选 0/1 | UI 离屏与复制分段探针 |
| `KASANE_TRACE_HIDE_MESH_EDGES` | 默认 0，可选 0/1 | 隐藏边线作排除实验 |
| `KASANE_TRACE_BATCH_MESH_EDGES` | 当前默认 1，可选 0/1 | 1 为正式批量绘制；0 恢复旧逐边路径作对照 |

例如：

```sh
KASANE_TRACE_BATCH_MESH_EDGES=0 ./scripts/window-benchmark.sh \
  models/local/mao/runtime/mao_pro.model3.json target/recheck-baseline 300 120
```

`compose_scheduled_callback_ms` 是 scheduled 回调被调用的时间差，包含回调通知延迟，并非 GPUStartTime 或驱动的 kernelEndTime。GPU 区间使用 Metal 的 GPUStartTime/GPUEndTime；显示时间使用 drawable presentedTime，均未以 CPU 回调接收时刻替代。采样从 GPUI 输入入口开始，不含硬件输入、AppKit 原生投递和屏幕像素响应。

参考：[Apple nextDrawable](https://developer.apple.com/documentation/quartzcore/cametallayer/1478172-nextdrawable)、[GPUStartTime](https://developer.apple.com/documentation/metal/mtlcommandbuffer/gpustarttime)。
