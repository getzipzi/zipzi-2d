# 大模型 modified() 判定基准

2026-09-28，Apple M4 / 16 GB。保留优化前基线与优化后复测。

## 优化后复测

已缓存文档与保存快照的比较结果，也缓存 pending 编辑与原网格的比较结果。有效内容修改会清除文档缓存，保存直接更新为干净状态；初始化、批量构建、候选提交、checkpoint 恢复及撤销/重做均覆盖失效处理。编辑预览写入会清除 pending 缓存。

前后文档菜单改用 `document_count()`，不再为了数量构建文档摘要。

使用相同模型、相同 benchmark、相同采样数量复测：

| 场景 | 优化前 Release P50 | 优化后 Release P95 |
| --- | ---: | ---: |
| 保存后的未修改判定 | 0.098 ms | <0.001 ms |
| 正在编辑的修改判定 | 0.100 ms | <0.001 ms |
| 已提交修改的判定 | 0.022 ms | <0.001 ms |
| Tab + Previous + Next 组合查询 | 0.278 ms | <0.001 ms |

组合查询的实测 Release P50/P95/P99 为 83/84/125 ns，Debug 为 208/209/250 ns。单次缓存读取已接近约 42 ns 的计时粒度，部分样本为 0 ns，因此只作“低于 1 微秒”的结论，不宣称实际耗时为零。

这些是预热后的**缓存命中**结果。内容变更后首次查询仍可能做原来的完整比较，保留“改回保存内容即清除圆点”的语义；没有把完整比较本身变为常数时间。

复测原始数据：

- [Release 第 1 次](benchmarks/2026-09-28/modified/cached/mao-release-1.csv)
- [Release 第 2 次](benchmarks/2026-09-28/modified/cached/mao-release-2.csv)
- [Release 第 3 次](benchmarks/2026-09-28/modified/cached/mao-release-3.csv)
- [Debug](benchmarks/2026-09-28/modified/cached/mao-debug-1.csv)

Core/SDK 111 项与编辑器 workspace 69 项测试通过。新增回归覆盖同 revision 的批量修改，以及名称/透明度/顶点预览改回原值、取消编辑、保存、撤销和重做。Release 应用已重新打包。

## 优化前结论

本地较大的 Mao 模型，保存后单次 `Document::modified()` 的 Release 中位耗时约 **0.10 ms**，P95 约 **0.20–0.24 ms**。一次调用约占 120 Hz 的 8.33 ms 帧预算的 **1.2%**（按中位数）。

当前 Tab 查询以及菜单的 Previous / Next 可用状态都调用 `Editor::documents()`。这三个 API 的组合查询，在单个干净文档下中位 **0.278 ms**、P95 **0.345 ms**，约占帧预算 **3.3% / 4.1%**。这是独立组合查询基准，未测量完整 GPUI 帧，也不能说明每一帧必然执行这些查询。

因此：**单次比较不算重，但重复判定有可避免的 CPU 开销；不能按 O(1) 状态读取看待。** 多文档会执行更多比较。是否按 revision 缓存应结合这个量级判断，此处为优化前的判断，实际优化及复测见上节。

## 优化前结果

单位为毫秒；Release 为 3 个独立进程各 1,000 次，合计每场景 3,000 个样本；Debug 每场景 1,000 次。每个场景先预热 100 次。分位数使用 nearest rank，不过滤异常值。

| 场景 | Release P50 | Release P95 | Release P99 | Debug P50 |
| --- | ---: | ---: | ---: | ---: |
| 导入、尚无保存快照 | <0.001 | <0.001 | <0.001 | <0.001 |
| 刚保存、未修改 | 0.098 | 0.243 | 0.537 | 0.612 |
| 重新打开、未修改 | 0.100 | 0.205 | 0.459 | 0.598 |
| 编辑预览、值未改变 | 0.099 | 0.148 | 0.283 | 0.599 |
| 编辑预览、值已改变 | 0.100 | 0.252 | 0.760 | 0.662 |
| 已提交的名称修改 | 0.022 | 0.027 | 0.068 | 0.098 |
| 撤销回保存状态 | 0.095 | 0.169 | 0.355 | 0.732 |
| Tab + Previous + Next 组合查询 | 0.278 | 0.344 | 0.480 | 1.854 |

“编辑预览、值已改变”仍先比较未提交的主文档与保存快照，再比较 pending 网格，因此没有享受主文档提前发现差异的短路。提交后才可能在文档内容比较中较早退出；其位置受到 HashMap 遍历顺序和修改对象影响，0.022 ms 不代表所有 dirty 状态。

没有保存快照的导入模型走快速分支，其多数样本已接近计时器分辨率，不把记录中的 0 ns 解释为实际零耗时。

## 模型和边界

- 使用仓库已有较大模型 `models/local/mao/runtime/mao_pro.model3.json`。
- **260 个网格、6,012 个顶点、7,939 个三角形**，保留模型原有参数、绑定和附件。
- 临时保存后的项目 manifest 为 **3,094,579 字节（3.09 MB）**。源模型资源包约 9.36 MB，包含一张 4096×4096 纹理；比较成本不能直接按纹理文件体积推算。
- 导入后真实保存至临时目录，再重新打开，确保测到 `saved_content` 的内容比较路径。源模型和用户项目未被写入。
- 文件读写、导入、保存、改名、提交和撤销均在计时之外。计时仅包围 dirty 查询；查询结果通过 `black_box` 观察，并逐次断言期望值。
- 同一场景连续调用，属于热缓存微基准；不包含 UI 布局、磁盘 IO、GPU 渲染、每帧其他工作造成的缓存竞争。Release / Debug 依次执行，没有并行运行本次编译任务。
- 个别长尾保留在原始 CSV。Debug 的撤销场景出现一次 114 ms 异常长尾，不能据此把单次纯比较的典型成本定为 114 ms。
- 结论适用于这个模型规模，没有用它代表任意尺寸的项目。

之前的 [120 Hz 交互报告](interaction-performance.md) 中，GPUI 平移测试使用刚导入、尚未保存的模型，走的是此处的快速分支。它不能证明保存后 dirty 比较的成本也已包含在那次测量里。

## 复现与原始数据

```sh
cargo run --locked --release -p editor-core --example modified_benchmark -- \
  models/local/mao/runtime/mao_pro.model3.json 1000 > modified-release.csv

cargo run --locked -p editor-core --example modified_benchmark -- \
  models/local/mao/runtime/mao_pro.model3.json 1000 > modified-debug.csv
```

- [Release 第 1 次](benchmarks/2026-09-28/modified/mao-release-1.csv)
- [Release 第 2 次](benchmarks/2026-09-28/modified/mao-release-2.csv)
- [Release 第 3 次](benchmarks/2026-09-28/modified/mao-release-3.csv)
- [Debug](benchmarks/2026-09-28/modified/mao-debug-1.csv)
- [模型与运行元数据](benchmarks/2026-09-28/modified/mao-release-1.txt)

基准源码：`crates/editor-core/examples/modified_benchmark.rs`。四次运行的全部预期状态断言通过；合计 32,000 个计时样本。
