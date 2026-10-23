# Properties 编辑链路基线与瓶颈：2026-10-21

本轮建立基准与可选探针，未进行性能重构。结果支持优先收敛属性结束／取消时的
投影失效、Parameter 重复验证，以及通用候选／求值器重建。
尚不支持直接决定把 SDK 全部持久数据改成共享容器，也不支持预测界面加速倍率。

[测量方法与复现](../references/property-chain-benchmark.md)；
[机器可读结果](2026-10-21-property-chain.json)。

## 方法与覆盖

- Apple M4、16 GiB、macOS 27.0.1，Release。完整环境、编译器、源码／模型／二进制
  SHA-256、Git 版本及差异保存在每组数据的 `provenance.json`。
- Mao：260 个 Mesh、6012 个顶点、7939 个三角形、128 个 Parameter、7 个 Motion、
  8 个 Expression。目标 ArtMesh215 有 158 个顶点，实际走 **MeshKeyform** 作用域；
  Motion 选取 132 条轨道、341 个关键帧的资源。导入的未解析引用诊断保留在元数据中。
- 主基线覆盖 17 项读取／提交／拖拽负载。每项 10 次预热，60 次正式交互；
  每个拖拽有 12 次不同值更新，共 720 次更新／进程。完成和取消各 30 次。
- 无探针和有探针各运行三个新进程；每轮交替二进制顺序。无探针耗时作为总成本依据，
  探针用于分段和计数。两类构建全部结束后才开始测量，采样边界拒绝编译进程，
  测量期间不运行构建或测试，并检查源码和模型未变化。
- 普通输入通过生产 `Editor` API，预览通过实际 CPU 求值和场景构造。每次确认实际值、
  候选隔离、历史条数、撤销／取消恢复和 CPU 帧接受结果。没有保存源模型。
- 以下均为调用线程的**同步墙钟耗时**，不是纯 CPU 计数器；包含调度暂停。
  不包含 GPUI 布局、GPU、OS 输入投递或显示器呈现。Animation 使用 Editing 模式，
  不包含 Result 模式的物理模拟／回放。

表中数值为三次进程中位数的中位数；原始逐输入样本和每轮分布均保留。
“含预览”按同一个输入的两个阶段相加后再求分位数，不相加独立 P95。

## 无额外旧索引读者时的主基线

### 提交

毫秒。`commit` 包含外层预验证、正式事务及同步后处理；后续 CPU 预览另计。

| 编辑 | commit | commit ＋ 后续 CPU 预览 |
| --- | ---: | ---: |
| Mesh Name | 8.57 | 9.59 |
| Mesh Opacity | 10.04 | 11.30 |
| Mesh Multiply | 17.46 | 21.39 |
| Parameter Default | 23.02 | 24.14 |
| Motion Fade | 21.23 | 27.38 |
| Expression Fade | 18.99 | 25.23 |
| Physics Mobility | 17.62 | 24.02 |

物理字段写入已很快，但一次**独立提交**仍会承担全结构验证、属性索引重建及
Animation 投影等成本，不能用微秒级字段 setter 的结果代替完整提交成本。

### 拖拽更新

毫秒。`update` 为已开始手势的一次有效更新，不含开始／结束。所有输入实际改变值。

| 编辑 | update | update ＋ CPU 预览 |
| --- | ---: | ---: |
| Mesh Opacity | 2.035 | 2.319 |
| Mesh Multiply | 1.860 | 2.099 |
| Parameter Default | 2.726 | 3.032 |
| Motion Fade | 3.121 | 7.221 |
| Expression Fade | 1.984 | 6.162 |
| Physics Mobility | 0.037 | 4.066 |
| Timeline 通道值 | 0.352 | 0.660 |

同一属性的普通提交、拖拽更新、拖拽结束是不同负载，不能相互替代。

### 松手与取消

| Mesh Opacity 手势阶段 | 毫秒 |
| --- | ---: |
| finish 本身 | 6.77 |
| finish 后索引／CPU 预览消费 | 12.86 |
| 同一次 finish ＋ 后续消费 | 19.37 |
| cancel 本身 | 0.274 |
| cancel 后索引／CPU 预览消费 | 11.99 |
| 同一次 cancel ＋ 后续消费 | 12.24 |

这解释了只测拖拽更新为什么会漏掉松手或取消后的停顿。后续消费中，
ObjectIndex 构造约 6.2 ms，CanvasTopology 构造约 2.3～2.4 ms。
调用栈采样还观察到 `Document::object_index → Arc<ObjectIndex>::drop_slow →
ObjectInfo / serde_json::Value` 的大量释放路径；这部分在现有分段中属于外围 self time，
尚未单独计时，不能把所有剩余时间都算作索引释放。

## 分段证据与重构顺序

分段值来自带探针进程的均值，再取三个进程的中位数；它与无探针总耗时的统计口径不同。
下面保留近似精度。Inclusive 包含子阶段，不能与子阶段重复相加。

**1. 先修正标量编辑结束／取消的失效范围，并扩展提交后的局部索引刷新。**

即便 Opacity 没有改变拓扑，普通属性手势结束／取消后仍各重建一次完整属性索引和
画布拓扑。一次结束的后续成本约 12 ms。Multiply、Parameter、Motion、Expression、
Physics 独立提交也会重建 ObjectIndex，单是构造约 5.9～7.0 ms，另有旧对象释放。
优先目标应是维持正确的局部投影与版本，而不是只减少一个字段的 JSON 编码。

**2. 合并 Parameter 候选与验证，同时处理 SDK 内部验证副本。**

一次 Default 提交的实测调用次数：

| 阶段 | 次数／提交 | Inclusive 合计约值 |
| --- | ---: | ---: |
| `fork_candidate` | 6 | 2.51 ms |
| `replace_parameter` 内部 `Document::clone` | 6 | 1.98 ms |
| `replace_parameter`，包含上面的内部 clone | 6 | 4.94 ms |
| 最终 `validate_structure` | 1 | 4.16 ms |
| 其中的 `validate_motion` | 7 | 2.52 ms |
| `ObjectIndex::build` | 1 | 5.86 ms |

6 次 fork 之外还有 6 次内部 clone；不能只按 fork 次数估算所有复制。
但 fork 本身也只占 23 ms 提交的一部分：仅消除 5 次 fork，按本轮粗算约节省 2 ms，
不会自动把完整提交变成六分之一。最终历史快照等其他复制仍需保留或另行设计。

**3. 通用拖拽复用候选、求值准备与缓冲。**

Mesh Opacity 更新的带探针平均分段：

| 不重叠 self 阶段 | 约耗时 | update 墙钟占比约值 |
| --- | ---: | ---: |
| 候选持久内容 clone | 0.46 ms | 22% |
| 候选 checkpoint 交换／索引重建 | 0.10 ms | 5% |
| 求值准备 | 0.57 ms | 27% |
| 替换并释放旧候选／预览 | 0.38 ms | 18% |
| 求值本体，已排除准备阶段 | 0.27 ms | 13% |

每次有效更新均发生一次 fork、一次求值准备和一次求值。
本目标 Mesh 的 Multiply JSON 编解码确有开销，但没有超过上述重复模型工作。
大型 Mesh 是否改变这个排序还需要另测；本轮仅测 158 顶点目标。

Motion／Expression／Physics 的 Editing CPU 预览还会重新生成动画源、复制快照和候选、
重建 MotionEditingPreview，并重新准备／求值模型。约 4 ms 的后续预览成本说明只优化
字段写入不够。结果不能直接外推到 Result 或手动物理模式。

**4. 验证范围比 Timeline 曲线容器复制更值得优先处理。**

Timeline 通道值更新约 0.35 ms，其中完整 `validate_motion` 的 self time 约占 89%；
SDK 候选 apply 的剩余 self 约占 3.5%，增量投影约占 2%。
这里首先应判断是否可以保留依赖／ID 索引、缩小验证范围。
普通 Mesh Name 提交的最终结构验证也会遍历全部 7 个 Motion，合计约 4.44 ms，
说明事务级验证具有跨资源成本。缩小验证范围前仍须覆盖相关依赖的正确性。

**5. Physics 重复字段校验暂居后。**

无额外索引读者时，一次 mobility update 中位数约 37 μs；确有两次
`prepare_physics_patch`。与随后约 4 ms 的 Editing CPU 预览相比，继续压缩这两次局部
准备的整体收益有限。本轮没有据此改动 Physics 编辑实现。

## 索引读者压力对照及原生核查

相同模型、目标、值、预热、样本数、拖拽长度和三轮配对构建；压力对照只运行相关
Opacity／Physics 负载，主基线还包含其他负载。保留一个上一版本 ObjectIndex 直到
当前更新／预览结束，触发完整索引 COW。这是额外读者压力条件。

| update | 无额外读者 | 保留旧读者 |
| --- | ---: | ---: |
| Mesh Opacity | 2.035 ms | 5.700 ms |
| Physics Mobility | 0.037 ms | 3.713 ms |

带探针索引 COW 约 3.6～4.1 ms。这证明“局部刷新”仍可能支付整个索引的复制成本。
但是本次原生 Opacity 拖拽的 60 次输入中，53 次实际改变值，53 次均记录为
`editor.object_index_unique`，**零次** `editor.object_index_cow_clone`。
因此本轮以无额外读者为主要结果；没有把压力对照当成当前 UI 的常态。
其他面板组合、其他编辑入口或未来快照保留行为仍需按场景核查。

原生检查完成、trace issue 列表为空、lost events 为零。它只用于核对真实 UI 的
所有权分支；有探针的原生延迟不充当无探针前后对比。

## 可信范围与验证

- 主基线的 Parameter 提交各轮中位数为 21.80／23.02／24.53 ms；Opacity update 为
  2.15／1.76／2.04 ms。可用于瓶颈排序，不能据此声称几个百分点的精确收益。
- 探针／无探针的比值同时包含探针扰动和跨进程波动，并非纯探针开销。
  部分短路径和后段预览差异超过 10%，Physics update 的配对中位数比值约 1.18；
  这条短路径主要依据无探针总耗时和稳定计数判断，不用分段比例作精确性能预测。
- 结束／取消各只有 30 个样本／进程，P95 仅描述这些样本；不报告稳定稀有尾延迟。
- 额外 `/usr/bin/sample` 采用无探针二进制、5 秒、1 ms 采样间隔，定位到了求值准备、
  复制和旧索引 JSON 释放。采样包含校验／恢复操作，且会扰动运行，因此不将其时间
  或调用栈样本比例并入上面的基线统计。
- 本轮仅一份真实模型、一个 Mesh 目标、Editing 模式；没有做模型规模／大网格／
  长 Motion 扫描，也未测试显存、分配字节数或实际帧率。基准支持更换模型复测。
- editor-core：23 单元＋322 集成测试通过，另 2 项默认忽略。
  SDK Physics：6 项通过，性能微基准默认忽略。嵌套计时核算测试 1 项通过。
  Python：42 项通过。首次与编译并行的 Python 检查触发既有 dev 握手超时测试不稳定，
  正式测量后空闲复跑全套通过。

## 保留的数据与复现

- 主基线：`target/property-chain-20261021-unique/`。
- Opacity 旧索引压力：`target/property-chain-20261021-retained-opacity/`。
- Physics 旧索引压力：`target/property-chain-20261021-retained-physics/`。
- 原生所有权检查：`target/property-chain-native-20261021-opacity/`。
- 调用栈：主基线目录内 `opacity-stacks.txt`；其 `profile-inputs.jsonl` 为受采样器干扰的
  补充运行，不属于 `round-*-*.jsonl.gz` 的正式基线。

每组 CPU 数据保存原始压缩样本、逐进程汇总、源码快照、二进制及哈希。
上面的 JSON 将三组汇总整理为可提交的结果；原始产物继续保留在本机 target 目录。

```sh
python3 scripts/property-chain-benchmark.py \
  models/local/mao/runtime/mao_pro.model3.json target/new-chain-unique \
  --reader-policy unique
python3 scripts/property-chain-benchmark.py \
  models/local/mao/runtime/mao_pro.model3.json target/new-chain-retained-opacity \
  --reader-policy retained --filter mesh_opacity
python3 scripts/property-chain-benchmark.py \
  models/local/mao/runtime/mao_pro.model3.json target/new-chain-retained-physics \
  --reader-policy retained --filter physics_mobility
```
