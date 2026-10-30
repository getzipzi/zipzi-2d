# Python 生成代码与文档拆分实施方案

日期：2026-10-30。状态：已实施并验收，2026-10-30 归档。

实施收尾见 [P5 记录](../../plans/python-generated-code-readability-p5.md)、
[审查与验收](../../plans/python-generated-code-readability-review.md)及
[后续精简验收](../../plans/python-generated-code-slimming.md)。

将 Python SDK 的生成产物按业务领域和职责拆分，消除重复文档、无效赋值和重复类型实现，
保留显式类型、离线帮助、直接编解码及 Rust 契约来源。实施范围包括生成器、Python 客户端、
原生桥接产物、schema、安装包内文档和相应验收。

项目尚未正式发布，允许调整公开导入路径；同一阶段更新仓库调用方，不建设旧路径兼容层。
本次不改变业务操作、事务与恢复语义，不升级 Runtime wire protocol，也不引入运行时动态造类。

## 当前基线

基线为提交 `0d5692db` 的生成产物。以下是静态统计，不是运行时性能测量。
表中路径均相对 `crates/zipzi-python2/`。

| 产物 | 行数 | 主要问题 |
| --- | ---: | --- |
| `schema/authoring.json` | 145,072 | 命名类型沿引用路径反复内嵌 |
| `client/python/zipzi/_generated/catalog.py` | 42,049 | 继承方法、导出别名、字段说明重复存储 |
| `src/generated/modeling.rs` | 32,747 | 多个领域的原生读取、写出和预算测量集中生成 |
| `client/python/zipzi/docs/reference.md` | 30,816 | 所有条目平铺，公共行为全文重复 |
| `client/python/zipzi/_generated/modeling.py` | 19,679 | 类型、校验、编解码混合，存在无效模板代码 |
| `client/python/zipzi/_generated/binary.py` | 5,169 | 各领域二进制适配集中生成 |

具体可消除项：

- catalog 共 2,918 个条目，其中 `__repr__`、`summary` 各 851 个，各自只有一种说明。
  对应 reference 内容约 14,468 行，占全文约 47%。
- 782 个条目的字段说明同时出现在 `documentation` 的 `Fields:` 和 `fields` 列表。
  reference 中重复的字段说明块约 3,762 行。
- modeling 有 733 个类，validate、encode、read 函数各 268 个。
  1,157 次 `object.__setattr__(self, 'x', self.x)` 原值回写没有转换作用；
  382 个 `__post_init__` 完全由此组成。
- schema 有 2,483 次命名类型出现，涉及 328 个名称。内存中将定义提取到类型表、
  将其他出现改为引用，并用完整定义解析同名 opaque 后，试算为 28,802 行、约 0.87 MB，
  相对当前约 7.25 MB 减少约 88% 字节。此数值用于评估收益，不是最终格式的验收值。

主要实现入口：

- [generate.py](../../../crates/zipzi-python2/tools/generate.py)：调度、缓存、输出所有权与写入。
- [generate_modeling.py](../../../crates/zipzi-python2/tools/generate_modeling.py)：类型、校验和 Python/Rust 转换。
- [generate_catalog.py](../../../crates/zipzi-python2/tools/generate_catalog.py)：AST 分析、文档和 catalog，目前还改写 Python 源码。
- [generate_dtos.py](../../../crates/zipzi-python2/tools/generate_dtos.py)、[generate_api.py](../../../crates/zipzi-python2/tools/generate_api.py)：领域投影与公开导出。
- [authoring_schema.rs](../../../crates/zipzi-contract/src/bin/authoring_schema.rs)：编译导出入口。
- [installed_contract.py](../../../crates/zipzi-python2/tests/installed_contract.py)：离线符号、类型、文档与指纹验收。

## 目标结构与边界

### 单向生成流程

最终流程为：

```text
Rust 契约和显式 API 投影配置
                  ↓
      规范化类型图与公开 API 模型
                  ↓
Python 类型与 codec / Rust bridge / binary / catalog / reference
```

Rust 字段、默认值、枚举形式和 wire 布局仍来自编译后的契约。
Python 命名、领域归属、公开别名、便利接口和特殊投影由显式配置决定。
手写生命周期文档可以继续通过 AST 读取；生成过程不得导入 SDK、加载 native 或启动 Runtime。

`render_catalog()` 最终只渲染文档数据。当前在其中执行的 CompactValue 注入、
docstring 注入、`__all__` 添加和 `read`/`wire` 重命名，迁到类型/API renderer。
各 renderer 从同一 API 模型读取最终名称和文档，不再互相修改对方的字符串输出。

### 文件布局

以下为目标骨架，`<domain>` 按下一节的归属规则展开；不是按固定行数切出的编号文件。

```text
crates/zipzi-python2/
  tools/codegen/
    model.py                  # 类型图、公开符号、字段与方法文档模型
    normalize.py              # 定义去重、引用解析、冲突检查
    projections.py            # 公开路径、领域归属与特殊投影
    dependencies.py            # 模块依赖与循环检测
    outputs.py                 # 产物清单、删除检查与指纹输入
  schema/
    authoring.json             # 格式版本、协议、根类型、操作与分片索引
    types/<domain>.json        # 每个命名类型只定义一次
  client/python/zipzi/
    modeling/
      __init__.py              # 核心操作/查询入口和领域模块
      <domain>.py              # 有限、显式的公开导出
    _generated/
      metadata.py              # 包版本、协议与构建指纹，无 catalog 依赖
      values/<domain>.py       # 显式 dataclass、enum、union
      codecs/<domain>.py       # validate / encode / read
      binary/<domain>.py       # 直接二进制适配
      catalog/
        index.py              # 路径路由、规范条目与别名索引
        common.py             # 公共方法及行为说明
        <domain>.py            # 领域条目数据
    docs/
      reference.md            # 导航和阅读入口
      reference/
        common.md
        runtime.md
        <domain>.md
  src/generated/modeling/
    mod.rs
    <domain>.rs
  generated-manifest.json     # 生成器拥有的产物路径及内容摘要
```

现有小型生成模块无需为了目录对称立即迁移。迁移 modeling/binary 时，统一其类型引用；
确有重复的领域 DTO 才合并。保留现有 meshes、parameters、animation 等便利入口，
但它们引用规范类型，不再额外声明同一类型。

### 类型归属与公开路径

首先覆盖以下领域，较大的领域再按语义拆为子模块：

| 领域 | 内容 |
| --- | --- |
| common | 无领域依赖的坐标、基础值和共享枚举；不成为杂项收纳模块 |
| mesh、scene、parameters、bindings | 网格、场景对象、参数及绑定的规范记录与领域编辑值 |
| deletion | 删除请求、选择、汇总、分页报告及相关枚举 |
| motion、expression、pose、physics | 对应创作记录、编辑值和结果 |
| sampling、spatial、observation、preview | 采样、空间查询、GPU 观察与独立播放的输入输出 |
| project、addressing | 项目请求报告、对象寻址与能力查询 |
| commands、queries | 跨领域命令 union、查询和聚合结果，依赖具体领域 |

每个投影记录 `rust_type_id`、`projection_id`、实现模块、规范公开路径、公开别名，
以及构造、字段转换和 wire 规则的特殊项。字段内容不手工复制进投影配置。

默认一份 Rust 类型对应一份规范 Python 实现。只有构造签名、字段表示或语义确有差异时，
允许不同 `projection_id`；必须在配置旁说明原因，并有转换测试。
相同语义的导出直接引用同一 class 对象。现有 sampling 对 MeshQuality 等类型的复用可沿用。

目标公开入口示例为 `zipzi.modeling.deletion.DeletionRequest`；保留类型名，先不同时缩写类名。
`zipzi.modeling` 根仅导出核心命令/查询入口和领域模块，不保留全部类的通配重导出。
旧导入路径随调用方一起删除；没有历史 pickle 或外部脚本迁移承诺。

生成器先建立依赖图，再决定 import。共享叶类型在底层，聚合命令在上层；强连通类型应放在
同一语义模块，或通过明确的局部 codec 导入解决。禁止为了打破循环大量降级为 `Any`。
枚举默认值、union 赋值、dataclass 默认工厂是真实运行时依赖，不能仅放入 `TYPE_CHECKING`。

若设置 class 的 `__module__` 为公开路径，该模块必须具备解析注解所需的私有绑定。
`inspect.signature()`、`typing.get_type_hints()` 和新旧模块加载顺序的测试作为拆分门槛。

## 文档和 catalog 去重规则

文档模型明确区分 summary、字段说明、声明方法、继承行为和示例。执行以下规则：

1. 生成 class docstring 时组合 summary 和字段说明；catalog 分开存储两者。
   reference 与 `ApiDescription.format()` 各渲染一次字段，不再输出两份 `Fields:`。
2. `CompactValue.summary`、`__repr__` 的说明各存一份。具体类型记录继承关系，
   reference 链接公共行为页，离线 `describe()` 解析继承后返回完整说明。
3. 同一对象的多个公开路径使用规范条目和 alias。调用 `describe(alias)` 保留请求路径，
   但内容取自规范条目；不把内容复制到 alias 数据中。
4. Session/Edit 的相同方法说明复用。具有不同错误、资源或候选状态语义的方法保留独立说明，
   不以相似文本为依据盲目合并。
5. `reference.md` 保留当前入口文件名，只提供导航、用法选择和公共语义链接。
   领域页按任务/类型组组织，不再将所有路径按字母序平铺为全文。
6. 离线目录提供内部 `lookup(path)` 和 `iter_paths()`，测试不再导入巨大的 `CATALOG` dict。
   `children` 与直接查询共用索引；枚举与继承展开采用有界、确定性规则，检测 alias 环。
7. `CONTRACT` 等常量迁入 metadata；Connected backend 不导入帮助数据。
   catalog 分片仅在相应查询时加载，不能探测宿主或网络。

字段语义仍须同时可从 Python docstring、`describe()` 和领域 reference 找到。
不同使用渠道保留说明属于有意的输出，不要求跨文件消除所有相同文本。
本次消除同一渠道内的重复展开，不牺牲直接查看源码和 `help()` 的可用性。

现有“字段必须出现在 catalog.documentation 内”的测试改为检查完整格式化结果，
并保留 `fields` 的结构化断言；不能直接删除语义覆盖测试。

## 代码精简与转换约束

`freeze()` 应返回是否需要转换的信息。字段无转换时不生成赋值，所有字段均无转换时
省略 `__post_init__`。tuple、嵌套容器、MappingProxyType 和 extension 的所有权转换保留，
禁止仅按“值看起来不可变”删除必要的深层冻结。

复用现有 `_dto_support.py`、`_values.py`、`_extensions.py` 的基础能力，
为确有缺口的整数范围、有限浮点数和固定长度检查补充 helper。
保留有类型的领域校验入口；不得用通用反射遍历替换所有显式 codec。
保持 bool/int 区分、f32 范围、字段缺省、空值、flatten 冲突和 union tag 处理。

不将全部 union 变成自由 dict，也不引入 `make_dataclass()` 或同步维护一份完整 `.pyi`。
类型声明、JSON codec、binary codec 和 PyO3 bridge 是不同执行职责，不能因字段相同就删除其中一种。
去重由生成时共用模型和少量运行时 helper 完成。

Rust modeling 按同一领域拆分，必要的兄弟模块调用使用显式可见性。
PyO3 按类型归属定位 Python class，去掉硬编码的 `zipzi._generated.modeling` 导入。
保留直接转换、预算测量和 binary 布局，不经 JSON 中转。

## Schema 与生成产物管理

### 规范化 schema

先在 Python 生成阶段将当前完整导出归一化为类型图，各 renderer 改为消费引用。
待消费者迁移完，再让 Rust exporter 输出规范化结构，并将持久化文件按领域分片。
中间过渡适配器仅用于实施阶段，收尾删除；不维护旧 schema 版本兼容分支。

类型 ID 使用完整 Rust 类型身份，包括泛型参数；字段和 variant 的顺序保持契约顺序。
同名 opaque 引用可以解析到完整定义；两个不一致的完整定义必须报错。
跳过字段的占位类型不视为公开类型。缺失引用、未知投影和循环定义必须显式处理。

`authoring.json` 升级内部 schema 格式版本，记录分片路径与摘要；此版本独立于 wire protocol。
`--schema` 读取根文件后只解析其受约束的相对分片路径，验证完整性、重复定义和路径边界。
契约摘要从排序稳定的完整逻辑内容计算，覆盖所有分片，不仅计算索引文件字节。
同步修改 distribution 的 `source_contract_sha256` 计算和消费者。

### 输出清单和删除

当前固定 `TARGETS` 与 unmanaged-file 检查需要改为根据已验证模型构建预期产物清单。
动态目标仍受生成目录和有限公开导出路径的白名单约束，不接受任意路径写入。

`generated-manifest.json` 记录相对路径、生成器 owner 和内容摘要；清单自身不纳入自身摘要。
首次引入时仅接管既有明确 `TARGETS` 且生成标记匹配的文件。

写入前一次性验证完整增删改集合。旧清单中存在、新清单中不存在的路径，只有在路径安全、
生成标记及旧内容摘要均匹配时才允许删除；被手动改过的旧产物报错并保留。
未知文件、符号链接和手写文件不得自动接管或删除。

继续持有 checkout 生成锁。先在临时目录完整渲染和验证，再逐文件原子替换，清单最后发布。
不宣称跨文件事务原子性；中断后 `--check` 必须检测混合状态，正常生成能明确恢复或报告冲突。
`--check` 只报告缺失、变化、陈旧文件，不能修复、删除或改写清单。

### 缓存和构建指纹

cache key 包含完整契约、投影配置、全部 codegen 模块、手写 API 文档来源、版本元数据与工具版本。
将当前 `generate*.py` glob 扩展为受控的 codegen 输入枚举，新增 helper 不能漏入缓存键。
过渡期间凡 renderer 实际读取的源文件都纳入键；最终 renderer 不读取磁盘上的旧生成产物。

安装源指纹从排序后的运行时代码和指南计算，明确排除承载该指纹的 metadata 与派生 catalog，
避免自引用。catalog/reference 完整性由产物摘要另行验证。
生成器和 installed audit 共用同一套输入选择规则，并覆盖新增子目录和非 Python 文档。

## 分阶段实施

每阶段独立提交，提交包含生成器、重新生成的产物、调用方和对应测试。
不得提交只能在后续阶段才能 import 或通过 `--check` 的中间状态。

| 阶段 | 实施内容 | 完成门槛 |
| --- | --- | --- |
| P0 基线与产物管理 | 增加统计报告、引入输出清单和安全删除、更新缓存输入枚举 | 原有产物语义不变；生成确定性、清单与保护测试通过 |
| P1 文档和模板精简 | 分离 metadata；catalog 引用公共行为；拆 reference；移除无效赋值 | 帮助路径覆盖不丢失，字段只渲染一次，纯客户端离线审计通过 |
| P2 统一生成模型 | 引入规范化类型图、投影归属和 DocEntry 模型；迁出 catalog 的代码改写 | renderer 从统一模型生成；完整定义冲突、缺失文档和引用均能检出 |
| P3 Python 与原生拆分 | 按领域拆 values/codecs/binary/Rust；更新公开 modeling 包及调用方 | 类型检查、导入、Local/Connected 行为与 wire 验证通过；旧大文件删除 |
| P4 Schema 分片 | Rust 导出规范化结构；持久化分片；更新 replay、哈希和分发消费者 | schema replay 与直接导出完全一致；归档内可独立生成和验证 |
| P5 安装与收尾 | 纯 wheel、native wheel、源归档、离线文档、性能对照和文档更新 | 全部验收项完成；删除临时适配器，记录实际体积和性能变化 |

P1 可以先在现有 AST 分析基础上实现规范条目引用；P2 再替换其输入模型。
类型归属在 P2 确定，P3 使用它拆分，避免先搬文件再重写 import。

### P0 的统计交付

新增 `tools/python/python2_codegen_stats.py`，仅分析文件和 AST，不 import SDK。
输出各文件行数/字节数、类型数、重复继承文档数、重复字段块数、无效 setattr 数和类型归属覆盖率。
报告保存到 `target/python2/codegen-readability/`，把汇总写入收尾记录。
工具测试放入 `tools/tests/`；不为文档措辞编写快照测试。

性能另在同一环境、同一负载下记录冷进程 import、首次 describe、重复 describe、
冷/热生成耗时与峰值内存。SDK 探针通过 `./tools/dev-python` 启动；
准备和 native 构建时间单列，不混入被测调用时间。

### P3 的调用方更新范围

查找并更新公开 `zipzi.modeling` 类型导入、私有 `_generated.modeling` 引用、
`encode_*`/`read_*` 调用、PyO3 模块字符串，以及二进制适配的类型路由。
范围包含 Python SDK 手写模块、tests/type fixtures、recipes、仓库 tools 和活跃说明文档。
优先复用现有领域入口；低层建模值使用新的 modeling 子模块。

历史归档保留历史路径，不按文本匹配批量重写。更新现行 README、guide、dev-python 参考和
活跃工作流示例，使它们只展示新路径。分发与工具测试中的冻结样本按其测试目的单独处理。

## 测试与验收

先运行聚焦测试，阶段收尾再运行相应完整门禁。以下命令从仓库根执行；
新增测试模块由 `tools/test_tools.py` 自动发现。

| 改动 | 运行命令或验证入口 |
| --- | --- |
| P0/P1/P2 快速 codegen 回归 | `uv run --locked python tools/test_tools.py -p test_python2_codegen.py -k Python2Codegen` |
| 生成漂移与无缓存重放 | `uv run --locked python crates/zipzi-python2/tools/generate.py --check --no-cache` |
| 离线帮助与发现行为 | `./tools/test-sdk.sh python2 --discovery` |
| 契约导出/schema 变化 | `cargo test --locked -p zipzi-contract` |
| 类型、Python 与 native 联动 | `./tools/test-sdk.sh python2` |
| 纯 wheel 与真实 subprocess host | `./tools/test-sdk.sh python2-connected` |
| 代码生成隔离阶段验收 | `uv run --locked python tools/test_tools.py -p test_python2_codegen.py`，包含昂贵的隔离 checkout 测试 |
| 分发门禁 | `uv run --locked python crates/zipzi-python2/tools/distribution.py --output target/python2/codegen-readability/distribution` |

上述 Python2Codegen 选择仍包含 Rust 编译类用例，日常修改可继续用具体方法名 `-k` 缩小。
无需例行运行 SDK/Editor full，也不为此结构重构要求 GPU；若修改观察路径的实际行为，
另运行相应 observation 验收。

新增或调整以下有意义的验证：

- 类型图：定义唯一、opaque 解析、未解析引用、projection 冲突、完整定义冲突和确定排序。
- 输出：嵌套目录、删除旧产物、手改旧产物拒绝删除、symlink 拒绝、`--check` 零修改、
  缓存删除后重建、helper 修改后失效、写入中断后检测。
- 文档：规范条目、别名、继承方法和 children 全部可达；每个字段只出现一次；
  未知路径仍报 KeyError；业务失败/恢复语义和资源关闭要求不丢失。
- 类型：所有公开类的签名与 get_type_hints；不同 import 顺序；union 默认值；
  正例与负例静态检查；别名指向同一 class。
- 转换：从现有测试选取 record、unit/tagged/untagged union、默认值、可选字段、
  容器冻结、bytes、扩展 flatten、数值边界，验证 Python/JSON/binary/native 的约定。
  binary 对照使用 Rust 实现或已有固定样本，不能仅以 Python 自己往返证明布局正确。
- 分发：纯 wheel 不含 native 仍可 describe；所有分片和文档实际装入 wheel；
  离线审计递归检查文档链接和锚点；摘要覆盖完整契约，源归档可从零生成。

## 完成标准

以下结构条件必须全部满足：

- [x] 不再存在 4 万行 catalog 和 3 万行 reference 单文件；公共表示方法说明各存一份。
- [x] 每个 reference/describe 条目的字段只渲染一次，全部公开字段保留语义说明。
- [x] modeling 中无原值回写，无仅含原值回写的 `__post_init__`。
- [x] 每个规范类型只有一个实现归属，特殊投影都有显式原因和测试。
- [x] modeling 类型、codec、binary 和 Rust 转换按领域拆分；根模块无全量通配导出。
- [x] schema 命名类型只定义一次，操作和字段使用引用；全部生成器消费同一类型模型。
- [x] catalog 不改写 Python 源码；Connected backend 不为版本常量加载 catalog。
- [x] 所有旧产物安全清除，生成检查、安装审计、源归档与缓存能识别全部新目录。
- [x] P5 记录前后体积、测试命令/结果和性能对照，所有阶段临时适配器已移除。

可读性预算作为 P0 起新增的统计检查：reference 导航页不超过 200 行；
Python/Rust 生成代码和领域 reference 单文件目标不超过 1,500 行，2,000 行作为审查阈值。
超过阈值时先按语义继续拆分；不可合理拆分的单一类型/union 必须在配置中具名说明，
不能通过删空行、压缩长行或创建无语义编号分片达标。JSON 主要检查定义唯一与分片归属。

体积报告同时展示单文件最大值、总字节和重复项数量，防止只搬移代码却未减少冗余。
不预先承诺总代码压缩比例。首次帮助/import 内存应随 catalog 拆分下降；
若代表性编解码性能出现可重复退化，先检查公共 helper 与 import 开销，
保留热点的显式生成实现，并在收尾记录中解释取舍。
