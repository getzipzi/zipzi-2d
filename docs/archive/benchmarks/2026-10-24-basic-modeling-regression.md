# 基础建模 UI 性能退化定位 — 2026-10-24

剩余 UI draw 退化的主要 CPU 来源已定位到菜单能力查询：每次根视图 render 都重新计算菜单状态，新增创建命令的位置校验调用完整 organization capabilities，其中包含删除依赖检查和全模型引用扫描。该查询又分别出现在命令 enabled 状态和禁用原因计算中。属性控件重建与 Taffy 求解不是本轮 Warp／Rotation 剩余增量的主要来源。

## 原生复测

原基线 Editor `44accd1`／SDK `b86b13`，修复前当前 Editor `b4e4abc`／SDK `e27230a`，修复后当前为同一提交组合加前一轮的两处 UI 修复。使用保存的三份 Release `test-support` 二进制，未启用 `bench-instrumentation`；驱动、模型和纹理一致，哈希已核对。没有修改产品代码来进行本次诊断。

18 个正式进程、14,760 次正式输入，全部通过现有 frame reporter、benchmark_done、唯一 end 和 lost_events=0 检查；每秒进程监测均未观察到编译器。保留用户授权的后台任务，不表示排除了其他 CPU／GPU 调度与帧相位变化。每种负载两轮，版本顺序交错；画布与变形器每种 30＋300 次，参数 30＋360 次，120 Hz。同一 Mao runtime、同一选取规则，Warp 固定为原报告中的 `5101be9b-3d35-45a7-8764-b8d62616ada4`。

以下为每进程 UI draw P95 的两轮中位数，单位 ms，不合并原始样本。

| 负载 | 原基线 | 修复前当前 | 修复后当前 | 修复后相对基线 |
| --- | ---: | ---: | ---: | ---: |
| 直接 Opacity | 3.682 | 3.624 | 3.354 | -8.9% |
| 平移 | 4.232 | 4.187 | 3.580 | -15.4% |
| 缩放 | 4.517 | 3.967 | 3.591 | -20.5% |
| 参数滑条 | 3.266 | 3.691 | 3.179 | -2.6% |
| Mesh 多顶点 | 3.716 | 3.903 | 3.507 | -5.6% |
| Mesh 单顶点 | 3.783 | 3.940 | 3.565 | -5.8% |
| Rotation | 2.783 | 3.400 | 3.322 | +19.4% |
| Warp | 2.419 | 3.080 | 3.365 | +39.1% |

Warp 两轮为基线 2.482／2.357 → 修复后 3.604／3.126 ms，Rotation 为 2.742／2.824 → 3.544／3.100 ms，均同方向上升。参数两轮接近原基线；平移、直接 Opacity 和 Mesh 存在轮间方向变化，不把中位数下降解释为稳定优化。

## 根因与代码位置

[根视图 render](../../../crates/editor/src/ui.rs#L98) 每次都调用 [refresh_menus](../../../crates/editor/src/menus.rs#L131)。菜单先遍历全部 command_state，再在 creation_reasons 中重新调用 authoring_command_capability，最后才比较 MenuState 是否相同并返回；相同状态没有避免前面的查询成本。

完整调用链：

```text
EditorView::render → refresh_menus
  command_state + creation_reasons
  → authoring_command_capability
  → validate_scene_object_creation_in
  → scene_creation_container / deformation_creation_container
  → organization_authoring_capabilities
  → SDK organization_capabilities
  → organization_delete_users → references_to（全模型扫描）
```

创建位置校验在 [scene_creation_container](../../../crates/editor-core/src/authoring/scene_creation.rs#L311) 和 [deformation_creation_container](../../../crates/editor-core/src/authoring/scene_creation.rs#L231) 中调用 organization_authoring_capabilities，却没有使用返回的能力字段。对于本次单选 Mesh／Deformer，8 个场景创建命令在 enabled 和 reason 两处各查询一次，按代码路径通常每次根 render 至少触发 16 次完整能力查询；菜单实际重建时还会查询 item 状态。这个次数是源码路径推导，没有作为动态计数器结果。

SDK 的 organization_capabilities 为计算 delete 字段调用 organization_delete_users，后者调用 references_to；references_to 遍历 Mesh、Transform、Binding 等集合。它是正确但较重的完整能力接口，创建位置查询把其中与当前目的无关的工作放到了逐帧路径。Document::authoring_capabilities 已有的版本缓存没有覆盖这里直接调用 SDK 的 organization_authoring_capabilities。

这几份 SDK 文件在 `b86b13` 与 `e27230a` 间无变化；Metal renderer、runtime 和 Editor worker 路径也无变化。因此本轮证据指向 Editor 新增的调用频率和重复查询，不能把开销归为 SDK 此函数实现自身变慢。代码历史定位到 `e7d6da6` 的组织树创建校验、`4602e0f` 的菜单／快捷键接入、`908ddbe` 的变形树创建扩展；这是源码历史定位，未做逐提交构建二分。

## CPU 采样与消融

对两份无诊断插桩的原生二进制分别做 5 秒、1 ms 间隔的 sample，采样工具运行跨度内的 benchmark ticks 全部属于 Warp；两次均无观察到的编译器活动。采样运行的时间指标不计入上面的正式性能对比。

| 采样归属 | 基线 | 修复后当前 |
| --- | ---: | ---: |
| Window::draw 内采样点 | 1081 | 1335 |
| 其中菜单刷新 | 15 | 207 |
| 其中菜单下删除依赖检查 | 0 | 188 |
| 其中菜单下 references_to | 0 | 185 |

这些是包含子调用的栈归属计数，不能相加，也不能把两次采样点总数差解释为耗时倍率。当前菜单 207 个点中 185 个在 references_to 内（约 89%）；两条上游分支分别来自命令状态和创建原因，确认了重复查询确实发生。

隔离源码副本使用同一份诊断二进制，只通过环境变量选择跳过菜单刷新或绑定操作行。共 19 个成功诊断进程、19,380 次正式输入，全部通过 frame reporter 与 trace 完整性检查；10 个进程观察到编译器，完整保留并标记。以下仅列第 3／4 轮两组双方均未观察到编译器的菜单配对。单位 ms；CPU 为线程 CPU 总和／同阶段窗口布局统计帧数，UI draw 为墙钟。

| 轮 | 负载 | 根 render CPU/帧（正常 → 跳过菜单） | 菜单 CPU/帧 | UI draw 均值 | UI draw P95 |
| --- | --- | ---: | ---: | ---: | ---: |
| 3 | Warp | 0.617 → 0.063 | 0.577 → 0.002 | 2.718 → 2.635 | 3.130 → 3.248 |
| 3 | Rotation | 0.736 → 0.043 | 0.699 → 0.001 | 2.853 → 2.415 | 3.200 → 2.762 |
| 4 | Warp | 0.680 → 0.036 | 0.631 → 0.001 | 2.738 → 2.008 | 3.062 → 2.318 |
| 4 | Rotation | 0.758 → 0.043 | 0.720 → 0.001 | 2.873 → 2.245 | 3.233 → 2.500 |

CPU 增量的消除方向一致；第 3 轮 Warp 的 P95 未下降，不能把单轮尾延迟改善率作为稳定收益。第 4 轮 Warp 的 build 均值 1.340 → 0.615 ms，Rotation 为 1.375 → 0.705 ms；prepaint、paint 没有同等幅度变化，compute_layout 仍约 0.02 ms。布局统计约 17 个 built views／帧、16 个新节点／帧，跳过菜单前后基本相同。这把主要增量定位在根视图构建期间的菜单查询，而非布局节点规模或求解算法。

Warp／Rotation 正式拖动阶段没有持续 Properties render 或行投影重建。第 2／3 轮单独去掉绑定操作行，菜单和根 render CPU 开销仍在；去掉菜单后才消除。新绑定按钮可以改变初始行布局，但没有证据表明它是当前逐帧增量的主要来源。参数与 Mesh 的共享 Select 重绘问题已由前一轮修复覆盖，不能把那一问题继续当作 Warp／Rotation 的根因。

## 修复方向与范围

建议先将创建位置校验中的完整能力查询改为所需的对象存在／类型和目标集合校验，避免为了创建位置扫描删除依赖；在菜单内复用同次能力结果生成 enabled 和 reason，并按文档已提交版本、选择和必要上下文缓存重查询。手势、焦点、Undo／Redo、播放及模态状态等动态菜单字段仍应及时刷新，执行动作时保留最终校验。跳过整个 refresh_menus 只是诊断对照，不是可交付的产品修复。

本次只新增报告与隔离诊断文件，未实施上述产品修复，原有两处 UI 修复与其他未提交工作保持原样。没有再次运行昂贵 Rust 套件。

上述根因解释的是剩余 UI draw 的主要 CPU 增量，不构成所有端到端指标的完整归因。例如参数的输入到呈现 P95 本次约 36.022 → 47.362 ms，而 UI draw 接近原基线；Warp 的 preview_gpu P95 也有明显差异。它们包含帧相位、队列和 GPU 并发调度因素，不能由菜单 CPU 数据直接归因为 GPU 算法退化或视为已经解决。完整逐轮指标、请求终态和缺失样本保存在 JSON；未测量 input-to-photon。

## 完整性与复现

两次失败尝试另行归档并排除：首次低层运行器未带原计划的固定 Warp 参数，默认对象遇到 singular parent；另一次在正式组件手势前未通过组件选择校验。两者没有混入成功结果。成功数据合计 18 个正式性能进程、19 个诊断进程、2 个 CPU 采样进程，均完成且无丢失事件。

- [完整机器可读结果](2026-10-24-basic-modeling-regression.json)
- [前一轮两处修复及验证](2026-10-24-ui-draw-fix.md)
- 本地原始目录：`target/basic-ui-regression-20261024/`，含二进制、构建日志、原始 trace、每秒噪声监测、CPU sample、诊断补丁和脚本。产品 Release 输出已恢复为修复后的原始二进制。

```sh
python3 target/basic-ui-regression-20261024/run_native.py
python3 target/basic-ui-regression-20261024/run_probes.py
python3 target/basic-ui-regression-20261024/sample_plain.py
python3 target/basic-ui-regression-20261024/summarize_native.py
python3 target/basic-ui-regression-20261024/analyze_probes.py
python3 target/basic-ui-regression-20261024/parse_samples.py
python3 target/basic-ui-regression-20261024/validate_traces.py
python3 target/basic-ui-regression-20261024/capture_evidence.py
python3 target/basic-ui-regression-20261024/write_report.py
```

脚本会跳过已有完整结果；重新采样应使用新目录并保存原始二进制／资产哈希。run_probes.py 包含四轮诊断／消融计划，原始 run 元数据与代码均保留。
