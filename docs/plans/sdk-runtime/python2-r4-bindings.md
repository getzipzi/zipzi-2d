# 新包 R4：网格绑定与 CPU 参数采样

日期：2026-10-25。基线 `c0e186b` 已保存 R3 类型化事件、参数及批量查询。
本批接回新的 `zipzi-python2` 网格绑定和显式 CPU 采样，仍按
[R0–R4 主线](python2-r0-r4.md)推进。R4 尚未达到完整退出门槛。

## 共享层与公共调用

跨领域 `ModelingCommand` 及逐项调度已从 SDK 移至
`zipzi_runtime::modeling`。原协议操作/字段保持原含义，Runtime 的批次准入、
统一候选失败和一次来源推进保持不变。SDK 继续提供领域方法、查询、规划和
求值；不再提供跨领域传输命令枚举及 `EditSession::apply_modeling`。

SDK 新增 `MeshBindingCreateSpec`、`MeshBindingAxisSpec`、`MeshKeyformSpec` 及
领域内 `MeshBindingOperation`。输入使用数值网格，不序列化持久记录里的导入
专用 `keys` 字段；Rust core 分配/规范化轴和 sample 身份。省略的外观由 SDK
默认值补齐；网格完整性、范围、依赖、插值和关键形规则都调用既有 core/SDK。

```python
from zipzi import bindings, sampling

with session.edit("Bind face") as edit:
    binding = edit.bindings.create(bindings.MeshBindingSpec(face,
        (bindings.Axis(angle, (0., 1.)),), (
            bindings.Keyform((0.,), geometry.positions),
            bindings.Keyform((1.,), tuple((x + 10., y) for x, y in geometry.positions)))))
    snapshot = edit.bindings.get(binding)
    candidate = edit.modeling.sample(({angle: .5},), expected=snapshot.source,
        checks=sampling.Checks(min_triangle_area=.01))
formal = session.modeling.sample(({angle: 0.}, {angle: .5}, {angle: 1.}))
```

`ops.CreateMeshBinding` / `SetMeshKeyform` 与网格、参数、图片和 Plan 进入同一个
`Edit.apply_many`。混合批次只推进一次候选来源，提交只有一条历史。
`set_keyform` 替换一个数值网格点的完整负载，保持轴/sample 身份。
创建失败、关键形错误及捕获编码错误均沿用现有不可部分发布的事务规则。

`session/edit.bindings.get/get_many/list` 返回 SourceRef 和只读规范记录，保留
选取顺序及重复项；公开轴/sample 身份、关键形 sample 坐标、位置、外观及绘制
顺序。先核对整批 256 条/1 MiB 保守响应与剩余 CPU 预算，再复制选中记录。

## 采样契约与范围

Local 和 IPC 现在共用 `Runtime::sample_modeling(ModelingSampleQuery)`；旧协议
中独有的采样准备/预算逻辑已收回共享入口。每批最多 64 个显式 pose，每个
pose 最多 256 个参数；8 MiB 保守响应估算及展开预算可能要求更小的批次。
Runtime 在复制冻结来源和实际求值前核对来源、响应与宿主余量。

结果包含精确 formal/edit SourceRef、Version、会话状态、不可变请求参数、
实际求值参数（含 clamp）、CPU drawable/deformer/offscreen/render-command
frames，以及原 SDK 几何诊断。可选 `expected` 只接受完全相同的来源；候选继续
写入后旧来源拒绝。采样与绑定查询失败不会毒化候选。

求值在 SDK 冻结快照上进行，不设置 Preview、不提交、不消费事件或增加历史。
丢响应仍按原 RequestId 恢复；即使另一个客户端随后发布，恢复得到原版本的
frame。恢复所有者没有新增事务状态机。本地使用直接拥有的 PyO3 转换，不经过
JSON 编码、解码或 Value 树；网格生成器保持原有 10 个产物，本批转换先手写。

原生契约为 `zipzi-python2-6`；IPC 保持 `zipzi-runtime-r4-1`，新包要求能力
`typed_mesh_bindings` 和 `typed_modeling_samples`。旧宿主缺少能力时明确拒绝。

P0 重跑发现既有冷启动竞态：公开路径 bind 到 chmod 之间，严格客户端可能
看到临时 socket 权限并拒绝。宿主现先在私有目录内准备短名称 socket，完成
`0600` 和 nonblocking 设置，再通过同 inode 硬链接原子发布；已有文件不会被
覆盖，失败回收临时路径。未放宽客户端检查，也未改动冻结测量脚本或阈值。

## 验证

本页保留实际命令、验收结果和未运行项。
现有 P0 比较只覆盖原六项 CPU fixture；
它不代表新绑定/采样专项性能、大负载、真实 Editor 延迟或完整 R4 预算验收。

- 源码 50 项行为/故障用例通过；严格类型正例通过，8 条精确反例为零基行号 5/6/7/8/9/12/15/16。
- SDK 新 spec 2 项、Runtime authoring/host/modeling/protocol 共 29 项聚焦测试通过；原批次拒绝/中途失败、资源/Plan/Job 及请求去重继续通过。
- socket 发布新增 3 项聚焦测试，验证私有权限、实际收发、失败保留已有文件、临时路径清理及最长已支持路径。
- 两后端数值 recipe 覆盖 canonical 身份、0/.5/1/越界 clamp、关键形更改、过期来源、诊断、正式/候选隔离、no-op/redo、失败和响应恢复，以及保存后从新宿主重开。
- 安装纯客户端 14 项连接测试通过；新增用例在丢采样响应后由另一连接改写关键形，再恢复原版本 frame。
- 生产 Editor 双客户端冷启动、共享绑定/参数历史、CPU pose、真实被杀客户端宽限回收及保存重启/重开通过；正常纯/原生 wheel 在 `/tmp` 用 `-I` 分别通过绑定、纹理、Plan、历史与新宿主重开 recipe。实际纹理 frame 的 render command、非默认关键形外观/绘制顺序也有断言。
- 原生 Editor + 安装纯包 + Metal 验收通过：同一历史写入网格/参数/绑定，候选和正式数值 sweep 一致；人工 Preview 保持 -15，另一个进程采样并取消临时候选后，Preview 值/版本、正式版本和共享历史不变。UI Undo 撤销参数/绑定，另一客户端读到连续发布事件。

完整 BlendShape/scene-binding 数值 recipe、场景/组织/拓扑、句柄/地址/能力、
完整 Plan、二进制资源/格式 IO、长任务取消、公平调度及总预算仍待 R4 后续实现。
本批 CPU frame 定义覆盖现有字段；只有本记录列出的建模场景已经独立验收，
其他领域须随其写入能力补充行为证据。
