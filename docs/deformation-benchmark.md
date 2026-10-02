# Mesh / Deformer 窗口 benchmark

沿用 [窗口 benchmark](archive/window-benchmark.md) 与 [Parameters slider](archive/parameter-slider-performance.md) 的真实 GPUI 窗口、异步 Metal 预览及 JSONL 帧追踪，新增四项持续拖动负载。

```sh
scripts/deformation-benchmark.sh \
  models/local/mao/runtime/mao_pro.model3.json \
  target/deformation-mao 300 120
```

参数依次为模型、输出目录、每项正式样本数、目标输入 Hz。支持 `.model3.json` 和 `.kasane`；默认 Release，每项预热 30 次、测量 300 次、目标 120 Hz。输出目录中的 trace 必须不存在。测试会激活窗口，运行时保持窗口可见、有焦点，不要操作测试窗口；结束后自动退出，不保存模型修改。

| 报告负载 | 操作 |
| --- | --- |
| `mesh_single_vertex` | 选择一个 mesh 顶点并持续拖动 |
| `mesh_multi_vertex` | 用 Shift 点击选中同一 mesh 的多个顶点，再一起拖动；默认 8 个 |
| `warp_deformer` | 拖动 Warp 的一个 Bézier 锚点，通过真实编辑流程使控制网格及后代 mesh 形变 |
| `rotation_deformer` | 沿圆弧拖动 Rotation 的轴端点，改变旋转角度并使后代 mesh 形变 |

所有测量移动事件都通过 GPUI 输入分发、画布命中测试和 SDK 编辑/求值，不直接改渲染后的顶点数组。选择对象、选择顶点及松手提交不计入正式拖动样本。每项使用一个连续手势，预热和正式采样共用该手势；结束后松手，再撤销这一次编辑，恢复下一项测试的模型基线。

Mesh / Warp 的位移为水平 ±12、垂直 ±6 logical px 的周期轨迹；Rotation 的角度偏移为 `0.12 + sin(phase) × 0.18` 弧度。相位每次输入增加 0.13，不依赖实际定时器频率。与旧 benchmark 一样，迟到后不会补发积压输入，报告记录实际输入间隔。

## 目标选择与复现

自动选择可见、可编辑且控制点落在画布内的目标。Mesh 优先顶点多的对象，deformer 优先可见后代顶点多的对象，同规模按场景顺序选择。不会创建替代 deformer；模型必须具备可编辑的 Mesh、Warp 和 Rotation。绑定状态不允许编辑、控制点不可达或实际选中点数不足时会明确失败。

可用环境变量调整：

```sh
KASANE_BENCH_VERTICES=16 KASANE_BENCH_WARMUP=60 \
  scripts/deformation-benchmark.sh models/local/mao_pro.kasane target/deformation-16 360 120
```

- `KASANE_BENCH_VERTICES`：多顶点项的精确选中数量，范围 2–10000；不会静默减少。
- `KASANE_BENCH_MESH`、`KASANE_BENCH_WARP`、`KASANE_BENCH_ROTATION`：可选的内部对象 ID，限制各项选择目标。用固定 `.kasane` 项目复现这些 ID；重新导入 model3 可能生成新的 ID。
- `KASANE_BENCH_PROFILE=debug`：Debug 测试。报告仍按既有规则返回非零并标识不适合作为 Release 验收；应检查是否还有其他 issues。

`application.log` 的 `benchmark fixture` JSON 记录实际对象 ID、控制点 ID 和目标 mesh / deformer 子树的可见顶点总数。单点和多点项各自选择可满足条件的 mesh；比较性能时确认它们使用同一个对象。跨版本比较也应保持模型、目标对象、控制点数量、窗口尺寸、显示器和追踪开关一致。

## 输出和有效性

输出与旧脚本一致：`trace.jsonl`、`requests.csv`、`summary.json`、`environment.txt`、`build.log`、`application.log`。`summary.json` 分别报告四项的阶段耗时和输入到显示的 P50/P95/P99、实际输入间隔，以及合并、拒绝、未显示等请求结果。元数据中的 `workloads` 声明预期负载；报告继续兼容旧的 pan / zoom / opacity 和 parameter slider trace。

每次拖动后检查手势、待提交编辑和模型求值计数；空拖动、编辑失败、失焦、未完成、缺项、丢失 trace 或没有关联显示时间都会使测试失败。fixture 选择和恢复等待有超时限制。准备/恢复产生的帧归入 `manual`，不混入四项测量。

这是 GPUI 输入入口到系统报告显示时间的测试，不包含硬件/OS 输入送达和屏幕扫描。目标输入 120 Hz 不等于实际达到 120 fps；判断性能时同时看实际输入节奏和未显示请求比例。

原有入口保持可用：

```sh
scripts/window-benchmark.sh MODEL OUTPUT_DIR 300 120
KASANE_BENCH_PARAMETERS=1 scripts/window-benchmark.sh MODEL OUTPUT_DIR 360 120
# 新脚本等价于：
KASANE_BENCH_DEFORMATIONS=1 scripts/window-benchmark.sh MODEL OUTPUT_DIR 300 120
```

Parameters 与 deformations 两个开关不能同时为 1。

## Debug 性能定位与优化（2026-10-03）

初始 Debug 结果只有约 18–20 fps。采样和 CPU profile 显示两条重复路径：控制点拖动会让 Parameters 面板反复重建完整 `ObjectIndex`，而每次场景快照会为所有 Warp 控制器生成完整 Bézier 曲线，即使这些控制器没有被选中。

当前实现做了三项局部优化：

- deformer 姿态、网格位置和 Warp 曲线点的更新标记为 `Positions`，复用 SDK 的 prepared evaluation；只有拓扑、绑定轴、key 坐标或关系变化才重建结构缓存。
- ObjectIndex 的关系图和绑定索引在控制点拖动期间复用，只更新当前 deformer 的属性 JSON。
- Bézier authoring geometry 延迟到命中测试或选中控制器真正需要时生成。

同一 Mao 模型的 Debug 对照（30 次预热、300 次正式样本）如下：

| 指标 | 优化前 | 优化后 |
| --- | ---: | ---: |
| Mesh 场景生成 P95 | 15.6–15.7 ms | 3.87–3.90 ms |
| Deformer 场景生成 P95 | 21.2 ms | 3.90–3.97 ms |
| UI 绘制 P95 | 34.4–35.2 ms | 2.63–3.06 ms |
| 实际预览频率 | 17.9–20.3 fps | 73.0–73.4 fps |

优化后的完整 trace 在 `target/deformation-mao-debug-optimized-final/`。当前机器显示器为 75 Hz，因此这次只能确认接近显示器刷新率，不能据此宣称达到 120 fps。Debug 报告仍保留既有的“不适合 Release 验收”标记。
优化后输入恢复到接近 120 Hz，75 Hz 显示器会丢弃部分尚未合成的中间帧；这些记录保留在 `accepted_not_composed` 中，不代表编辑请求失败。

## 本次验证（2026-10-03）

Mao 实际窗口 Release：每项预热 30 次、正式输入 300 次、目标 120 Hz。四项均完成，报告 `issues=[]`、trace 丢失为 0。当前显示器为 1920×1080 logical / 3840×2160 physical、75 Hz，因此本次不能用于确认 120 fps。

| 负载 | 正式输入 | 关联显示 | 输入到显示 P95 |
| --- | ---: | ---: | ---: |
| 单顶点 | 300 | 299 | 58.04 ms |
| 8 顶点 | 300 | 299 | 51.74 ms |
| Warp | 300 | 298 | 36.34 ms |
| Rotation | 300 | 290 | 49.52 ms |

其余 14 个请求为 `accepted_not_composed`，已保留在分母中。本地原始数据在 `target/deformation-mao-release/`；这里只记录一次流程验证，不作为跨版本性能结论。

11 项报告脚本测试、Release 构建、格式与 shell 语法检查通过。编辑器测试为 51 通过、6 失败、1 忽略；将修改的 Rust 源码恢复到 HEAD 后重跑，得到相同的 6 项失败（布局 2 项、参数面板 4 项），属于现有测试问题。
