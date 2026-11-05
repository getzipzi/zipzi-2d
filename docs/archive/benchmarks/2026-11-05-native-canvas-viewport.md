# 2026-11-05 Viewport UI benchmark

当前 worktree 的原生 Metal overlay 与统一 GPUI 回退使用同一个保存的 Release 二进制比较。
这是当前两条渲染路径的比较；历史重构前版本没有参与本次采样。

Apple M4（10 核 GPU，16 GB），macOS 27.0.1；主屏 3840×2160、逻辑 1920×1080 @120 Hz，镜像连接另一个 60 Hz 显示器。
Release 启用 test-support,bench-instrumentation，正式采样关闭详细计时。渲染请求尺寸在全部 32 个进程中固定为 941×1312 像素。
Mao runtime：260 Mesh、128 参数。重负载选中 150 个可见且未锁定 Mesh，
5159 个 overlay 顶点、12202 条边、2825 条边界线。

每个场景 4 对新进程，AB/BA/AB/BA 交错；每个模式预热 30 次、测量 360 次，目标输入 120 Hz。
正式接纳 32 个进程、25,920 次测量输入；每种模式每条路径 1,440 次。
后台进程每秒采样；接纳的进程均未观察到编译器。8 个被整对排除的进程保留在 attempts.json。
全部接纳 trace 通过原有 frame reporter 完整性检查：benchmark_done、唯一 end、lost_events=0、GPU 状态成功、全部 workload 和测量输入齐全。
两条路径的 Warp/Rotation/Mesh 对象、组件 ID 和受影响顶点数，以及参数与 Mesh 选择逐对一致。

本次最稳定的收益出现在全选 Mesh 后拖参数：4 轮 UI P95 都降低，整体中位数降低 18.0%；dispatch → present P95 从 46.00 ms 降到 39.12 ms，新 preview FPS 从 105.27 提高到 116.00。

轻负载 UI P95 的变化多数在 ±4% 内。Opacity preview 的 UI P95 中位数上升 16.7%，原生路径 4 轮分布为 3.24–4.86 ms，GPUI 为 3.33–3.48 ms；这是需继续定位的波动，不能算作全面性能改善。Mesh 8 点拖动的呈现 P95 从 35.22 ms 增至 42.75 ms，新 preview FPS 从 118.84 降至 105.87。Opacity 也出现呈现延迟分位数下降、但新 preview FPS 下降的组合，需要结合 outcomes 和帧率解释。

下表使用“每个进程的 P95，再取 4 轮中位数”，单位 ms；变化为 Metal/GPUI−1，负值表示耗时降低。完整 P50/P95/P99、每轮数值和范围在 comparison.json。

| 场景 | GPUI UI draw P95 | Metal UI draw P95 | 变化 |
| --- | ---: | ---: | ---: |
| Opacity preview（命令入口） | 3.414 | 3.983 | +16.7% |
| 平移 | 4.823 | 4.711 | -2.3% |
| 缩放 | 3.801 | 3.890 | +2.3% |
| 参数拖动，无选中对象 | 4.507 | 4.486 | -0.5% |
| 参数拖动，选中全部可见 Mesh | 6.641 | 5.448 | -18.0% |
| Mesh 8 点 | 2.699 | 2.796 | +3.6% |
| Mesh 单点 | 4.443 | 4.533 | +2.0% |
| Rotation 旋转柄 | 5.255 | 5.121 | -2.6% |
| Warp9 控制点 | 5.691 | 5.510 | -3.2% |

UI draw 是整个窗口在 UI 线程上的布局/绘制墙钟时间，包含其他面板。

| 场景 | GPUI dispatch → present P95 | Metal dispatch → present P95 | GPUI / Metal 新 preview FPS |
| --- | ---: | ---: | ---: |
| Opacity preview（命令入口） | 40.25 | 35.59 | 119.66 / 109.30 |
| 平移 | 42.26 | 40.80 | 118.01 / 119.50 |
| 缩放 | 48.81 | 47.92 | 120.00 / 120.00 |
| 参数拖动，无选中对象 | 40.48 | 36.55 | 114.84 / 117.83 |
| 参数拖动，选中全部可见 Mesh | 46.00 | 39.12 | 105.27 / 116.00 |
| Mesh 8 点 | 35.22 | 42.75 | 118.84 / 105.87 |
| Mesh 单点 | 40.53 | 38.77 | 116.31 / 109.96 |
| Rotation 旋转柄 | 40.51 | 40.79 | 118.01 / 118.01 |
| Warp9 控制点 | 46.25 | 45.38 | 108.88 / 116.55 |

输入起点是 GPUI 合成事件分发，终点是 compositor presentation 时间戳；不包含 OS 输入投递或显示面板响应。
FPS 使用实际带关联 preview 的 presentation 时间戳。没有产生新 preview 的输入、合并、取消和未呈现请求保留在 outcomes，延迟分布只使用具有相应边界的请求；不将缺失延迟当作零。
Preview GPU 与 GPUI composition GPU 的范围分别记录；原生 overlay 使用独立命令缓冲，另行做详细诊断。

参数审计：Angle_X，影响 93 个 Mesh，authoring history 不变。
变形审计：`[{"mode": 5, "object": "dab3e07e-c588-4d0b-a8ff-de896174d04d", "component_ids": [1], "affected_vertices": 158}, {"mode": 6, "object": "dab3e07e-c588-4d0b-a8ff-de896174d04d", "component_ids": [1, 2, 3, 4, 5, 6, 7, 8], "affected_vertices": 158}, {"mode": 7, "object": "84128150-2a83-4895-9771-4a7676d9b71b", "component_ids": [2147483648], "affected_vertices": 4933}, {"mode": 8, "object": "f7039c46-e890-4f08-be96-9890ce222bd9", "component_ids": [0], "affected_vertices": 358}]`。

未接纳的初始运行：pilot 有后台编译；primary 的旧 Warp57（5101be9b…）在正式输入约第 81 次遇到父级逆映射失败；Warp71 无可达编辑点；Warp9 完整探针通过但有后台编译。所有失败原始日志均保留在相邻 target/viewport-ui-20261105-* 目录。

二进制 SHA-256：`fd9d602f3cd4ee7f7038b4a8fe7b7265205f513418b926736dec5c40911802d5`。源 HEAD：`0ec23705ab9c8d59c88c2fc9d5a7f62885a6fde2`，包含未提交修改；源 diff 和新增源文件已归档。
当前开发 Editor 保持运行；测量进程使用私有 Runtime/config，MTL HUD 关闭。后台非编译任务与调度波动未被完全排除，原始进程快照可查。

复现入口：tools/benchmarks/viewport-ui-benchmark.py；本次参数见 run.json。原始证据保存在 `/Users/liangbohao/web/zipzi-2d-warp-viewport/target/viewport-ui-20261105-accepted`：results.json、attempts.json、每个目录下的 trace.jsonl、summary.json、application.log 和 background.json。

逐轮 UI draw P95 范围（ms）：

| 场景 | GPUI min–max | Metal min–max |
| --- | ---: | ---: |
| Opacity preview | 3.330–3.480 | 3.241–4.857 |
| 平移 | 4.810–6.459 | 4.492–4.961 |
| 缩放 | 3.766–3.879 | 3.866–4.050 |
| 参数：无选中对象 | 4.437–5.189 | 4.421–4.752 |
| 参数：全选 Mesh | 6.326–6.816 | 5.366–5.590 |
| Mesh 8 点 | 2.680–3.027 | 2.669–3.232 |
| Mesh 单点 | 4.355–4.520 | 4.277–5.167 |
| Rotation | 5.131–5.477 | 5.027–5.214 |
| Warp9 | 5.553–5.853 | 5.357–6.110 |

详细诊断另跑 2 对 AB/BA，仍为同一个二进制和全选参数 fixture；以下数据不混入上面的正式 UI 指标。

| CPU 阶段 | GPUI P50 / P95（ms） | Metal P50 / P95（ms） |
| --- | ---: | ---: |
| 几何绘制提交 | 0.662 / 0.754 | 0.179 / 0.268 |

原生 overlay 独立命令 GPU 时间 P50/P95 中位数为 0.930/3.759 ms，共 689 个测量区间内命令，均有完整成功时间戳。GPUI overlay 的 GPU 工作合在 composition 中，没有可直接比较的独立值；不对不同样本的分位数求和。
原生路径每次更新最多 22,746 个实例、1,455,744 字节上传、1 次 instanced draw。缓存命中帧不重新提交 overlay。

详细诊断原始数据：`/Users/liangbohao/web/zipzi-2d-warp-viewport/target/viewport-ui-20261105-detail`。工具相关 6 个测试通过；git diff --check 通过。二进制和全部模型资源在测量前后的 SHA-256 一致，验证结果保存在 verification.json。
