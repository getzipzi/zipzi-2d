# GPUI Kit 0.7.3 / GPUI Fast 0.1.7 升级与性能验证

> 后续调查已复现上游收益，并定位到 Fast 0.1.7 的虚拟列表缓存边界判定缺陷；见 [复现与原因报告](gpui-fast-reproduction.md)。本报告保留原始升级测量。正式修复与新的窗口对照见 [修复验证](../gpui-fast-cache-boundary-fix.md)。

日期：2026-10-13。GPUI Kit 与 gpui-fps 固定到上游 main 的 [507e82b](https://github.com/longbridge/gpui-kit/commit/507e82b8115ac73af92b7d3856501f9d7b73b2c9)，均启用 `gpui-fast`。原生 Metal / frame trace 的三个本地补丁已迁移到 Fast 0.1.7，原 GPUI 0.3.6 vendor 已替换。补丁边界见 [vendor/README.md](../../vendor/README.md)。

本轮结论：功能与原生渲染回归通过，Mao 动画窗口实测约 120 Hz；视图保留在所测画布负载中没有带来加速，3 轮 UI CPU P95 中位数约为关闭时的 1.78–1.84 倍。两个模型的部分变形负载受可编辑控制点条件限制，未完成的记录单独保留。

## 环境与测量口径

- Apple M4（10 核 GPU），16 GiB RAM；macOS 27.0.1（26A434）；Rust 1.99.0。
- Kasane 依赖：`8d448d09df299bd960183e5e9f6b229ecf7c02e3`；编辑器基线：`dcaac1d78444ad2b6c3c4d577425c2fb3f0d9444` 加本次工作区变更。
- Release 构建，关闭 Metal HUD，暂停开发监听器；各 benchmark 串行执行，测量期间不并行编译。
- 显示器镜像配置：CB27U1 3840×2160 / 1920×1080 @120 Hz，Redmi 27 NQS 镜像 @75 Hz。记录实际呈现时间，不能只用显示器标称刷新率推断性能。
- 窗口交互每负载 300 次正式输入 + 30 次预热，目标 120 Hz；缩放平滑动画可使请求记录多于输入数。
- CPU 参数 384 次、modified 每状态 2000 次、独立 Metal 每负载 300 次；UI CPU 100 次正式样本 + 10 次预热。
- A/B 仅切换同一 Fast 后端的 `GPUI_VIEW_RETENTION=0/1`，滚动层保持默认开启。它不是与旧 Kit 0.6.5 的完整升级前后对比，也不是完全关闭所有 Fast 优化。
- 下文时间单位均为 **ms**，除非明确注明 ns。原始数据中的 CPU、GPU、UI 接收与系统 presentedTime 是不同边界，不能混用。

## 验证

- `cargo test --locked --workspace`：399 passed，0 failed，4 ignored。
- 三个本地模型的 animation acceptance 和完整 headless screenshot 验收通过；原生 Metal 的透明度、遮罩、裁剪、覆盖层、尺寸及缩放验证通过。
- 4 个原有 ignored 项为 CPU benchmark 及 3 个本地模型专项测试；本次另行执行了 CPU benchmark。其余仍保持 ignored。
- `cargo build --locked --release --workspace --all-targets` 与 `cargo check --locked --workspace --all-targets --all-features` 通过（包含 playground、debug-tools）。
- 滑块 benchmark 修正后三模型重测均通过，相关参数交互测试另行重跑 16 项通过。
- Python trace 报告测试 17 项通过；`cargo fmt --all -- --check` 和 `git apply --reverse --check vendor/native-metal.patch` 通过。
- 上游 Fast/macOS 仍有 Cocoa 弃用警告，以及 `block 0.1.6` 的未来 Rust 兼容性警告。

## UI CPU：视图保留对照

以下为交替执行的 3 轮结果，各轮分位数再取中位数，不是把样本合并后的分位数。测量同步输入分发与 GPUI CPU draw，不包括窗口呈现。

| 模型 | 视图保留 | P50 中位数 | P95 中位数 | P99 中位数 |
| --- | --- | --- | --- | --- |
| mao | 关闭 | 0.833 | 0.934 | 0.996 |
| mao | 开启 | 1.428 | 1.664 | 1.746 |
| shirousagi | 关闭 | 0.745 | 0.814 | 0.887 |
| shirousagi | 开启 | 1.233 | 1.494 | 1.632 |
| heijiao | 关闭 | 0.668 | 0.829 | 0.952 |
| heijiao | 开启 | 1.243 | 1.506 | 1.530 |

## 实际窗口：视图保留对照

Mao 的 3 轮交替 A/B。表中同样是各轮 P95 的中位数；两个模式使用相同模型、输入和构建。

| 负载 | 视图保留 | UI draw P95 中位数 | 输入→呈现 P95 中位数 |
| --- | --- | --- | --- |
| pan | 关闭 | 2.782 | 39.676 |
| pan | 开启 | 3.264 | 48.901 |
| zoom | 关闭 | 2.739 | 48.835 |
| zoom | 开启 | 3.416 | 48.135 |
| opacity | 关闭 | 2.292 | 39.296 |
| opacity | 开启 | 2.805 | 45.760 |

开启视图保留在这些画布交互负载中增加了 UI CPU 成本，平移、透明度的呈现延迟也更高；缩放呈现延迟相近。不能据此宣称本次升级带来整体性能提升。按升级要求保留 `gpui-fast` 及其默认视图保留行为，未为改善成绩关闭功能。

## 实际窗口：三模型覆盖

下表为默认 Fast 设置的测量，平移/缩放/透明度取首轮，参数滑块取修正定位后的重测；列出输入→首次呈现 P95。呈现成功之外的记录在下一表保留。

| 模型 | 平移 | 缩放 | 透明度 | 参数滑块 |
| --- | --- | --- | --- | --- |
| mao | 40.685 | 48.474 | 46.587 | 31.635 |
| shirousagi | 40.298 | 47.948 | 40.214 | 40.653 |
| heijiao | 47.502 | 47.660 | 32.800 | 40.727 |

| 模型 | 负载 | 呈现/关联记录 | 其他结果 |
| --- | --- | --- | --- |
| mao | opacity | 254/300 | {'accepted_not_composed': 46} |
| mao | pan | 300/300 | 无 |
| mao | zoom | 311/311 | 无 |
| mao | parameter_slider | 276/300 | {'no_request': 24} |
| shirousagi | opacity | 273/300 | {'accepted_not_composed': 24, 'rejected': 3} |
| shirousagi | pan | 268/300 | {'accepted_not_composed': 32} |
| shirousagi | zoom | 312/314 | {'no_request': 2} |
| shirousagi | parameter_slider | 295/300 | {'accepted_not_composed': 1, 'no_request': 4} |
| heijiao | opacity | 300/300 | 无 |
| heijiao | pan | 300/300 | 无 |
| heijiao | zoom | 310/310 | 无 |
| heijiao | parameter_slider | 285/300 | {'accepted_not_composed': 11, 'no_request': 4} |

`accepted_not_composed` 表示 UI 已接收，但在进入合成前被更新纹理替代；`no_request` 表示该输入未产生预览请求。只看成功呈现样本的延迟会漏掉这些情况。

## 变形交互

Mao 覆盖全部四种负载：

| 负载 | 场景 P95 | UI draw P95 | 输入→呈现 P95 | 结果 |
| --- | --- | --- | --- | --- |
| mesh_multi_vertex | 0.743 | 2.884 | 34.442 | {'presented': 300} |
| mesh_single_vertex | 0.899 | 2.986 | 46.470 | {'presented': 300} |
| rotation_deformer | 0.977 | 3.007 | 45.892 | {'presented': 261, 'accepted_not_composed': 39} |
| warp_deformer | 0.961 | 2.743 | 46.564 | {'presented': 249, 'accepted_not_composed': 51} |

Shirousagi 完成单顶点、多顶点、Warp 后，因没有可用 Rotation 控制而中止。黑椒完成单顶点后，未找到具备 8 个可达顶点的编辑目标。这两次报告按设计返回非零，保留为不完整测量；不把缺失负载视为通过，也不更改模型制造成绩。

## CPU 与独立 Metal

CPU 参数设置与场景求值 P95，以及已保存未修改文档的 dirty 查询 P95：

| 模型 | 参数设置 | 场景求值 | dirty 查询（ns） |
| --- | --- | --- | --- |
| mao | 0.268 | 0.137 | 42.000 |
| shirousagi | 0.029 | 0.019 | 42.000 |
| heijiao | 0.053 | 0.037 | 42.000 |

独立 Metal：1280×800，5 次预热后每模式 300 次。`complete_ms` 包含 CPU 提交与等待 GPU 完成；`gpu_ms` 只统计 GPU 执行，不是窗口延迟。

| 模型 | 负载 | CPU 提交 P95 | GPU P95 | 完成 P95 |
| --- | --- | --- | --- | --- |
| mao | pan | 0.113 | 4.527 | 5.668 |
| mao | zoom | 0.477 | 6.308 | 7.679 |
| mao | mesh_edit | 0.694 | 4.533 | 5.953 |
| mao | deform_all | 1.000 | 5.817 | 6.788 |
| mao | opacity_edit | 0.867 | 4.046 | 6.023 |
| shirousagi | pan | 0.049 | 0.366 | 0.975 |
| shirousagi | zoom | 0.055 | 3.265 | 4.490 |
| shirousagi | mesh_edit | 0.057 | 0.192 | 0.921 |
| shirousagi | deform_all | 0.333 | 0.747 | 2.298 |
| shirousagi | opacity_edit | 0.558 | 4.139 | 5.818 |
| heijiao | pan | 1.183 | 7.231 | 8.742 |
| heijiao | zoom | 1.051 | 6.810 | 8.596 |
| heijiao | mesh_edit | 1.136 | 6.667 | 8.294 |
| heijiao | deform_all | 1.207 | 6.505 | 8.208 |
| heijiao | opacity_edit | 1.253 | 7.035 | 8.896 |

工作区导入和双文档切换也已执行（CPU 5 轮，GPU 100 次），各阶段分位数见原始汇总 JSON。


## 网格覆盖层

5 档网格（8、32、64、128、256），2 档分辨率（1280×800、2560×1600），视图/变形、带/不带控制点、3 种方法，共 120 组 ×100 正式样本。160 次像素 QA 最大 RGB MAE 为 0.002185（0–255 标度），差值大于 4 的像素比例不超过 0.0125%。

最大 256 网格、197120 条边、变形且显示控制点的 P95：

| 分辨率 | 方法 | CPU 提交 | GPU 执行并集 | 完成 |
| --- | --- | --- | --- | --- |
| 1280x800 | gpui_paths | 76.383 | 16.582 | 84.905 |
| 1280x800 | metal_full_overlay | 0.145 | 7.711 | 8.405 |
| 1280x800 | metal_texture | 16.975 | 7.998 | 25.595 |
| 2560x1600 | gpui_paths | 63.815 | 22.157 | 81.522 |
| 2560x1600 | metal_full_overlay | 0.112 | 7.594 | 8.327 |
| 2560x1600 | metal_texture | 10.171 | 7.971 | 17.689 |

完整 Metal 覆盖层显著低于 GPUI 路径方案，但最大负载仍有超过 8.33 ms 的尾部样本；这不是稳定 120 FPS 的保证。

## 动画

三个模型的动画功能验收通过；只有 Mao 提供本轮可播放 motion，因此持续播放性能数据来自 Mao。

| 场景 | 请求 | 接收 | 呈现 | 接收 Hz | 呈现 Hz | 求值 P95 | GPU P95 | 请求→接收 P95 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| animation-headless | 600 | 599 | 0 | 59.968 | — | 1.026 | 2.504 | 4.302 |
| mao-animation-window | 1203 | 1203 | 1203 | 119.968 | 119.999 | 1.026 | 5.539 | 4.853 |

无窗口测试 600 次采样，没有系统呈现数据；窗口测试参数 600 对应约 10 秒，实际节拍由显示回调驱动，得到 1203 次请求。两者 trace 丢失事件均为 0，CPU/render mailbox 高水位均为 1。

## 重现与本机原始数据

[运行清单](../../target/gpui-fast-upgrade/benchmarks/runs.json) 记录命令、环境覆盖、退出码和用时；[微基准汇总](../../target/gpui-fast-upgrade/benchmarks/microbench-summary.json) 保存 CPU/Metal 分位数；各子目录保留 `trace.jsonl`、`summary.json`、`requests.csv`、环境与日志。`target/` 被 Git 忽略，报告内汇总可随仓库保存，原始数据仅在本机。

```sh
# 全矩阵命令参见本机 run-benchmarks.py 和 runs.json
python3 target/gpui-fast-upgrade/run-benchmarks.py target/benchmark-new-run
# 示例：真实窗口平移、缩放、透明度
bash scripts/window-benchmark.sh models/local/mao/runtime/mao_pro.model3.json target/window-fast 300 120
# 只关闭视图保留做对照，仍使用 Fast 后端
GPUI_VIEW_RETENTION=0 bash scripts/window-benchmark.sh models/local/mao/runtime/mao_pro.model3.json target/window-no-retention 300 120
# 参数控件
KASANE_BENCH_PARAMETERS=1 bash scripts/window-benchmark.sh models/local/Shirousagi/Shirousagi.model3.json target/parameter-fast 300 120
```

本次修正了 benchmark 的滑块选择：固定 25% 位置会命中关键帧标记，只选值而不开始拖动。现在选取实际手柄位置，并同时检查当前展开的行及滚动视口，过滤虚拟化行保留的非可见测量边界。最初及中间排查的失败日志保留；三个模型均重测参数控件，报告采用 `*-parameter-thumb` 结果。
