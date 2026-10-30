# Python 生成代码精简：实施与验收

2026-10-30。比较基线：`8934039c`，即上一轮审查修复完成后的版本。
结论：实现、类型与编解码语义、测试、性能和本机分发验收通过。

## 实施范围

1. codec 聚合入口从重导出 804 个函数，改为导出 21 个领域模块。手写调用方、
   backend/Plan/DTO 生成器及测试直接使用对应领域的 codec。模块仍在导入阶段
   初始化：初版延迟加载使首次 canonical JSON 编码额外耗时约 1.3 ms，最终版本
   保留领域预加载，消除了这项延迟。
2. 清理私有 value、codec 和 binary 领域模块的 `Name as Name` 导入；保留公开
   modeling 入口和必要的类型重导出。公开 modeling 模块与基线逐字节一致。
3. 将 1,104 处基础值检查改为有类型的 helper 调用，保留显式领域校验与字段遍历。
   helper 从同一生成策略产生；str/float 的常见输入使用精确类型快路径，子类及其他
   输入使用原有判定。bool/int 区分、整数范围、f32/f64 有限值和 f32 范围均保留。
   输入值不被转换，异常类型与消息保持一致，包括超大整数的既有 OverflowError。

## 实际体积

统计客户端中带生成标记的 `.py`，含公开导出模块；行数包含空行。

| 范围 | 基线 | 实施后 | 减少 |
| --- | ---: | ---: | ---: |
| 不含 catalog 的生成 Python | 1,990,044 B / 31,254 行 | 1,808,368 B / 29,499 行 | **181,676 B（9.13%）/ 1,755 行** |
| 全部生成 Python | 3,867,637 B / 51,302 行 | 3,685,961 B / 49,547 行 | 181,676 B（4.70%）/ 1,755 行 |
| 全部客户端 Python，含手写部分 | 4,088,174 B / 55,357 行 | 3,906,816 B / 53,607 行 | 181,358 B（4.44%）/ 1,750 行 |

生成文件数不变。不含 catalog 的 Python 相较最初 `89b6e2f3` 的体积增幅，
从 14.75% 降到约 4.27%。剩余领域门面和类型导出仍占空间，属于后续可独立评估的范围。

## 语义验证

去掉函数内 import 后与基线比较 AST：536 个 JSON read/encode 函数、341 个 binary
函数完全一致；733 个值类的 AST 完全一致。公开 modeling 模块逐字节一致。
新增边界测试覆盖全部整数宽度、数值子类、bool、NaN/Infinity、f32 边界、非转换性
以及异常类别。原有 Local/Connected 用例继续对照 Rust/native 的实际行为及 wire。

## 性能对照

macOS arm64、CPython 3.14.7，同机交替运行基线和最终客户端各 3 个进程；每进程
9 轮，取各进程中位数后再取中位数。调用通过 `./tools/dev-python` 启动，native
准备/构建时间不计入。两个版本均使用普通源码目录查找，避免 editable hook 差异。

| 测量 | 基线 | 最终 |
| --- | ---: | ---: |
| 普通重命名 binary 编码 | 2.277 µs | 2.282 µs |
| Parameter 校验 | 0.306 µs | 0.273 µs |
| 3 点 keyform 校验 | 1.617 µs | 1.256 µs |
| 3 点 keyform 校验 + binary 编码 | 17.297 µs | 17.346 µs |
| 1,000 点 keyform 校验 | 184.304 µs | 99.309 µs |
| 1,000 点 keyform 校验 + binary 编码 | 432.278 µs | 346.740 µs |
| 1,000 点 keyform JSON 编码 | 74.771 µs | 75.120 µs |
| 1,000 点 keyform JSON 读取 | 372.590 µs | 374.031 µs |

JSON 读写各场景的中位数变化为 +0.4% 至 +3.4%；普通重命名 binary 为 +0.2%，
处于各进程的观测波动内。Parameter 校验约快 10.9%，大 keyform 校验约快 46.1%，
校验加 binary 编码约快 19.8%。两种大小的 JSON/binary 输出指纹在所有进程中一致。

冷进程导入的样本范围：基线 138.5–173.9 ms，最终 137.9–161.3 ms；首次 canonical
JSON 编码为 24.6–36.5 µs 与 29.0–36.7 µs，首次校验加 binary 编码约 1.1–2.5 ms。
这些是本机观测，未将冷启动波动解释成稳定性能收益。

可复用的探针为 `tools/python/python2_codegen_bench.py`：

```sh
./tools/dev-python tools/python/python2_codegen_bench.py \
  --source /absolute/path/to/baseline/client/python \
  --output target/python2/codegen-slimming/baseline.json
./tools/dev-python tools/python/python2_codegen_bench.py \
  --output target/python2/codegen-slimming/current.json
```

原始样本及汇总位于 `target/python2/codegen-slimming/accepted-*.json`、
`performance.json`、`semantic-and-size.json`。

## 测试与分发

| 检查 | 结果 |
| --- | --- |
| `test_python2_codegen*.py`，含类型图、产物管理及隔离 checkout | 47 项通过 |
| `./tools/test-sdk.sh python2` | 104 项行为、16 项恢复及正反例静态类型通过 |
| `./tools/test-sdk.sh python2 --discovery` | 9 项及静态类型通过 |
| `./tools/test-sdk.sh python2-connected` | 20 项、纯 wheel 离线审计通过 |
| schema / distribution 工具测试 | 4 / 7 项通过 |
| 无缓存生成检查、schema 重放 | 通过，345 个产物 |
| 源归档独立生成及重放、双 wheel、Local/Connected 安装示例 | 通过 |

离线审计覆盖 1,018 个符号、1,901 个 callable、2,437 个字段，反射、文档链接及
指纹失败计数均为零。JSON/binary 性能用例的输出摘要在基线和最终版本间一致。

最终运行结果见 `target/python2/codegen-slimming/` 下的测试日志与
`distribution/results.json`。本轮不涉及 GPU 或 Editor UI 行为验收。
