# R4：Modeling 与资源底座实施记录

日期：2026-10-24。状态：**实施中，首批底座已落地，尚未达到 R4 退出门槛**。
本记录不代表完整 Modeling、资源与工程操作已经交付。
2026-10-25：当前 Python 实现以独立 `zipzi-python2` 为准；其
[网格绑定/CPU 采样切片](python2-r4-bindings.md)已将跨领域 `ModelingCommand`
从 SDK 移至 Runtime。下文首批实现与结果保留原始历史范围。
逐能力状态见 [r4-coverage.json](../../runtime/r4-coverage.json)，本页保留测试验收结果。

## 基线与接口选择

起点是同仓 `0acebfa7d05455566d5b635744de75bdccdfd17a`，开始时工作区干净。
SDK、Project、Runtime、Python 与 Editor 共用本次未提交改动及根 Cargo.lock；
没有增加依赖或修改锁文件。锁文件 SHA-256 为
`9926b1fb21d17c1ad0679f3a3032f2ef8f087388124af11376267ea49be630b0`。

用户明确允许重构未经验证的 Python 接口。本批增加共享 SDK 建模值协议，保留
R3 最小闭环供回归；不以旧 Python facade 的方法数量推定新连接已经完整。
新的 `ModelingCommand` 是公开 SDK 候选操作集合，每项调用既有 SDK 方法；没有
将 PyO3 私有 Command 搬进协议，也没有增加连接层领域校验或另一份历史。

协议升级为 `zipzi-runtime-r4-1`，旧宿主需要重建并重启，严格握手拒绝旧契约。
握手按实际切片公布 modeling_batch、modeling_records、modeling_cpu_samples、
modeling_plans、image_rgba、image_memory_accounting 与 blobs。

## 已实现内容

### 共享建模入口

- `Edit.apply_modeling(*commands)` 接收带 `op` 的 SDK 记录或领域意图，批量调用
  Mesh、Parameter、Part、Transform、普通/场景绑定、BlendShape、Glue、Offscreen、
  场景、组织、拓扑内容、引用、资源、附件和文档操作。
- `Session/Edit.modeling_query(**selection)` 返回集合 ID、文档元数据和显式选择的
  完整 SDK 记录，不在客户端镜像整份模型。正式查询使用借用；候选查询读前序修改。
- `Session/Edit.sample_modeling(parameters, ...)` 在冻结来源上执行显式参数批次，
  返回来源、版本、候选序号、参数、CPU drawable/deformer frame 与既有 SDK 几何诊断。
  新增快照诊断入口复用原算法与容差。Native 结果也标明来源，求值不写 Preview/历史。
- `plan_modeling(domain, intent)` 与 `Edit.apply_modeling_plan(plan)` 调用 SDK 的八类
  规划入口；报告包含版本、影响 ID、复制映射、成本及损失。正式来源只允许用于该
  版本的初始候选；候选来源还校验 Edit 身份和序号。释放、客户端到期、Session 关闭
  会释放托管 Plan 及其固定的内存图片。字段地址和 SDK Handle 尚未连接。

已验证的共享 recipe 包含：生成 RGBA 图片、Mesh/Parameter 创建、稳定样本 ID 的
普通绑定、BlendShape、候选参数 sweep、候选 Plan、正式 Plan、提交、Undo/Redo 与保存。
普通/BlendShape 同时取值 `(0,0)、(.5,.5)、(1,1)` 时首顶点 X 分别为 `0、6、12`，
Native 与隔离安装的 Connected 返回相同几何、参数、诊断、图片哈希和历史。
Native 分支还设置独立的 authoring preview 并核对值、revision 和求值计数未变化。
这不是原生 Editor 人工播放状态的最终验收。

### 资源持有、计费与回收

Project 新增共享 `ImageMemoryBudget`、准备中 reservation、资源表计数与显式回收；
SDK 暴露预算/回收入口，候选固定自身引用的图片。快照现在包含未提交的 prepared
image，修复了其只从正式资源表查找内容的问题。

计费以 RGBA/PNG 的实际 Arc 缓冲长度为准，同一池内同一分配只计一次，分别报告
RGBA、编码 PNG、blob 和准备中负载。弱引用账本不持有像素；资源表条目移除后，
仍被 TextureData 的像素 Arc 引用的分配继续计费。PNG 编码前预留上界，并限制写入
增长；PNG 文件准备先预留文件/像素负载。Vec 转 Arc 的临时副本包含在保守预留中。
编码预算失败不缓存错误，释放准备占用后可以重试。批量资源准入先整体检查，避免
RGBA 成功而 PNG 失败时留下部分计费。

正式内容、Undo/Redo 的描述符构成资源表根；活动候选、Plan、快照和 prepared image
通过实际引用固定内容。SDK commit 在历史淘汰/redo 清理后回收，Runtime 取消、普通
失败和断连清理同样触发回收；独立 SDK 调用者可显式 `collect_image_resources()`。
资源准备不再要求等到关闭整个 Session 才能回收。托管前创建的 owned candidate
在登记和最终提交时重新检查宿主预算，不能沿用之前的无限预算绕过限制。
Cubism 导入的候选资源表也沿用接收 Session 的共享预算。

验证包括同一 Session 的 100 轮不同图片准备/取消，以及 100 次图片替换提交：
取消后条目和存活字节回到 0；历史上限为 2 时保留正式/Undo 两代共 3 张图片，
另有旧候选/快照时最多 4 张；最后释放旧来源后回到 3 条/12 字节。
PNG、RGBA 准入失败保留正式版本和历史，准备计数归零；旧候选、旧快照、Undo/Redo
均能继续读正确像素。这些是实际存活负载断言，不是 RSS 或分配器驻留测量。

### Blob 与预算

`Connection.create_blob(length)` 显式预留后提供 append、seal、read、recover、close。
单 blob 上限 256 MiB，块上限 1 MiB，宿主 blob 逻辑总额 512 MiB，每客户端 8 个。
控制帧仍是 JSON；块中的字节用数组编码，没有新增专用二进制 framing。
`Edit.create_rgba_image_from_blob` 分享同一 Arc，不复制图片负载；
`Edit.apply_modeling_blob` 可消费完整 UTF-8 JSON 批次。上传和释放有明确生命周期，
append 丢响应后按原请求恢复 offset，不重复追加；清理不替换原异常。

Plan 每客户端最多 64 个，宿主估算总额 128 MiB；资源池上限 768 MiB，blob 也受
该池约束，这是比两类额度相加更严格的首批策略。文档/历史/候选/Plan 元数据仍采用
保守估算；图片/blob 的实际负载加入 Runtime 总 CPU 计费。Plan 捕获前检查源与输入
增长，转换沿用 SDK 限额；采样捕获前检查批次/预测结果上限。超大响应明确报预算
错误，不因写不进控制帧而静默断开连接。

当前单次采样最多 64 个参数组，大结果需要拆批；昂贵规划/求值尚未迁入可取消的
只读后台 Job。没有声明固定 RPC 延迟、零开销、全宿主 RSS 或完整公平调度已经验收。

## 检查及修复记录

测试命令、计数与日志路径以 results JSON 为准。SDK fast 的两个 ignore 是既有的
Physics 字段和 Motion library Release 测量，不计为通过。未运行 SDK/Editor full、
原生 Metal 验收、发行 wheel、完整 UI 性能配对或真实授权模型验收。

开发时先修复了 recipe 对 DeltaKeyforms 使用错误判别字段的问题；真实安装客户端
随后通过。Project 聚焦测试首次编译发现旧测试仍调用 OnceLock 的 get，已按新的
可重试缓存更新并重跑。另修复托管候选沿用旧资源预算、PNG/像素部分计费以及
超大 Plan 输入未计入准入估算的问题。

## 后续退出项

1. 连接 SDK Handle、稳定字段/内容地址与完整能力查询，并验收删除、移动、Undo 后失效。
2. 为全部 Modeling 领域补同 recipe 的连接验收；目前其他建模命令只是已接线，不能以
   SDK 自身测试通过代替连接覆盖。补大网格、有绑定拓扑和 SDK 限额负例。
3. 接入文件图片/附件、读取诊断、打开/导入/替换/各格式导出完整入口，统一 generation
   与路径预留；对磁盘内容做真正的资源快照固定。
4. 将昂贵准备、规划、采样和工程 IO 接入可取消 Job；PSD/Cubism 解码器的全部准备中
   负载还需在解码前准入，当前只完成最终存活资源计费与注册预算。
5. 完成 EditorContextProvider、全部投影/选择/关系/纹理/属性表单的原生外部写验收，
   以及无窗口导入→构建→保存重开→导出闭环。
6. 统一所有临时来源/队列/结果的宿主总预算，记录大数组往返和性能，补 R4 RSS/缓存
   报告；达到 A04/A05/A09/A10/A12/A14/A15/A17 相应门槛后才标记 R4 已交付。

## 提交前复核

资源准备入口已统一为返回显式 lease 的 `prepare_rgba_image`，对应 Editor 调用点、
fixture 和快照测试同步迁移。查询展开前增加有界准入，并验证重复选择被预算拒绝后
候选仍可提交；准备中的图片也经过其他提交/回收后继续可读。

补充聚焦检查：Runtime protocol/modeling 13 项、SDK image_memory 7 项、Editor core
Runtime 18 项、inventory 工具 3 项通过；source inventory 与 diff whitespace 检查通过。
原始首批验证和本次提交前复核分别记录在 results JSON 的 tests 与 commit_verification。

讨论确认的后续调整包括二进制 blob 帧，以及将 ModelingCommand 调度定义移到适配层；
本次提交仍保留现有 JSON blob 与 SDK 总枚举实现，R4 状态仍为实施中。
