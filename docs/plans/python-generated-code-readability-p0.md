# Python 生成代码可读性：P0 基线

2026-10-30，工作区在计划基线上的本机单次测量；这些值不是性能门槛。
原始统计和计时保存在 `target/python2/codegen-readability/`，不纳入版本控制。

- 统计范围包含 schema、生成的 Python/Rust 和 reference：15,028,411 字节；最大文件 7,251,113 字节、145,072 行。
- modeling 有 733 个类、1,157 次原值回写；显式 values 领域模块归属覆盖率为 0/733。覆盖率仅统计该目录中声明的类，并非推测的语义归属。
- catalog 的 `summary` 和 `__repr__` 各有 851 条，相同说明各重复 850 次。
- 基于 `./tools/dev-python` 的新进程内测量：import 0.167 秒，首次 `describe("zipzi")` 0.00240 秒，100 次重复查询平均 0.000000751 秒，进程峰值 RSS 45,924,352 字节。适配器准备和编译不计入这些调用。
- `generate.py --check --no-cache` 全进程 3.89 秒、峰值 RSS 317,390,848 字节；预热缓存后的 `--check` 全进程 0.74 秒、峰值 RSS 140,804,096 字节。两者都包含契约导出，不包含首次编译准备。

后续对照需在相同环境和负载重复测量，不能将单次样本解释为显著性能变化。

P0 验证：`test_python2_codegen.py -k Python2Codegen`（26 项）、
`test_python2_codegen_outputs.py`（5 项）以及 `generate.py --check --no-cache`
通过；生成的 40 个既有产物无内容变更。
