# SDK Runtime R0 实施记录

日期：2026-10-23。工作范围：配对工作树中的 R0 清单、可运行 fixture、预算和基线。
R0 准备产物已交付；R1–R7 尚未实现。本文不把目标接口、现有 standalone 测试或性能工具视为连接验收。

## 配对基线与工作树

| 仓库 | 分支 | 起始 revision | 本阶段修改 |
| --- | --- | --- | --- |
| zipzi-editor | `codex/sdk-runtime` | `73c414d5c1361b96d9b2c6fd4e20f1ae47ba601f` | 静态迁移清单、生成工具、阶段记录和计划状态 |
| zipzi-2d | `codex/sdk-runtime` | `26afe00939ddae59a38ee96a6c69618aa7fcb063` | 完整源接口清单、默认配置、合成 fixture、Release CPU 探针、采集/校验工具 |

两个工作树位于同一父目录，保持 Editor 原有 `../zipzi-2d` 路径依赖；未改变
Cargo manifests/lockfile 或运行期行为。起始两个主工作区均干净。实施期间原 Editor
主工作区另有动画/UI 工作及并行编译出现；本阶段未复制、覆盖或提交那些变更。
基线采集不包含这些后续变更；合并时已整合主分支的动画/UI 提交至 `ada2599`，
并重新生成迁移清单。本文的性能与行为基线仍对应表中的起始 revision。

SDK R0 交付提交为 `ec60e46268eafaf32861169350b0acc719743664`（含
`49969dc49a11f6c972b9c6aa63526f6ad22097c8` 的清单/CPU 工具提交）；Editor 对应
交付为包含本记录的 `codex/sdk-runtime` 提交。正式 crate 源码、Cargo manifests 和
lockfile 在 R0 交付时与表中的起始 revision 相同；R0 新增的是工具、示例 fixture
和实施证据，随后合入的主分支动画/UI 改动不计为 Runtime 实现。

采集时的工作目录（合并后清理）：

```text
/Users/liangbohao/.codex/worktrees/sdk-runtime/zipzi-editor
/Users/liangbohao/.codex/worktrees/sdk-runtime/zipzi-2d
```

清理前将原始数据完整复制至主工作区的同名 `target/runtime-r0*` 目录：
Editor 的 `target/runtime-r0-native/` 与 SDK 的 `target/runtime-r0-clean/`、
`target/runtime-r0-baseline/`、`target/runtime-r0/` 均保留。原始报告中的采集路径
继续用于来源追溯；本记录内相对路径可在各仓库主工作区复核。

机器为 Mac mini / Apple M4（10 核 CPU、10 核 GPU、16 GiB 内存），Metal 4。
显示器为 3840×2160、逻辑 1920×1080、120 Hz。工具链为 Rust 1.99.0
`b940084d7` / aarch64-apple-darwin / LLVM 23.1.1、uv 0.12.21、CPython 3.14.7。
计时只采用 Release；功能测试采用 Debug。完整环境、来源文件 SHA-256、模型文件
SHA-256、工作区状态和每进程原始数据由采集工具保存，不记录设备序列号。

## A01 接口清单

SDK 仓库的 `docs/runtime/api-inventory.json` 为逐方法/类型/意图清单，
`docs/runtime/README.md` 解释来源、生命周期、批量形式、历史效果、后台执行和验收。
其输入包括全部 SDK 分模块实现、SDK 重导出的动画实现、Python facade、原生 stub、
当前公共导出和旧 Python 清单。每条 API 均有分类，没有把未绑定的方法排除出创作范围。

当前共 952 条 callable/record 记录：502 条 Rust SDK/重导出方法，450 条 Python
方法/记录；另附 20 个共享领域意图的实际定义、131 个公开输入/结果类型、31 个 Native
类型及原生导入点。分类为 748 条需要连接、167 条本地值、34 条本地 helper、3 条宿主专属。
所有条目的连接实现状态仍为 `not-connected`。同名 facade 路由只是调查线索，
旧清单的绑定测试只作为旧证据，不把它们计入 ConnectedBackend 已通过项。

旧 `python-coverage.json` 只有 123 条，检查器只读部分文件，不覆盖新分模块 API。
新清单补上 binding/scene/organization/reference/resource/document 与动画领域意图，
不能把私有 PyO3 `Command` 直接升格为协议。原生 Plan/Address/Token/Preview 和
异常类型的迁移，以及 Python `_call`、`commit finally`、`__exit__` 的失败/Busy/
结果未知区分已写入 SDK 侧说明。

## Editor 正式写入与生命周期迁移表

[`editor-migration.json`](editor-migration.json) 在整合主分支后记录 72 个文件、692 个会话/发布/
IO/生命周期引用，包括读操作、测试和注释；它是保守源索引，不把命中文本直接等同于
正式写入。采集基线时为 70 个文件、669 个引用。逐领域归属如下，R2 完成时需要同时关闭此表与对应行为测试。

| 当前入口 | 正式状态/副作用 | 迁移归属；留在 Editor 的状态 |
| --- | --- | --- |
| `document.rs` new/open/import/save、`document/transfer.rs` | 正式 AuthoringSession、工程身份、替换 generation、保存资源重定位 | Runtime 注册表和 Job；视图保留 import 展示名、投影 |
| `document/edits.rs`、`document/properties.rs`、`authoring/expression.rs` | owned candidate、属性/控件/Expression 手势、commit_to | Runtime Edit + 交互占用；鼠标、焦点、Esc、可恢复 preview 策略留在视图 |
| `authoring/publication.rs`、`object_state.rs`、`timeline_document.rs`、`parameter.rs` 的 event drain | 当前多处独立消费 SDK 事件及更新缓存 | Runtime 唯一 drain；Editor 统一按事件回执更新索引、选区、纹理、动画和画布，避免双重更新 |
| `authoring/*.rs` 域操作、`sorting.rs`、`parameter_group.rs`、`parameter_export.rs` | Scene/Binding/Organization/Pose/Physics/Reference/Resource/Document Plan 与提交 | SDK 定义规则；Runtime 路由及对象生命周期；UI 导航/能力展示留在视图 |
| `parameter.rs` | 参数建立/替换、preview 值校正、直接 commit_to | 正式写入 Runtime；用户参数采样是独立 Preview，不进入历史 |
| `mesh_generation.rs`、`authoring/resources.rs` | remesh、内存图片准备/替换、后台结果版本校验 | SDK 保留算法与校验；Runtime 资源预算/Job/来源；生成选项与工具交互留在 Editor |
| `timeline_document.rs`、`timeline/`、`animation/resources.rs` | Motion 创建、曲线候选提交、动画资产变化 | Runtime Edit/Plan/事件；播放钟、选区、展开状态、视口仍独立 |
| `document/edits.rs` Undo/Redo | 同一 SDK 历史、回执、对象身份校正 | Runtime 串行历史；文本输入 Undo 焦点规则保留在 Editor |
| `session.rs`、`workspace.rs`、`workspace/entry.rs` | DocumentSession 当前同时持有视图和正式 Document；save 移交整份 Session | 拆分 Runtime 正式会话与 DocumentView；工具/多视口/选择/面板不移入持久模型 |
| `workspace/io.rs` | 已开路径、在途 load、Save As 目标、归还会话、关闭与退出 | Runtime 统一已打开和在途路径保护；成功切换身份/失败保留旧身份/终态释放预留 |
| `editor/src/project.rs` save/load 回调 | `finish_save` / `finish_load` 位于弱视图 `update_in` 内 | 应用 Host 先归还会话并登记终态，再通知仍存在的视图；无窗口不自动弹窗 |
| `editor/src/lib.rs` 窗口关闭/退出 | 最后窗口关闭调用 `cx.quit()` | Host 常驻；窗口仅 detach；显式退出统一处理脏状态、任务和连接 |
| `dev_session.rs`、benchmark/test fixture | 独立创建/加载 Editor 状态 | 共用正式入口和 endpoint；保留测试控制能力，不让开发脚本成为发行启动器 |

当前 `find_path` 同时查询已开文档和 saving.path，`loads` 另存 Open 预留，
`ensure_available_path` 检查两者；它们需要整体迁入 Runtime，不能只迁每会话 Busy。
`project_identity` 已规范化目录/manifest 和存在祖先的符号链接，R1 必须复用同等规则。
取消 IO 请求不能提前释放目标预留；持有旧路径直到 Save As 成功终态。

## 默认预算与执行边界

可机读默认配置在 SDK `docs/runtime/defaults.json`；尚无 Runtime 代码执行这些限制。
选择依据为本机 16 GiB、合成/真实 fixture 的内容/快照/候选/图片负载，以及已有 SDK
每会话 50 步/256 MiB 历史限制；这是可配置的首期保守上限，不是吞吐量承诺。

| 项目 | 冻结默认值 |
| --- | --- |
| 客户端 / 托管会话 | 8 / 32 |
| 候选 | 每客户端每会话 1；每会话 4；宿主 16；总计 512 MiB |
| Plan / Preview / Snapshot | 每客户端 64 / 4 / 8；各自总负载 128 / 256 / 256 MiB（Snapshot 与 Proposal 共用） |
| Proposal | 每会话 4，受上述快照预算约束；R6 才启用 |
| 正式任务 | 每会话独占 1；宿主同时昂贵任务 2；队列总计 32、每客户端 8；控制队列单独每客户端 16 |
| 宿主可计量 CPU 负载 | 总计 2 GiB；所有子预算同时受总上限约束；不是 RSS/GPU 硬限制 |
| History / 资源 / blob | 宿主总计 512 / 768 / 512 MiB；SDK 每会话历史仍限 256 MiB；单张准备图片 256 MiB |
| 控制帧 / blob 分块 / 单 blob | 16 / 1 / 256 MiB；每客户端最多 64 条待处理消息且不超过 32 MiB |
| 事件 | 每 Session 512 条或 8 MiB；注册表 256 条或 1 MiB；任一先到则要求 resync |
| 去重结果 | 每客户端最多 1024 条或 8 MiB、宿主 64 MiB、300 秒窗口；活跃操作不可因淘汰变成新请求 |
| 断连 / 释放 blob 宽限 | 30 / 30 秒；进入提交点的操作仍投递终态 |
| Python / 启动等待 | CPython >=3.14,<3.15；15 秒；超时不判定宿主死亡，不二次启动 |

总计量包含正式内容、历史、候选、快照、Plan、资源、blob、队列、事件、请求结果和
Preview CPU 缓存；同一 backing allocation 按身份计一次，无法可靠去重的估计保守计费。
单项预算不叠加成额外额度。事件/结果大负载以预算内 blob 引用承载；记录/结果容量必须
在有副作用前预留。去重使用单调 RequestId 和已确认/淘汰水位；过期 ID 明确拒绝，
同 ID 不同摘要拒绝，Busy 后重试用新 ID。GPU 和 renderer 自有缓存须另计并受设备预算
约束；当前阶段不声称它们与 Observer 已共享所有纹理。

内存图片的当前持有/清理、RGBA+懒 PNG、取消/历史淘汰不回收资源表，以及快照对磁盘
资源未固定字节的缺口，已在 SDK R0 说明中落实到 R4 的统计/回收/资源固定原语。
R1 的候选估计限制不能冒充 A17 底层资源已回收。

后台清单：open/import/save/export、图片读取/哈希/解码/编码、重网格/拓扑与昂贵绑定
转换、长动画 seek/批量 CPU 求值/观察都从固定来源或 Busy 所有权移交执行。commit /
Undo/Redo 包含验证、变化总结和历史预算，不默认视为短操作；尚未拆 prepare/publish
的昂贵调用使用 Busy Job。R2 按测得的工作量保留已验证短调用，不对每次鼠标输入复制
全工程，不持有一个跨脚本或跨长任务的全局 Mutex。正式命令不合并；控制、取消、UI
任务与批量观察公平调度。

原计划性能门槛保持：CPU P50/P95 允许增量分别为 baseline 10%/15% 与 0.2 ms
中的较大值；输入到显示的绝对噪声裕量为 1 ms；R2/R5/R7 至少 5 对 baseline /
candidate 交替进程，分别报告各进程 P50/P95 的中位数与范围。没有因首轮噪声扩大阈值。

## Endpoint 与宿主定位（R3 待实现）

所有桌面、开发和 Python 入口使用同一用户/显式 profile 身份，默认 `default`，
不把 SDK build、工程路径或是否有窗口写进身份而静默产生另一宿主。
macOS 默认目录 `/tmp/zipzi-runtime-<uid>`，0700 且拒绝非本用户/符号链接目录；
socket 为 `<profile>.sock`，锁为 `<profile>.lock`，权限 0600。profile 限 ASCII
`[a-z0-9_-]{1,24}`，保证 Unix socket 路径长度；显式 `ZIPZI_RUNTIME_DIR` 覆盖须
是私有绝对目录且所有入口共用。发现记录仅提供诊断，成功监听后的严格握手才表示就绪。

冷启动持有同一个 endpoint 的排他 owner flock；启动者将已锁定文件描述符传给宿主，
成功 spawn 后关闭自己的副本，宿主接管并持有到退出，防止启动者退出与慢启动的空窗。
桌面直接启动也获取同一 owner lock。已有所有者时只等待/尝试握手；15 秒后只报未就绪。
协议不匹配直接返回不兼容，不启动替代宿主。只有取得 owner lock 且确认无活跃宿主后
才清理残留 socket；监听前失败退出释放本次锁，允许后续重试。此为 R3 契约，尚未实现。

可执行文件按顺序定位：显式绝对 `ZIPZI_EDITOR_HOST`；当前安装目录/调用者配置的
`.app/Contents/MacOS/zipzi-editor`；macOS 以现有 bundle ID `dev.zipzi.editor`
查询已安装 bundle。无法唯一定位则返回明确错误及显式配置方式，不使用 cwd 中偶然
存在的 Debug 二进制。开发脚本设置配对构建的显式路径；发行验收必须使用发行包。

## 测量与验证记录

实施时先运行的探索性 CPU 测量包含 5 个合成 fixture 和 5 个 Mao 进程，功能校验均完成。
该轮期间发现其他主工作区编译，故其时延不作性能门禁证据；原始数据保留于 SDK 工作树
`target/runtime-r0-baseline/`。采集器随后增加编译活动/源码漂移/模型漂移检查；正式可复核
基线及最终测试结果另列于同目录的 `r0-results.md`。

R0 源清单检查不是静态迁移已完成的证明；standalone 或 Editor 旧路径通过也不是 A02–
A20 Runtime/Connected 的通过。R2 才进行窗口生命周期变更后的 native 时延和像素比较。

可复用命令（分别从各自工作树根运行）：

```sh
# SDK
uv run --locked python tools/runtime_inventory.py --check
uv run --locked python -m unittest discover -s tools -p test_runtime_inventory.py
uv run --locked python tools/runtime_baseline.py target/runtime-r0-clean \
  --model /absolute/path/to/mao_pro.model3.json
cargo test --locked -p zipzi-sdk --test integration -- vertical_slice:: handles:: history_budget::

# Editor
python3 scripts/runtime-migration-inventory.py --check
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration workspaces::
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked --workspace --features zipzi-editor/test-support --lib headless_tests::ui_performance
# R2 使用同机/同模型/同窗口脚本完成成对比较；此处只保留可复用入口。
uv run --project ../zipzi-2d --locked python scripts/property-chain-benchmark.py /absolute/MODEL target/runtime-r2-chain --rounds 5
./scripts/timeline-benchmark.sh target/runtime-r2-timeline 600
```
