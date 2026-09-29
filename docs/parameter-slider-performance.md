# Parameters slider 性能（2026-09-29）

主因是每次拖动都构造、布局全部参数行，包括屏幕外的 Slider、标签和按钮。
Mao 有 128 个参数，原始全量列表的 UI 绘制 p95 达到 15.33 ms，超过 120 fps 的 8.33 ms 帧预算。
同期模型场景生成 p95 仅 0.12 ms。

## 最终实现

- 保留工作区已有的 `uniform_list` 虚拟化，只创建可见行的元素，不再每帧复制全量参数。
- 参数行使用独立 Entity 和缓存；Slider 的通知只使对应行失效。缓存使用明确的 3.5 rem 行高，滚动位置在文档结构变化时重置。
- 相同量化步进值和 Release 不重复提交模型更新；成功提交后更新本地值。
- 保持参数更新立即执行。按显示帧延后模型更新的实验增加了延迟，未采用。

## 窗口实测

Apple M4，主屏 3840×2160 / 1920×1080 @ 120 Hz，Release，窗口 1100×740 logical px。
Mao model3，30 次预热、360 次测量，目标输入 120 Hz；隔离配置、关闭测试进程的 Metal HUD 环境变量。
合成鼠标事件经过真实 Slider 命中测试、订阅回调、模型更新及异步 Metal 呈现，不包含 OS 输入送达时间。

基线使用本仓库 HEAD 的原始全量列表，加同样的测量入口；最终版本包含虚拟化、行缓存及重复值过滤。
基线并非本次开始时已经含有虚拟化的未提交版本。源码快照、环境和原始 trace 见下方归档。

| 指标 | 原始全量列表 | 最终版本 |
| --- | ---: | ---: |
| UI draw p50 | 13.97 ms | 2.46 ms |
| UI draw p95 | 15.33 ms | 2.66 ms |
| UI draw p99 | 15.86 ms | 2.79 ms |
| 窗口实际呈现频率 | 65.59 fps | 120.00 fps |
| 不同模型预览呈现频率 | 61.01 fps | 118.33 fps |
| 输入到呈现 p95 | 59.45 ms | 34.14 ms |
| 有效模型请求呈现 | 334 / 360 | 355 / 355 |

最终版本 360 次输入中有 5 次落在相同参数值，未重复生成预览；不同模型预览的间隔 p95 为 8.33 ms，p99 为 16.67 ms。
120 fps 描述呈现节奏，不代表输入到显示的延迟为 8.33 ms，也不保证其他模型、窗口尺寸或 Debug 构建的表现。
仅行缓存相对已虚拟化列表的提升较小；主要收益来自避免全量列表布局。

## 复现

```sh
KASANE_BENCH_PARAMETERS=1 scripts/window-benchmark.sh \
  models/local/mao/runtime/mao_pro.model3.json target/parameter-benchmark 360 120
```

不设置 `KASANE_BENCH_PARAMETERS` 时，原有 pan / zoom / opacity 工作负载保持不变。
benchmark 将 input scope 保持到 GPUI 完成事件队列 flush，确保 Slider 的异步订阅回调正确关联输入。
报告区分窗口呈现频率和不同模型帧呈现频率；窗口失焦、trace 丢失、测试未完成会标记为无效。

验证包括编辑器单元及 headless 测试、报告脚本测试，并覆盖重复点击/松手不再求值、128 行滚动后的编辑和重置。

测量归档：[原始列表](archive/benchmarks/2026-09-29/parameters/eager/summary.json)、[最终版本](archive/benchmarks/2026-09-29/parameters/optimized/summary.json)。
各目录保留压缩原始 trace、环境和 SHA-256，`sources.json.gz` 保留本次比较的源文件。
