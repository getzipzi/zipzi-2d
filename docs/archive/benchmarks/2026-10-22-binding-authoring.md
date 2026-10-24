# C4 Binding 转换容量与 Release 探针

日期：2026 年 10 月 22 日，最终代码验收通过。对应[核心方案](../plans/unified-object-authoring-core.md)与
[实施记录](../plans/unified-object-authoring-core-implementation.md)。本记录只验收 C4
转换成本；全领域的属性链、Timeline 对照与最终交付仍属于 C6。

## 测量方法

相邻 `zipzi-2d` 的 `modules/zipzi-sdk/examples/c4_authoring_benchmark.rs`
是可复现入口。它导入真实 model3、预热求值，按普通 Mesh 网格的
“形态数 × 顶点数”选择前三个绑定，在首段插入中点。每个绑定执行八次规划、提交、
Undo；每次均检查真实提交和取消后的版本／历史。身份由导入生成，输出 ID 只用于
本次运行定位，不是跨导入对照键。

全局分配器包装 `System`，记录当前 live allocation 与规划／提交期间相对阶段基线的
最大增量。此值包含 SDK 身份集合、候选、checkpoint、报告和变更摘要的实际堆分配；
不含分配器保留页、栈、GPU、文件映射，也不是进程 RSS。规划与提交分别测量，
不能相加作为同一时刻峰值。历史中已有的 checkpoint 包含在阶段基线内。

本次环境为 Apple M4、16 GiB 内存、macOS 27.0.1、Rust 1.99.0。最终 Release
二进制重建后，在 UI／Python 构建和测试结束后依次运行两个模型，避免本任务的并发编译
干扰耗时。未进行旧版／新版配对速度测量。

```sh
cargo build --manifest-path ../zipzi-2d/Cargo.toml --locked --release \
  -p zipzi-sdk --example c4_authoring_benchmark
../zipzi-2d/target/release/examples/c4_authoring_benchmark \
  "$PWD/models/local/mao/runtime/mao_pro.model3.json"
../zipzi-2d/target/release/examples/c4_authoring_benchmark \
  /absolute/path/to/zipzi-2d/models/local/Shirousagi/Shirousagi.model3.json
```

授权模型不纳入仓库；缺少模型时必须提供合法本地路径。无普通网格的模型不是此探针的
有效输入。Heijiao 当前导入得到零个普通 MeshBinding，本次不把它记为容量验收通过。

## 容量结论

初版 `temporary_bytes = 2 × retained + 2 × payload` 低估提交峰值：Mao 实测约
8.53 MB，原估算约 5.40 MB；Shirousagi 实测约 0.66 MB，原估算约 0.50 MB。
这来自完整 SDK 提交链，而非仅新形态的位置数组。

C4 改为 `temporary_bytes = 6 × retained + 2 × payload`，为观测到约 3.3 倍文档
载荷的提交增量留出余量；网格 payload 同时计入稳定地址、重建来源和插值选择，
共享表按每采样／使用者计入元数据。历史估算仍为
`2 × retained + payload`，SDK 原有完整历史预算另外执行。

会话默认预算固定为 **65,536 采样组合、64 MiB 临时估算、256 MiB 历史估算**。
新公式在完整笛卡尔积分配前执行，溢出、组合数、临时及历史边界各有合成回归。
最终代码的两个模型均在默认预算内完成转换，探针断言实测规划／提交峰值低于计划估算。
本次使用补齐旧地址报告预留等报告预检后的最终代码。
它们是有限预检策略，不构成任意模型、分配器或前端额外克隆的严格 RSS 上限。

最终运行结果见下表（每行表示三个绑定的观测范围；耗时为各绑定八次运行的中位数，
最大耗时另列）。运行期间系统负载会影响耗时，不将本轮作为旧版／新版配对速度对照。

| 模型 | Mesh / 普通绑定 | 转换组合数 | 规划 p50 / 最大 ms | 提交 p50 / 最大 ms | 规划峰值 | 提交峰值 | 临时估算 | 历史估算 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Mao | 260 / 185 | 12 | 0.416–0.444 / 0.514 | 12.784–13.409 / 13.676 | 849,412–851,748 B | 8,530,620–8,531,604 B | 16,002,790–16,018,534 B | 5,379,554–5,387,426 B |
| Shirousagi | 24 / 15 | 3–4 | 0.019–0.024 / 0.043 | 0.584–0.675 / 0.692 | 19,444–24,996 B | 660,476–660,892 B | 1,479,068–1,496,828 B | 505,252–514,132 B |

Mao 的文档 retained 估算为 2,622,901 B，转换 payload 为 135,872–143,744 B；
Shirousagi 分别为 234,310 B 和 36,688–45,568 B。payload 包含形态与报告元数据。
以上结果完成 C4 的真实模型容量验收，并固定上述有限默认预算；全领域性能验收继续归 C6。

取消在第八个协作检查点触发，已进入网格工作；候选片段在错误返回时释放。
取消后版本、历史保持。释放本轮错误与控制 token 后，相对取消前的额外 live 字节为
Mao 0 B、Shirousagi 136 B。该观测含只读缓存等进程内记账，
不代表保留了可提交候选。SDK 回归另外覆盖跨线程取消、共享转换中断、规划后取消，
以及已装入独立候选后在提交边界取消；都不清 redo、不发布变更。
