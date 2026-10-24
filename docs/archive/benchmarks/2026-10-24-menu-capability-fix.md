# 菜单能力查询修复与 Warp／Rotation 复测 — 2026-10-24

菜单中的重复能力查询和删除依赖扫描路径已修复；CPU 采样不再出现该菜单热点。但目前不能确认 Warp／Rotation 整体 UI draw 退化已完全消除：三轮 P95 中位数分别比原基线高约 12.1%／9.4%，轮间方向有波动。

## 修复内容

- [创建位置校验](../../../crates/editor-core/src/authoring/scene_creation.rs#L231) 改为 SDK handle 的存在／类型校验，避免调用完整 organization capabilities、重复扫描删除依赖。目标集合锁、Offscreen 约束和最终创建计划的校验保留。
- [菜单刷新](../../../crates/editor/src/menus.rs#L126) 每轮只为当前工作区中的创建命令查询一次能力，复用于 enabled 状态与禁用原因；菜单项也使用已经计算的 CommandState。结果只在本次刷新内复用，后续选择、锁定和历史变化会重新读取。
- [Core 状态接口](../../../crates/editor-core/src/commands.rs#L755) 接受本轮已经取得的能力结果，仍检查命令目标、工作区与生命周期条件；真正 dispatch 使用普通 command_state 重新验证。

新增 Core 和原生菜单测试，覆盖能力不可用／错误、工作区切换、过期文档目标、锁定后动作重新验证，以及菜单在锁定、Undo／Redo 和选择变化后的刷新。8 个定向过滤共 82 次测试执行通过，包含 81 个不同测试；格式检查与 git diff --check 通过。未运行完整 Rust 或 SDK 套件。

## 原生复测方法

| 版本 | Editor／SDK | 含义 |
| --- | --- | --- |
| 原基线 | `44accd1`／`b86b13` | 用户指定基线 |
| 修复前 | `b4e4abc`／`e27230a`＋前两处 UI 修复 | 本轮菜单问题仍存在 |
| 修复后 | 同一提交组合＋本次菜单修复 | 完整修复构建 |

使用保存的 Release `test-support` 二进制，未启用 bench-instrumentation 或诊断插桩。同一 Mao runtime、纹理、120 Hz、隔离配置与 benchmark 驱动；Warp 固定为 `5101be9b-3d35-45a7-8764-b8d62616ada4`，Rotation 沿用原选择规则。各版本四个变形器场景的对象、组件与影响顶点数逐进程一致，已核验日志。

为沿用原基线二进制的相同驱动，每进程仍先执行两个 Mesh 阶段，再执行 Warp／Rotation，各阶段 30 次预热＋300 次正式输入。三轮版本顺序为基线→修复前→修复后、修复后→基线→修复前、修复前→修复后→基线。最终共 9 个进程、10,800 次正式输入，其中 Warp／Rotation 为 5,400 次；下面的性能结论只针对这两个场景。

所有构建和定向测试在采样前完成。保留用户授权的后台任务，每秒记录编译器与高 CPU 进程；9 个正式运行都未观察到活动编译器。所有运行完成、frame reporter issues 为空、唯一 end、lost_events=0。未观察到编译器不代表排除了其他调度、GPU 并发、温度或显示相位影响。

## UI draw 结果

单位 ms，使用每进程 P95 的三轮中位数，不合并原始样本。

| 场景 | 原基线 | 修复前 | 修复后 | 修复后相对修复前 | 修复后相对原基线 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Warp | 2.512 | 3.303 | 2.816 | -14.7% | +12.1% |
| Rotation | 2.816 | 3.337 | 3.080 | -7.7% | +9.4% |

逐轮 P95：

| 场景／轮次 | 原基线 | 修复前 | 修复后 | 修复后相对原基线 |
| --- | ---: | ---: | ---: | ---: |
| Warp／1 | 3.287 | 3.303 | 2.882 | -12.3% |
| Warp／2 | 2.512 | 3.289 | 2.718 | +8.2% |
| Warp／3 | 2.330 | 3.333 | 2.816 | +20.9% |
| Rotation／1 | 2.757 | 3.290 | 3.080 | +11.7% |
| Rotation／2 | 2.868 | 3.337 | 2.596 | -9.5% |
| Rotation／3 | 2.816 | 3.442 | 3.581 | +27.2% |

Warp 相对修复前三轮均下降；Rotation 有一轮相对修复前上升。两者相对原基线都存在轮间方向变化，不能将中位数差值当成已经确认的稳定回归幅度，也不能宣称已完全恢复基线。P50 中位数：Warp 为 2.041／2.831／2.225 ms，Rotation 为 2.348／2.898／2.435 ms（原基线／修复前／修复后）。剩余差异尚未归因。

## 菜单热点验证

对完整修复后的原生二进制做 5 秒、1 ms 间隔的 CPU 栈采样；采样工具运行跨度内的 ticks 全部属于 Warp，未观察到编译器。该进程完整执行并通过 frame reporter，但其计时不混入正式性能对比。修复前引用上一轮同一 SHA-256 二进制的 Warp 采样。

| 栈归属采样点 | 修复前 | 修复后 |
| --- | ---: | ---: |
| Window::draw 内 | 1335 | 1192 |
| 其中菜单刷新 | 207 | 10 |
| 其中菜单下删除依赖检查 | 188 | 0 |
| 其中菜单下 references_to | 185 | 0 |

这些计数包含子调用，不能相加或当作耗时倍率。修复后的菜单路径未采到 organization_capabilities／organization_delete_users／references_to，源码也已移除相关调用；菜单本身只占本次 draw 内约 0.8% 的采样点。该证据支持重复查询热点已消除，不支持把所有剩余 UI draw 差异继续归因为原来的引用扫描。

## 构建与完整性

最终源码及二进制哈希见 JSON，并在测试、构建和报告阶段核对；完整修复后的二进制 SHA-256 为 `3cc5eb24ce39d641a7fc6930767196e231a6bca2df4a3fdd05d7e363dc4f56c0`。本轮发现共享构建缓存曾复用旧产物，已使受影响的本地 crate 强制重建并禁用 rustc wrapper，核验实际源码依赖路径后再采样。产品 Release 输出保持为完整修复的新二进制。

另保留了早期仅包含 Core 校验修改的 9 个进程／10,800 次输入，目录为 partial-core-only；该构建没有包含菜单侧复用，单独归档，不混入上述完整修复结果。没有按性能结果筛选或删除有效运行。

所有源文件与前一轮已有 UI 修复保持在工作区，没有提交。SDK 未修改。

- [完整机器可读结果](2026-10-24-menu-capability-fix.json)
- [上一轮根因分析](2026-10-24-basic-modeling-regression.md)
- 本地原始目录：`target/menu-capability-fix-20261024/`，含三份最终对照二进制、源码快照、补丁、逐轮 trace、噪声记录、构建日志、测试日志、CPU sample 与复现脚本。

```sh
python3 target/menu-capability-fix-20261024/checks.py
python3 target/menu-capability-fix-20261024/build_native.py
python3 target/menu-capability-fix-20261024/run_native.py
python3 target/menu-capability-fix-20261024/summarize.py
python3 target/menu-capability-fix-20261024/sample_plain.py
python3 target/menu-capability-fix-20261024/parse_samples.py
python3 target/menu-capability-fix-20261024/write_report.py
```

已有完整采样会跳过；新复测应使用新目录并核验源码、二进制与资产哈希。
