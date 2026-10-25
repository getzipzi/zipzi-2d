# Python2 / Runtime R4 完整连接验收

2026-10-25。实现位于 `crates/zipzi-python2`，公共导入为 `zipzi`；Runtime/SDK/Editor 使用同一工作区。**按用户“直接收尾”的要求，本轮停止于最后性能修复后的必要检查，R4 保持实施中，不能标为全部退出门槛通过。**Python P3A 的统一公开契约和 agent 验收属于下一阶段，本记录不将它标为完成。

R4 的共享入口现已覆盖完整 Modeling、Plan/能力/身份与字段地址、二进制资源、工程 IO、可取消读取和 Editor 投影。`ModelingCommand` 为穷尽 Rust 分派，规则由 SDK/core 执行；生成器从编译后的 Rust 类型导出字段、默认值、枚举和有限二进制布局。输入先做不分配的长度/展开检查，再构造具体类型；Native 直接创建类型化 Python 值，Connected 大数组从借用的 SDK 字段编码，不构造完整 JSON 模型树。

| 退出项 | 实现与验证入口 |
| --- | --- |
| 全 Modeling/资源覆盖，A04/A05/A12 | 完整 `Edit.apply/apply_many`；选取式 canonical records；SDK 全领域回归、Runtime typed/authoring/references 和两后端公共行为测试 |
| 相同 recipe 的内容、几何、诊断、历史 | 安装版 `tests/r4_recipe.py`；相同固定 ID/输入、独立几何断言和 SHA256；原生 SDK 自动生成的内部 ID 不要求跨 Runtime 相同 |
| 正式/候选 CPU sweep，A15 | 普通关键形、scene binding、BlendShape；同步/后台均带准确 Source/Version/sequence；读取不发布、不推进历史、不改变人工 Preview |
| SDK 大网格与超限负例 | 通过 SDK 在候选中生成 255×255 分段、65,536 顶点、130,050 三角形；迁移已有绑定；显式完整拓扑替换及 Undo/Redo；256×256 分段被 SDK 拒绝，正式内容/历史不变 |
| 无窗口素材/建模/IO | PSD 内存导入、构建、保存/重开与 generation；Model3、裸 MOC3+纹理、配对 Cubism；包、CMO3、CAN3、Cubism、CDI3 和四类 sidecar 导出 |
| A14/A17 底层资源生命周期 | 同一驻留 Session 100 轮不同图片准备/取消、100 轮不同图片替换/历史淘汰；显式旧图片快照及磁盘文件改写；直接检查条目、唯一分配、RGBA/PNG/准备/传输字节 |
| 宿主失败边界 | A/B lease 独立；附着上传的 hash/decode/SDK/放弃失败由服务端统一终止候选；预附着预算拒绝保持候选；blob 扩容/seal 峰值、源克隆、输出展开、PSD/Cubism 预解码准入；原始协议负例不依赖客户端清理 |
| A09/A10 Editor | 安装纯包驱动真实 Editor/Metal；所有 modeling 对象投影、选择、关系、内存纹理与属性更新；人工手势期间外部提交 Busy，手势完成后旧版本拒绝；版本化可选 EditorContextProvider |
| 性能/工作量 | 同一机器、模型、Release、五对 AB/BA 进程；CPU 属性链、数值/颜色、变形、动画、Timeline/导航/双视口、另一文档 agent 批次；沿用 10%/15% 和 0.2/1 ms 门槛，不移动阈值 |

## 资源和协议边界

Native 合约为 `zipzi-python2-10`，IPC 为 `zipzi-runtime-r4-3`。二进制 frame 使用长度高位、16 KiB JSON 头、最多 1 MiB 原始分块；每个 payload 最多 256 MiB，SHA256 校验后才消费。附着 Edit 的准备状态禁止其他写入，任何已附着的普通失败都会在服务端终止候选。每阶段丢响应都保留原 owner/RequestId，恢复后序号与发布恰好一次；清理失败保留原异常及可恢复 cleanup。

默认聚合预算为 2 GiB；RGBA/PNG/blob 的唯一底层分配共享 768 MiB，blob 表为 512 MiB。候选、Plan、冻结来源、排队/执行/待发布 Job、响应展开和 socket 发送都有保留期间的计费。Arc 共享内容按分配计一次；同一 Arc 经多种视图复用时保留首次注册的 RGBA/PNG/blob 类别，三个桶互斥，不能把逻辑视图再次加计；文档/转换工作区使用保守估算，不能当作 RSS。

成功下载后的 binary request cache 可以继续持有原 Arc：每客户端最多 256 MiB、全宿主最多 512 MiB、最长 300 秒。JSON 结果缓存另限每客户端 8 MiB、总计 64 MiB。缓存淘汰后，仍在发送的 Arc 通过传输 lease 保持计费；结果未知的写入不能伪装成“已确认未执行的预算拒绝”。计数归零不等于所有底层资源必须归零：正式内容、默认 50 步历史和显式快照继续持有有效内容。

100 轮替换后的 1×1 RGBA fixture 留存当前内容和 50 步历史，共 51 条资源/204 字节；额外旧快照在关闭前最多增加 1 条/4 字节，关闭后立即回到该基线。安装流程先做一次取消预热，清理前一独立 recipe 已无根的资源条目，再建立本驻留 Session 的基线；不关闭或重开它来完成清理。

## 可复现命令与产物

从仓库根运行，原生 Editor 命令清除 `ZIPZI_DEV_SESSION_DIR`。本页保留验收结果；临时日志、样本及二进制位于 `target/python2/`，历史 JSON 产物已清理。

```sh
cargo test --locked -p zipzi-runtime
cargo test --locked -p zipzi-sdk --test integration
cargo test --locked -p zipzi-cmo3 --test import host_workspace
cargo test --locked -p zipzi-core --lib binary_wire
cargo test --locked -p zipzi-project --lib resource_budget
uv run --locked python crates/zipzi-python2/tools/test.py
uv run --locked python tools/test_tools.py -p test_python2_codegen.py
uv run --locked python tools/test_tools.py -p test_runtime_shared_inventory.py
python3 tools/runtime/runtime_inventory.py --shared-only --check
python3 tools/workspace/cargo_domain.py --check
```

源码归档在无 `.git`、无工作区引用的解包目录重新生成检查、重建正常 wheel、隔离安装并运行同 recipe。纯包解释器没有 Native extension；Native 发行物和生产 CPU 宿主不含测试控制/探针。历史失败日志保留，修复后以独立完整门禁为结论依据。未读取或搬回退役 Python 实现。

完整平台/发行 app 矩阵和动画/独立 Preview/Observer/Proposal 仍按 R5–R7 推进；本阶段测试的 CPU Modeling 采样不延期到这些阶段。

P0 六项脚本保持原测量方法，额外重采正常 Release 与独立探针。256×256 图片在 R4 改为原始二进制上传，含 start/chunk/seal/prepare/release，协议次数与旧 JSON 图片路径不同；其字节、次数和耗时单列解释，不能宣称旧固定 RPC/JSON 次数仍适用。普通小查询/写入保持原入口，既有 UI 路径仍严格使用原成对性能门槛。新的 65,536 顶点网格/绑定及四姿态 CPU 数组负载另以安装版五对进程采集，不与旧 32×32 数组或动画负载混算。

首五对 CPU 控制中，颜色提交 P95 和物理取消后的两个 P50 相对允许增量分别超出约 18–31 微秒，进程间范围明显大于该差额。为检查稳定性，采集整组固定扩至十对 AB/BA；原五对全部保留并共同计算，不替换进程、不挑选通过批次、不改变二进制/工作负载或 10%/15%/绝对裕量。最终判定以完整十对为准，初始五对报告亦保留。


## 本轮收尾结论

主体连接能力、73 项两后端行为与严格类型、15 项隔离连接、配对源码包/wheel、真实 Editor/Metal 均已有通过记录。真实模型 UI 的完整十对控制通过全部 145 项；65,536 顶点数组的五对测量也已完成。以上 Release/安装记录均对应最后布局修复之前的冻结产物，本页保留其验收范围和结果。

P0 额外发现 Native `batch_100` 的 P50/堆峰值和 `plan` 堆峰值回归。修复将完整 Modeling 命令放到 Box 中，避免每个普通操作占用 688 字节；LocalHost 邮箱也改为持有 Box<Completion>，避免普通调用槽内联整个正式 Session。透明 Box 的二进制布局/签名和成本预检有独立回归，最新 Runtime 回归已通过。**这些修复尚未重建 Release 并复验 P0，也未再次产出最终安装包；不能据此宣称回归已消除。**

图片二进制路径的 P0 中位耗时从约 26.07 ms 降至 16.79 ms，Rust 临时堆峰值从约 16.78 MB 降至 0.53 MB，但 RPC/JSON/复制计数改变，旧固定计数比较仍报失败；本轮保留原报告，不改门槛、不伪报通过。后续只需继续上述明确的 Release/受影响路径和发行复验，无需重新设计已实现领域或将 Python P3A 标为完成。
