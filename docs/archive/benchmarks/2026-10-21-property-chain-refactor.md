# Properties 编辑链路重构与前后对比：2026-10-21

本轮按[基线结论](2026-10-21-property-chain.md)的顺序重构结束／取消缓存失效、
Parameter 候选与校验，以及通用拖拽候选／求值器。下面区分编辑 API 耗时和包含后续 CPU
预览的耗时；所有倍率都不代表 GPU、窗口呈现或整体帧率的提升。

## 实现

1. 标量 Properties 编辑不改变静态画布拓扑。结束／取消后局部更新 ObjectIndex，
   Draw Order 同步更新绘制组；保留旧索引的读者仍通过写时复制获得独立快照。
   Physics FPS 同时同步所有组和资源根节点，避免局部刷新留下旧值。
2. 提交前验证直接生成待发布的候选。结束其他编辑所有者后重新核对字段基线和
   Keyform 作用域；SDK 版本改变时重新准备。Parameter 的依赖校验、预览范围检查、
   分组调整和正式提交复用同一个候选，保留最终事务级完整校验。
   Core 参数校验以局部参数替换验证依赖，失败恢复原值，去掉内部整文档复制；
   默认值／名称等不影响依赖合法性的变化不再扫描关键形的几何。
3. SDK 新增类型化标量 patch 和可恢复失败的预览更新。拖拽期间复用事务、
   PreviewState、FrameEvaluator 和模型求值准备；Mesh appearance / draw order
   更新不经过几何 JSON。Motion、Expression 的 Inspector 读取只提取描述字段。
   求值失败时局部恢复字段、文档版本和已发布帧，之后仍可继续更新、完成或取消。
   Parameter 精度变化会失效求值准备，因为它决定关键形选择容差。

完整文档校验、撤销检查点复制、Animation 源快照复制和额外旧索引读者导致的整索引
COW 仍然存在。本轮没有把所有 SDK 数据容器改成共享存储。

## 测量方法

沿用[同一套基准](../../../tools/benchmarks/README.md#property-editing-chain-benchmark)。Mao 模型、ArtMesh215
的 MeshKeyform、Motion 目标、17 个负载、每进程 10 次预热／60 次交互／每拖拽 12 次
更新、三轮新进程均与基线相同。负载 Rust 源码哈希一致，模型文件哈希一致。

优化后再次运行原来的 plain / instrumented 配对测量，用无探针总耗时判断成本，
用探针判断调用次数和剩余成本。另外取基线保存的原始 plain 二进制与新 plain
二进制交替运行：第一轮 before → after，第二轮 after → before，第三轮 before → after。
更新 API 本身包含候选的预先求值；“含 CPU 预览”再加上后续请求预览及场景构造，
因此更新列也不是 SDK 纯字段赋值成本。
不重新编译“优化前”代码，避免当前工作树影响对照。测量边界拒绝编译进程，期间
不运行构建或测试，并检查源码、模型和二进制指纹。

表中耗时为各进程中位数的中位数；倍率为同轮 before / after 比值的中位数，
因此可能略不同于表中两列中位数相除。“含预览”先按同一次输入相加再求分位数。

## 无额外旧索引读者：交替二进制对照

单位为毫秒。表中倍率为同轮比值的中位数，尤其短路径不能用两列聚合中位数直接重算。

### 提交

| 负载 | 提交：前 → 后 | 配对提速 | 含 CPU 预览：前 → 后 | 配对提速 |
| --- | ---: | ---: | ---: | ---: |
| Mesh Name | 9.067 → 8.008 | 1.25× | 10.369 → 9.495 | 1.21× |
| Mesh Opacity | 10.686 → 7.946 | 1.23× | 11.663 → 9.095 | 1.24× |
| Mesh Multiply | 20.273 → 7.465 | 2.55× | 24.486 → 8.800 | 2.68× |
| Parameter Default | 27.303 → 8.433 | 3.20× | 28.575 → 10.034 | 2.85× |
| Motion Fade | 21.353 → 10.235 | 2.42× | 27.418 → 14.228 | 2.20× |
| Expression Fade | 19.011 → 9.668 | 2.16× | 25.060 → 14.031 | 1.98× |
| Physics Mobility | 17.787 → 18.715 | 0.95× | 24.189 → 26.114 | 0.94× |

### 拖拽更新

| 负载 | 更新：前 → 后 | 配对提速 | 含 CPU 预览：前 → 后 | 配对提速 |
| --- | ---: | ---: | ---: | ---: |
| Mesh Opacity | 2.381 → 0.193 | 12.12× | 2.741 → 0.335 | 7.74× |
| Mesh Multiply | 2.336 → 0.195 | 11.51× | 2.628 → 0.334 | 7.44× |
| Parameter Default | 2.937 → 0.399 | 7.64× | 3.255 → 0.574 | 5.76× |
| Motion Fade | 3.585 → 0.449 | 8.32× | 8.418 → 5.680 | 1.52× |
| Expression Fade | 2.124 → 0.020 | 116.68× | 6.488 → 4.752 | 1.52× |
| Physics Mobility | 0.039 → 0.044 | 1.00× | 4.404 → 4.931 | 1.00× |
| Timeline Value | 0.356 → 0.375 | 1.02× | 0.679 → 0.728 | 1.04× |

### Mesh Opacity 结束／取消

| 阶段 | 前 → 后，ms | 配对提速 |
| --- | ---: | ---: |
| 完成 | 7.469 → 6.910 | 1.08× |
| 完成后预览 | 14.578 → 1.294 | 11.27× |
| 完成＋预览 | 22.425 → 8.261 | 2.53× |
| 取消 | 0.347 → 0.387 | 0.89× |
| 取消后预览 | 13.550 → 0.245 | 55.36× |
| 取消＋预览 | 13.837 → 0.646 | 21.26× |

### Inspector 读取

此表单位为微秒。

| 负载 | 前 → 后，μs | 配对提速 |
| --- | ---: | ---: |
| mesh_read | 2.38 → 2.33 | 1.02× |
| motion_read | 150.85 → 0.62 | 241.37× |
| expression_read | 8.81 → 0.33 | 26.36× |

Physics、Timeline 的更新没有明确加速，三个配对倍率在 1 附近波动；
它们保留为对照，不把某一轮小幅变化解释成新收益。Motion / Expression 写入的
倍率明显高于包含预览的倍率，说明直接用字段写入提速推算整个交互会高估收益。

参数提交的三个配对倍率为 3.24× / 2.90× / 3.20×；
Opacity 更新为 12.37× / 12.12× / 10.84×。
原基线与这次重跑旧二进制的耗时存在可见差异，因此上表使用本次交替对照；
最初基线和优化后 plain / instrumented 三轮结果仍完整保留。

## 保留旧索引的压力对照

以下均为新旧二进制交替运行。单位为毫秒，仍采用原来的 retained reader 负载；
没有把这一条件冒充主基线中的正常所有权。

| 负载 | 阶段 | 前 → 后，ms | 配对提速 |
| --- | --- | ---: | ---: |
| Mesh Opacity | 更新 | 6.018 → 3.805 | 1.59× |
| Mesh Opacity | 更新＋CPU 预览 | 6.320 → 4.175 | 1.54× |
| Physics Mobility | 更新 | 3.576 → 3.194 | 1.19× |
| Physics Mobility | 更新＋CPU 预览 | 7.426 → 7.295 | 1.07× |

两组优化后探针均稳定记录每次更新一次整索引 COW。Opacity 更新降低到约
3.8 ms，但仍远高于无额外旧读者时的 0.19 ms，证明该共享容器的粒度仍是独立瓶颈。
本轮没有再次进行原生窗口的所有权核查；基线原生检查的 53 次实际更改均为
unique 分支，仅能代表当时的面板组合，不能据此排除其他界面的旧索引读者。

## 验证与复现

- editor-core：23 项单元测试、322 项集成测试通过，2 项默认忽略。
- kasane-core / kasane-sdk：202 项测试通过，1 项性能用例默认忽略。
- 编辑器：349 项测试通过，1 项默认忽略。
- Python：43 项测试通过。与构建并行的首次全套脚本检查再次触发已有 dev late-PID
  超时用例不稳定；构建结束后全套重跑通过，没有改动该用例。
- 新增或扩展的回归覆盖求值失败后的局部回滚、SDK 编辑继续可用、已发布帧不可变、
  单次提交／撤销，以及结束／取消后的索引和拓扑复用、跨物理组 FPS 同步。
  现有 Keyform 测试验证空补丁恢复起始值和绘制组边界。

```sh
python3 scripts/property-chain-benchmark.py \
  models/local/mao/runtime/mao_pro.model3.json \
  target/new-optimized-unique --reader-policy unique
python3 scripts/property-chain-compare.py \
  target/property-chain-20261021-unique target/new-optimized-unique \
  target/new-before-after-unique
```

旧索引压力条件重复同样步骤：构建配对运行时使用
`--reader-policy retained --filter mesh_opacity` 或 `--filter physics_mobility`，
再用对应基线目录和新目录运行比较脚本。比较脚本拒绝模型、负载源码、交互参数或
二进制指纹不一致的运行，保留原始样本、每轮分布、配对倍率和测量期环境状态。

## 工作量与剩余成本

优化后三个探针进程的调用计数一致。表中 Mesh 指本基准的 Opacity／Multiply 更新，不包含改变绘制顺序的字段。
表中为每次操作的调用次数；最后两行只统计
结束／取消之后的 CPU 预览阶段。

| 工作量 | 优化前 | 优化后 |
| --- | ---: | ---: |
| Parameter 提交：`fork_candidate` | 6 | 1 |
| Parameter 提交：内部校验 `Document::clone` | 6 | 0 |
| Parameter 提交：`replace_parameter` | 6 | 1 |
| Parameter 提交：整 ObjectIndex 重建 | 1 | 0 |
| Mesh 拖拽更新：`fork_candidate` | 1 | 0 |
| Mesh 拖拽更新：求值准备 | 1 | 1/12 |
| Mesh 完成后预览：整索引／静态拓扑重建 | 1 / 1 | 0 / 0 |
| Mesh 取消后预览：整索引／静态拓扑重建 | 1 / 1 | 0 / 0 |

每个新拖拽仍创建一个候选，第一次更新准备求值结构，后续 11 次复用；不是整个
拖拽完全没有复制／准备。完成正式提交后也仍需准备已发布模型的求值结构。

探针指出新的优先级：

- Parameter 提交中，完整结构验证及其中的 Motion 验证，合计约占 self time 的
  59%；最终事务边界现在比重复候选更值得研究。若缩小验证范围，需要证明所有
  相关依赖仍被检查，不能直接删除全量验证。
- Mesh Opacity update 中，求值及准备合计约占 self time 的 94%。重复候选复制已
  消失，继续优化编辑字段自身的预期整体收益较小。
- Animation Editing 的后续预览仍包含源快照复制、求值准备和动画求值；Fade
  字段写入减少后，这些成本依然存在。
- Timeline 保留原路径，完整 Motion 校验仍是更新中的主要成本。本轮没有重构
  这条独立的通道编辑链路。

以上比例来自优化后的有探针运行，仅用于剩余瓶颈排序。总耗时和收益使用下面的
无探针对照；不把探针比例或少量短路径的百分点当成精确性能上限。

## 保留的数据

[机器可读结果](2026-10-21-property-chain-refactor.json)包含全部优化后 plain /
instrumented 汇总，以及三个条件下的交替 before / after 汇总、逐轮数值和配对倍率。
原始压缩样本、源码归档、模型及二进制哈希保留在以下目录：

| 条件 | 优化后配对探针 | 旧／新无探针交替对照 |
| --- | --- | --- |
| 正常所有权 | `target/property-chain-20261021-optimized-unique/` | `target/property-chain-20261021-ab-unique/` |
| 旧索引 Opacity | `target/property-chain-20261021-optimized-retained-opacity/` | `target/property-chain-20261021-ab-retained-opacity/` |
| 旧索引 Physics | `target/property-chain-20261021-optimized-retained-physics/` | `target/property-chain-20261021-ab-retained-physics/` |

三个条件使用完全相同的新二进制哈希和相同源码指纹。各比较目录也保存所运行的
旧／新二进制副本及哈希，其 provenance 指向原构建的完整源码归档。
主配对探针目录内 `validation/` 保存测试输出。本次没有测量 GPU、帧率、分配字节数、
更多模型规模或 Result 物理模拟；P95 仅描述每进程这些样本，不代表稀有尾延迟界限。

## 后续架构处理：属性规则、索引快照与 Motion 描述

同日后续处理了本轮 review 提出的三个问题：

- `PropertySpec` 独立声明 `PropertyWrite`、`PropertyEffect` 与 `preview`。
  控件是否支持预览不再决定标量 patch、结构缓存或绘制组的刷新策略。
  预览所有者捕获写入方式，完整对象替换仍可使用隔离候选；空补丁恢复起始值。
- `ObjectIndex` 的查找表、依赖路径、绑定和绘制组共享存储；对象及关系记录以
  32 个指针为一页。保留旧读者时复制页目录、受影响的指针页和被改记录，
  不复制其他 JSON payload、绑定或影响路径。未变的物理关系详情和资源 FPS
  不再触发记录复制；只改关键形外观时，base-object 记录也保持共享。
- `MotionClip` 的 tracks / events 使用 `SharedVec`，保持原有 JSON 数组编码。
  Fade 只校验描述字段；Duration / FPS 额外检查轨道初始点、末端、事件边界、
  源时间范围和半帧容差，不遍历每个关键帧。正式提交仍完整校验文档，
  历史内存预算仍保守计入共享负载。无变化和失败更新保留旧状态。

另外修复组合提交 Mesh 名称和关键形外观时覆盖关键形 Draw Order 的问题，
并对齐 Timeline 验收的 SDK 参考快照：SDK seek 默认不收集可选物理观测，
参考快照现在与编辑器一样显式收集，继续比较全部快照字段和求值几何。

### 有旧索引读者的小样本交替对照

使用相同 Mao、负载 Rust 源码和编译环境，保留修改前二进制，交替运行
before → after、after → before。每个进程 2 次预热、12 次交互、每次拖拽
6 次更新，共两轮；这组参数独立于前文的 60 次／三轮结果。
单位为毫秒，数值为两个进程中位数的中位数，倍率为配对倍率的中位数。

| 负载 | 更新：前 → 后 | 配对提速 | 更新＋CPU 预览：前 → 后 | 配对提速 |
| --- | ---: | ---: | ---: | ---: |
| Mesh Opacity | 3.525 → 0.182 | 19.36× | 3.833 → 0.321 | 11.97× |
| Motion Fade | 3.779 → 0.016 | 232.62× | 7.484 → 3.682 | 2.03× |
| Physics Mobility | 3.543 → 0.038 | 93.41× | 7.423 → 3.843 | 1.93× |

Timeline 更新＋CPU 预览作为对照为 0.637 → 0.640 ms，配对倍率 1.00×。
因此短路径 API 的倍率不能代替整个交互的收益，也没有据此声明 GPU 帧率提升、
稳定 P95 或其他模型规模的表现。仍有完整提交校验、模型快照与动画求值成本；
分页目录也仍需浅复制指针，不能称为完全没有复制。

两个探针进程计数一致：Mesh Opacity 的关键形更新不复制对象记录；Motion Fade
与 Physics Mobility 每次各复制一个记录。Fade 每次调用一次
`core.patch_motion_properties`，不调用 `core.validate_motion` 或时间边界校验。
`editor.object_index_cow_clone` 现在只表示小型根节点复制；实际记录复制由
`editor.index_record_cow_clone` 单独记录，不能与前文整索引复制阶段混淆。

完整原始样本、源码归档、模型及二进制哈希保存在：

- `target/property-chain-architecture-before/`
- `target/property-chain-architecture-after/`
- `target/property-chain-architecture-ab/`

### 验证

- kasane-2d 工作区：553 项通过，1 项性能用例默认忽略。
- 最新 editor-core：25 项单元测试、323 项集成测试通过，2 项默认忽略。
- 编辑器：352 项测试通过；17 项原生渲染测试通过。
- 三个本地模型动画验收、真实 Metal 截图及 Properties 专项截图通过。
- Timeline 首次暴露上述可选观测的验收适配问题，修复后重新完整运行并通过。
- Python 44 项、文档测试、格式及差异检查通过。

新增回归检查记录复制数量及未变记录的指针共享、旧快照不可变、标量时间校验与
完整 Motion 校验的边界一致性、失败后的候选继续可用、提交／撤销／重做、
JSON 编码往返，以及组合 Mesh 属性写入保留关键形绘制顺序。

### 提交前复核

补充修复同时提交 Mesh 名称／启用状态与 Draw Order 时的缓存遗漏：通用刷新路径
现在同步对象记录的显示名称和启用状态，画布名称更新也覆盖组合提交。
回归用例先复现旧名称残留，再验证与完整重建一致、旧快照不变及撤销／重做。
此修复在上述性能采样之后完成；表格仍对应已归档的二进制，没有重新测量性能。

重新构建 Python wheel，并在仓库外隔离安装后通过 52 项 CPU 测试。
`tools/validate_sdk.py` 的完整流程因现有的 `examples/sdk/python_observe_recipe.py`
缺失而无法启动，因此未将此流程记为通过；没有删除或绕过该脚本的验收条件。
最终重新运行 kasane-2d 工作区测试：553 项通过、1 项默认忽略；
编辑器 `scripts/test.sh`：44 项 Python 工具测试、719 项 Rust 测试通过、
5 项默认忽略，三个真实模型动画、Metal 截图和 Timeline 验收全部通过。
