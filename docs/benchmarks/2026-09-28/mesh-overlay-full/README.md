# Mesh overlay: three-method controlled run

2026-09-28，Apple M4 / 16 GB，macOS 27.0，Release。

运行：`./scripts/mesh-overlay-benchmark.sh target/mesh-overlay-full-controlled-30 30`。

- 2 个分辨率 × 5 个网格规模 × 2 种负载 × 标记开关 × 3 种实现，每组 10 次预热、30 次正式采样。
- `gpui_paths`：GPUI 合批边线与 GPUI 顶点标记。
- `metal_texture`：Metal 边线纹理与 GPUI 顶点标记。
- `metal_full_overlay`：Metal 边线与标记绘制到同一纹理，再由 GPUI 合成。
- 使用六种排列循环，30 次正式采样中每种方法在三个执行位置分别出现 10 次。
- 选中图案固定，每 37 个顶点选中一个；悬停索引随帧变化，普通/悬停尺寸为 6×6 / 8×8 逻辑像素。

`isolation.json` 记录暂停 Dev/监听并在退出时恢复；采样期间每秒检查 cargo/rustc，276 次检查未检测到并行编译。该检查不排除系统调度、GPU 动态频率和其他系统活动。

`samples.jsonl.gz` 保存全部 4,800 条记录，包括 1,200 条预热；有效样本 3,600 条。`summary.json` 为 nearest-rank 分位数，没有删除异常值；30 个样本的 P99 等于最大值。

`quality.json` 保存 160 组像素比较：每个配置在第 0、17 帧，将两种候选分别对照 GPUI。320 张 PNG 保留在本地 `target/mesh-overlay-full-controlled-30/`，校验值见 `png-sha256.json`。

`source-sha256.json` 是构建前关键源码校验值，结束后已核对一致；`sources.json.gz` 保存对应源码文本快照。环境及基线提交见 `environment.txt`；其中 editor-core/canvas 的并行改动不属于本次 benchmark 工作。

验证：3 个 example 单元测试、32 组小规模像素对照、160 组正式像素对照通过；4,800 条原始记录的组数、执行顺序、时间戳区间与源码校验通过。完成格式检查。

详细方法、结果和限制见 [benchmark 文档](../../../mesh-overlay-benchmark.md)。旧版两组数据在相邻 `mesh-overlay/` 中单独保留，不混合统计。
