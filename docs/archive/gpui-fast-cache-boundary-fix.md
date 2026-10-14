# GPUI Fast 虚拟列表缓存边界正式修复

日期：2026-10-13。基于 [前轮定位与实验](archive/gpui-fast-reproduction.md)，正式修复已应用于本地 `gpui-fast 0.1.7`；不再需要实验开关，也不增加缓存诊断日志。

## 改动

`RetainedSubtree` 独立记录 `cache_boundary`。显式缓存视图创建记录时设为 true，跨帧复用和 splice 复制时保留；`is_cached()` 直接读取该标志。能否独立重建仍由原有 `Rebuild` 决定，不再用它推断显式缓存身份。移除了不再使用的 `Rebuild::is_cached()`。

这修正了 `uniform_list` 中没有父布局 key 的 `.cached()` 行：它们仍是缓存边界，后代未通知的更新不会被错误传播成重复构建；实际通知和其他失效条件仍有效。editor 的数字输入没有额外包缓存，也没有改变控件尺寸或业务交互。

- [引擎正式补丁](../vendor/cache-boundary.patch)，只含 `fast/retained.rs` 与 `fast/splice.rs`。
- [回归测试](../crates/editor/headless_tests/view_cache.rs)：`virtual_list_cached_rows_ignore_render_writes_but_refresh_notified_inputs`。
- [vendor 维护清单](../vendor/README.md)。本修复独立于已有的 `native-metal.patch`。

## 回归验证

测试使用真实 `uniform_list`，每个显式缓存行里放置一个普通子视图；子视图在 render 中写入配置状态，父视图收到通知前还会有未通知的配置更新。分别在 retention on/off 下验证：连续父级通知不重建缓存行或输入；子输入通知后改变绘制结果；不变的兄弟输入继续复用；后续帧保持更新后的内容；全量刷新仍重建所有输入。

同一测试在未修复引擎上失败：缓存行 render 计数从 `[3, 2]` 增长到 `[5, 3]`；应用修复后通过。仅有“给自己写配置”不足以触发此回归，测试同时覆盖缓存后代被外部无通知更新的失效条件。

- Release 工作区：400 passed、0 failed、4 ignored；另完成三模型动画验收及真实 Metal headless 截图验收。
- Release 工作区构建（editor 与 component-playground）通过。
- 帧 trace 报告工具：17 项测试通过。
- `cargo fmt --all -- --check`、两份 vendor patch 反向核对与 `git diff --check` 通过。

## 性能测量方法

保存未修复与正式修复的 release 程序和测试程序，完成所有编译后串行测量；交替 before/after 顺序，共三轮。两个版本的 editor、模型、feature、原生 Metal 补丁相同，只改变缓存边界修复。Kit 依赖仍固定 `507e82b`，Fast 为 0.1.7；没有改变依赖版本。原始日志、二进制、版本/设备信息和运行清单位于本机 `target/gpui-fast-fix/`。

CPU 测试每轮 10 次 warmup +100 次平移，三模型，两个版本均测 retention on/off。首轮保存末帧 quad/sprite 描述，忽略 quad draw-order 编号后比较。原生窗口主矩阵两个版本均开启 retention，每个模型每轮分别 pan、zoom、opacity，各 30 次 warmup +300 次正式输入，目标输入频率 120 Hz，配置目录隔离。另对三个模型分别补测 before/after 参数滑块，各 300 次正式输入。

表格的统计口径为各轮分位数的中位数，不是合并样本分位数。真实窗口延迟从 GPUI 输入入口到对应纹理首次呈现的 Metal `presentedTime`，不含硬件输入、AppKit 投递或屏幕像素响应。必须同时看未呈现请求及真实输入节拍；CPU 降幅不能直接当作端到端延迟降幅。

## CPU 结果

| 模型 | 未修复 retention off | 修复后 retention off | 未修复 retention on | 修复后 retention on | on 耗时下降 |
| --- | ---: | ---: | ---: | ---: | ---: |
| mao | 0.970 ms | 0.925 ms | 1.593 ms | 0.734 ms | 53.9% |
| shirousagi | 0.858 ms | 0.859 ms | 1.497 ms | 0.639 ms | 57.3% |
| heijiao | 0.737 ms | 0.731 ms | 1.502 ms | 0.652 ms | 56.6% |

三轮 P95 中位数。开启 retention 的 CPU 收益稳定复现；关闭 retention 的对照变化较小，符合修复针对自动保留失效传播的预期。三个模型四种组合的末帧 quad/sprite 对照一致（规范化 draw-order；并非所有帧逐像素 oracle）。

## 真实窗口结果

主矩阵三轮 P95 中位数，单位 ms；before/after 均开启 retention。

| 模型 | 负载 | UI draw 修复前 | UI draw 修复后 | 输入→呈现修复前 | 输入→呈现修复后 |
| --- | --- | ---: | ---: | ---: | ---: |
| mao | pan | 3.219 | 2.797 | 39.381 | 39.800 |
| mao | zoom | 3.415 | 2.851 | 48.429 | 48.085 |
| mao | opacity | 2.917 | 2.628 | 39.043 | 34.491 |
| shirousagi | pan | 3.148 | 3.014 | 39.689 | 39.236 |
| shirousagi | zoom | 3.216 | 3.202 | 48.303 | 48.197 |
| shirousagi | opacity | 3.069 | 2.770 | 39.336 | 34.731 |
| heijiao | pan | 3.064 | 2.767 | 40.733 | 48.988 |
| heijiao | zoom | 3.230 | 3.105 | 48.158 | 48.890 |
| heijiao | opacity | 2.928 | 2.254 | 40.649 | 45.926 |

CPU draw 的收益在真实窗口仍存在，但幅度比 headless 小；主矩阵的端到端延迟没有普遍改善。Mao/白兔的 pan、zoom P95 基本持平，opacity 约下降 4.6 ms。黑椒的 pan/opacity 主矩阵出现回退，不能声称这个修复让所有真实交互都更快。

成功呈现与其他结果（三轮合计；zoom 的滚动/惯性分发会产生额外关联记录，不能硬套 900 条）：

| 模型 | 负载 | 修复前 呈现/记录 | 修复后 呈现/记录 | 其他结果（前 → 后） |
| --- | --- | ---: | ---: | --- |
| mao | pan | 899/900 | 900/900 | accepted_not_composed=1 → 无 |
| mao | zoom | 932/932 | 930/930 | 无 → 无 |
| mao | opacity | 887/900 | 900/900 | accepted_not_composed=13 → 无 |
| shirousagi | pan | 888/900 | 886/900 | accepted_not_composed=12 → accepted_not_composed=14 |
| shirousagi | zoom | 931/931 | 932/932 | 无 → 无 |
| shirousagi | opacity | 890/900 | 900/900 | accepted_not_composed=9, rejected=1 → 无 |
| heijiao | pan | 900/900 | 900/900 | 无 → 无 |
| heijiao | zoom | 930/930 | 938/941 | 无 → accepted_not_composed=1, coalesced=1, no_request=1 |
| heijiao | opacity | 891/900 | 870/900 | accepted_not_composed=9 → accepted_not_composed=27, rejected=3 |

黑椒 pan 主矩阵的 P95 分段：

| 阶段 | 修复前 | 修复后 |
| --- | ---: | ---: |
| delivery_ms | 0.034 | 7.838 |
| ui_wait_ms | 3.110 | 2.528 |
| ui_draw_ms | 3.064 | 2.767 |
| drawable_wait_ms | 2.487 | 6.297 |
| preview_gpu_ms | 6.186 | 6.085 |
| compose_gpu_ms | 0.795 | 0.803 |
| input_to_submit_ms | 8.817 | 16.710 |
| submit_to_present_ms | 32.462 | 32.412 |

增加的等待可在 UI 交付和 drawable 获取阶段观察到，预览 GPU、合成 GPU 和提交后呈现时间没有相应增加。它与主线程/显示节拍及背压有关，但这些分段分位数本身不能证明具体调度因果，也不能相加还原总延迟。此次没有修改呈现、缓冲数、GPU 队列或调度策略。

考虑到黑椒的回退与轮次波动，主矩阵完成后追加一组对照，单独列出，未替换前三轮结果：

| 黑椒附加对照 | UI draw 前 → 后 | 输入→呈现前 → 后 |
| --- | ---: | ---: |
| pan | 2.866 → 2.339 | 45.825 → 47.442 |
| zoom | 3.787 → 3.095 | 48.424 → 48.081 |
| opacity | 2.761 → 1.938 | 36.602 → 27.390 |

附加轮的 opacity 方向反转，说明窗口延迟轮次波动明显；它不能消除主矩阵里观察到的回退，也不足以推出固定的端到端提升或退化比例。附加轮 pan 修复前 300/300 呈现，修复后 298/300（2 coalesced）；zoom 两版 310/310；opacity 前 297/300、后 299/300，其他均为 accepted_not_composed。

主矩阵每组的实际输入间隔 P50 中位数约 8.33–8.34 ms；完整 P50/P95/P99、各轮数据、窗口和新预览呈现频率保存在 `summary.json`。硬件为 Apple M4、macOS 27.0.1，主显示器报告 120 Hz，并镜像到另一台报告 75 Hz 的显示器；两版使用同一显示配置。

## 参数滑块补测

每版每模型一轮，300 次正式输入，不能把单轮差异当作稳定收益。黑椒原始 after 采样因窗口失焦中止，保留为失败记录；表内黑椒使用随后完整重跑的 before/after 配对。

| 模型 | UI draw P95 前 → 后 | 输入→呈现 P95 前 → 后 | 呈现/记录 前 → 后 | 其他结果 前 → 后 |
| --- | ---: | ---: | ---: | --- |
| mao | 2.927 → 2.368 | 40.156 → 31.237 | 295/300 → 271/300 | accepted_not_composed=1, no_request=4 → accepted_not_composed=25, no_request=4 |
| shirousagi | 3.122 → 2.978 | 39.702 → 40.252 | 296/300 → 296/300 | no_request=4 → no_request=4 |
| heijiao | 2.951 → 2.355 | 48.968 → 40.098 | 293/300 → 295/300 | accepted_not_composed=2, rejected=1, no_request=4 → rejected=1, no_request=4 |

Mao 滑块的成功呈现样本变快，但 accepted_not_composed 增加到 25/300，因此不能仅凭延迟下降宣称整体体验改善。三个模型均完成了真实控件输入与呈现关联验证。

本轮共 64 次进程采样：36 次 CPU、18 次主矩阵窗口、6 次初始参数窗口、2 次黑椒参数重跑、2 次黑椒附加窗口。63 次完整通过；1 次失焦中止明确排除出性能结论，未覆盖原始日志。所有用于结论的窗口报告完整性检查通过。

## 复跑

```sh
cargo test --locked --release --workspace
cargo test --locked --release -p kasane-editor --bin kasane-editor virtual_list_cached_rows_ignore_render_writes_but_refresh_notified_inputs
GPUI_VIEW_RETENTION=1 KASANE_BENCH_MODEL="$PWD/models/local/mao/runtime/mao_pro.model3.json" cargo test --locked --release -p kasane-editor --bin kasane-editor canvas_ui_cpu_benchmark -- --ignored --nocapture
GPUI_VIEW_RETENTION=1 bash scripts/window-benchmark.sh models/local/mao/runtime/mao_pro.model3.json target/window-cache-fixed 300 120
# 核对当前源码，以下命令不改文件。
git apply --reverse --check vendor/cache-boundary.patch
git apply --reverse --check vendor/native-metal.patch
```

完整交替矩阵由本机 `target/gpui-fast-fix/run.py` 运行，`runs.json` 保存每次命令、环境、退出码和用时；CPU 数据及真实窗口 `summary.json` 由 `summarize.py` 汇总。`target/` 被 Git 忽略，最终报告与正式 patch 可随仓库保存。不要向已存在的输出目录重复运行矩阵，以免覆盖证据。
