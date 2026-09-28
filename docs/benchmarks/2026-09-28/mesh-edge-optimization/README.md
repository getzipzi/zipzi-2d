# 边线优化最终对照数据

四组最终采样，每组 3 种操作 × 300 次正式输入，共 **3,600 次**；各操作另有 30 次预热。全部报告完整性检查通过，原始追踪丢失数为 0。默认组未设置任何绘制诊断变量，trace 确认批量绘制已自动启用；基准组仅设置 `KASANE_TRACE_BATCH_MESH_EDGES=0`。三缓冲、同步、直接绘制在所有组一致。

表内数值为旧逐边路径 → 当前默认路径。各阶段分位数不可相加，显示延迟只覆盖显示成功的请求。

| 模型 / 操作 | GPUI GPU P50 ms | GPUI GPU P95 ms | 输入到显示 P95 ms | 显示请求数 |
| --- | ---: | ---: | ---: | ---: |
| Mao / 平移 | 7.65 → 1.14 | 8.20 → 5.21 | 56.42 → 39.89 | 300 → 297 / 300 |
| Mao / 缩放 | 7.40 → 1.15 | 8.10 → 5.30 | 55.28 → 40.69 | 300 → 288 / 300 |
| Mao / 透明度 | 7.60 → 1.16 | 8.74 → 3.01 | 56.64 → 35.69 | 278 → 300 / 300 |
| Shirousagi / 平移 | 5.99 → 1.17 | 6.60 → 5.64 | 48.79 → 40.13 | 300 → 291 / 300 |
| Shirousagi / 缩放 | 5.93 → 1.14 | 6.64 → 5.43 | 39.82 → 39.62 | 299 → 297 / 300 |
| Shirousagi / 透明度 | 5.61 → 1.15 | 6.54 → 5.52 | 40.59 → 38.78 | 297 → 288 / 300 |

- [Mao 基准](mao-baseline/summary.json) / [Mao 默认](mao-default/summary.json)
- [Shirousagi 基准](shirousagi-baseline/summary.json) / [Shirousagi 默认](shirousagi-default/summary.json)
- [完整指标、实际输入频率与路径计数](comparison.json)
- [实现与验证说明](../../../mesh-edge-optimization.md)

最终四组子目录包含摘要、所有请求 CSV、环境、原始 JSONL gzip 和解压后追踪的 SHA-256。`source.sha256` 标识最终四组使用的相关源码文件。

`before-input-isolation/` 保留采样器输入隔离修正前五轮探索的摘要、环境和校验值：Mao 两组与 Shirousagi 基准通过完整性检查；Shirousagi 两次默认组平移均为 300 个 `no_request`，摘要明确标为不完整。此目录五轮均未计入上表，最终四组已使用同一版修正后的采样器重新测量，不能用旧失败轮次中缺少平移请求的数据宣称提速。

这五轮探索的原始 trace gzip 和逐请求 CSV 位于本地 `target/benchmark-archives/2026-09-28/mesh-edge-optimization/before-input-isolation/`，不随 Git 提交。失败摘要仍入库，用于说明采样器修正的原因。
