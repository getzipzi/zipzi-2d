# Mesh overlay controlled run

2026-09-28，Apple M4 / 16 GB，macOS 27.0，Release。

运行：`./scripts/mesh-overlay-benchmark.sh target/mesh-overlay-controlled-30 30`。
两个分辨率 × 五个网格规模 × 两种负载 × 标记开关 × 两种实现；每组 10 次预热、30 次正式采样，交替 AB/BA。

`isolation.json` 记录本轮监测：采样期间暂停了已有 Dev 应用，每秒检查 cargo/rustc，未检测到并行编译；退出后已恢复 Dev。该检查不排除系统调度、GPU 动态频率和其他系统活动。

`source-sha256.json` 记录构建前关键源码，采样完成后再次核对一致。`environment.txt` 包含基线提交和当时工作区状态，其中 editor-core/canvas 的并行修改不属于本 benchmark 交付。

`samples.jsonl.gz` 保存全部 3,200 条原始记录，包含 800 条预热。`summary.json` 的分位数使用 nearest-rank；30 个正式样本的 P99 等于最大值，不宜视为稳定的尾延迟估计。无异常值剔除。

`quality.json` 保存 40 组像素误差；80 张原始 PNG 留在本地 `target/mesh-overlay-controlled-30/`，校验值见 `png-sha256.json`。不将测试图全部纳入版本库。

此前 `target/mesh-overlay-large-100`（中断）和 `target/mesh-overlay-final-30`（有后台编译）均未用于报告结论。

方法、限制与结果见 [benchmark 文档](../../../mesh-overlay-benchmark.md)。
