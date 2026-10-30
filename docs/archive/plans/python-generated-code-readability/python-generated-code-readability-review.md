# Python 生成代码可读性：审查与验收

2026-10-30。审查范围：`89b6e2f3` 至 `484a46e1`（含 P0–P5），以及本次工作区修复。
结论：本机验收通过。

## 已修复的问题

1. **公开投影模型与产物不一致。** 原模型将 311 个 Rust 类型的领域归属直接当作
   Python 实现路径，其中 45 个路径并不存在。类型图现在只负责契约及领域依赖；
   公开投影来自实际生成的 class、enum 和 union 声明。variant 保留真实 Rust enum
   身份及独立投影 ID，不再伪造 Rust 类型 ID。测试逐一核对实现声明和公开目录路径。
2. **规范路径未参与帮助去重。** catalog 原先按字母顺序选择规范条目，reference
   仍展开别名全文。现在优先采用投影中的规范路径；别名页链接到规范说明，离线查询
   仍保留请求路径、完整字段和对应 children。
3. **帮助覆盖和签名不完整。** 补齐 `mesh_generation` 模块及四个公开条目、设置字段
   的语义说明，以及 `Vec2`、`PreciseVec2` 的字段 docstring。生成的构造签名保留
   keyword-only 标记，枚举默认值使用公开成员名称，不暴露内部 reader/factory 模板。
4. **安装审计存在盲区。** 改为递归遍历实际 `__all__`，检查 dataclass 与 NamedTuple
   字段同时存在于结构化帮助和 docstring。纯 wheel 审计接入 `python2-connected`
   和分发门禁，并修正 Windows 安装路径判断。
5. **输出预检不完整。** 目标是目录、或父路径是普通文件时，原实现可能在部分替换后
   才失败。现在在写入前拒绝这些冲突及非普通文件；新增测试验证原产物和清单不变。
   输出路径及 schema 分片路径同时拒绝 Windows 分隔符和流名称。

## 结构验收

- 原有 2,944 条帮助路径全部保留；新增 5 条，共 2,949 条。
- 733 个 modeling 类和 311 个完整契约类型的领域覆盖率均为 100%。
- 无原值 `setattr`、重复字段说明块或重复的公共继承方法说明。
- reference 导航为 52 行；生成 Python/Rust 代码及领域 reference 最大为 1,452 行，
  没有超过 2,000 行的文件。
- 与 P0 相同统计口径：15,028,411 → 6,395,769 字节，减少约 57.4%。
  schema 索引与领域分片合计 893,864 字节。
- JSON、binary 和 native 转换仍保留显式实现；本次未修改 wire 布局或事务/恢复语义。

### Python 生成文件的独立统计

以 `89b6e2f3` 为改造前基线，统计客户端内所有带生成标记的 `.py`，包括
`_generated/` 和公开领域导出模块；行数包含空行。MB 使用十进制单位。

| 范围 | 改造前 | 改造后 | 变化 |
| --- | ---: | ---: | ---: |
| 全部 Python 生成文件 | 5,444,356 B / 70,142 行 | 3,867,637 B / 51,302 行 | 字节减少 29.0%，行数减少 26.9% |
| 其中 catalog | 3,710,083 B / 42,049 行 | 1,877,593 B / 20,048 行 | 字节减少 49.4%，行数减少 52.3% |
| 排除 catalog 的 Python 生成文件 | 1,734,273 B / 28,093 行 | 1,990,044 B / 31,254 行 | 字节增加 14.7%，行数增加 11.3% |

Python 总计减少 1,576,719 字节、18,840 行；文件数由 30 增至 162，最大单文件
由 42,049 行降至 1,452 行。减量主要来自帮助数据去重。领域拆分增加了 import、
显式导出、跨领域依赖加载等代码，不能把包含 schema/Rust/reference 的 57.4%
降幅当作 Python 执行代码的缩减比例。

## 验证记录

报告和日志位于 `target/python2/codegen-readability/`。

| 验证 | 结果 |
| --- | --- |
| `test_python2_codegen*.py`（含隔离 checkout） | 46 项通过 |
| 最终代码的 `test_python2_codegen.py -k Python2Codegen` | 30 项通过 |
| 最终投影、导航和签名的聚焦检查 | 通过 |
| `test_python2_schema.py` | 4 项通过 |
| `test_python2_distribution.py` | 7 项通过 |
| `test_python2_tools.py` | 4 项通过 |
| `cargo test --locked -p zipzi-contract` | 11 项通过 |
| `generate.py --check --no-cache`，直接导出及分片重放 | 通过 |
| `./tools/test-sdk.sh python2` | 103 项行为、16 项恢复及静态类型正反例通过 |
| `./tools/test-sdk.sh python2 --discovery` | 9 项及静态类型通过 |
| `./tools/test-sdk.sh python2-connected` | 20 项及纯 wheel 离线审计通过 |
| 纯 wheel 离线帮助、反射、字段与链接审计 | 1,018 个符号、1,901 个 callable、2,437 个字段，各失败计数为零 |
| 源归档独立生成/重放、纯 wheel/native wheel、Local/Connected 安装示例 | 通过，见 `review-distribution/results.json` |

性能前后对照沿用 [P5 记录](python-generated-code-readability-p5.md)。本次修复未改变
编解码热点，不重新宣称性能收益。验收环境为 macOS arm64 / CPython 3.14；
没有执行 Windows 原生或 GPU/Editor UI 验收。
