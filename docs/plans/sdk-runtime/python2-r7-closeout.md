# R5/R7：发行复验与完整流程收尾

日期：2026-10-27。**R5/P5、简化 R6 与当前 macOS arm64 / CPython 3.14 范围的 Runtime R7 已完成。** 接续 R6 `b7faee0e`，修复与首轮验收提交为 `8a35de62`；本轮补齐实际窗口性能和干净修订上的配对发行复验。Proposal/Review 保持后置，Python P7 独立汇总不由 Runtime 编号替代。

## 本轮修复

- R6 增加交接说明后，安装指南达到 1,581 词，触发已有 D08 的 1,500 词限制。精简到 1,477 词并更新生成摘要；同时纠正 Local 不支持 GPU 的过时说明。
- 两个旧 Editor 断言未跟上此前已提交的行为：首次适配后视口不再随布局自动缩放，打开 Binding 清除参数焦点。更新断言，保留跨文档导航状态、初始适配及手工导航检查，没有回退这些产品行为。
- 共享 SDK 清单漏记 `MotionPreview::canonical_motion_output_bytes`。审查确认它是 Runtime 使用的输出预算估计，归为 host-only；其余差异为行号变化。共享清单现为 1,088 项，未读取退役 Python 包。
- 串联人工精修、保存、重开与导出时发现：IO 期间视口解绑并清除 GPU 图像，恢复后静止动画仍认为已请求过同一姿态，未再提交帧。恢复 DocumentSession 时设置动画请求屏障，保留播放时钟，重新请求图像并拒绝旧结果。新增 CPU 用例先复现失败，修复后复验。
- 原动画 fixture 故意保留不存在的 `test.wav`，用于缺失资源诊断。为完整导出提供带实际 PCM WAV 附件的变体，原 fixture 行为保持；导出的音频由安装版 Python `wave` 独立读取验证。

## 已通过的发行与回归

| 范围 | 本轮证据 |
| --- | --- |
| SDK 回归 | `./tools/test-sdk.sh regression`：83 个目标、1,016 项通过；2 个 GPU 和 2 个专用性能案例按入口约定另行验证 |
| Editor 回归 | IO 修复后再次运行 `./tools/test-editor.sh regression`：1,106 项通过；CPU 基准和缺少指定 Mao Zipzi 工程的案例共 2 项忽略 |
| Python 公共契约 | `./tools/test-sdk.sh python2`：95 项两后端行为、16 项完整调用恢复通过，正反静态类型通过 |
| 源码/wheel | 配对 sdist 在无 Git、无工作区引用的解包目录检查 40 个生成文件，重建正常纯 wheel、Native wheel 和 Release CPU host，隔离安装并运行完整 GPU recipe；12 个步骤通过 |
| 两后端结果 | R4、P3B、P4、R5 动画创作、独立 Preview、空间查询、GPU/inspection 的领域报告完全相同；运行模式字段分别为 Local/Connected |
| 离线文档 | 865 个符号、1,628 个 callable/property、2,066 个字段；D01/D02/D04/D07/D08 全部通过 |
| Runtime GPU | `--test observation -- --include-ignored`：实际 Metal 上 3 项通过，未把默认忽略计为通过 |
| IO 恢复及原生流程 | 修复后 45 项动画回归、扩展后的 `runtime_handoff_acceptance` 和原有 `runtime_ipc_acceptance` 通过；同一人工精修文档完成保存/重开、Motion3/runtime package/WAV 导出并再次显示 |
| 真实模型动画 | 指定主工作区的 Mao Model3，要求素材存在，原生 Animation acceptance 的五阶段实际 Metal 验收通过 |
| 正常 app 的真实模型创作 | 安装纯 wheel、正常 Release app、私有 endpoint/config，从无窗口导入 Mao（260 meshes、7 motions）；一个 Edit 改 mesh 和 Motion 参数 key（0→-15），独立 Preview 两帧、durable 保存、generation 更新后重开、完整 Motion 相等、Motion3 数值及 runtime package 导出、显示窗口后新客户端重连读取均通过。package 显式 `allow_loss=True`，本次 warnings/diagnostics 均为空；导出的 Model3 再通过五阶段原生 Metal Animation acceptance |
| 正常桌面产物 | Release app 内容检查、ad hoc 签名检查及 DMG 创建/挂载/签名/内容一致性检查通过 |
| 实际安装位置 | 临时安装到 `~/Applications/Zipzi Editor.app`，纯 wheel 在 `/tmp`、`-I` 下不传 host 路径且清除 host 环境覆盖，通过自动发现、两个进程并发冷启动、窗口重开、被杀客户端宽限清理、保存/宿主重启/重开；测试后清理临时副本，未改 `/Applications` 现有 app |
| 工具 | codegen 19 项、性能报告/隔离 6 项、共享清单 1 项、清单扫描器 3 项通过；测试域检查为完整且互斥的 17 SDK + 3 Editor 包 |

SDK 回归最初缺少 PurismCore 子模块而链接失败；初始化仓库固定的 `ad512766` 后完整复验通过。Editor 一次回归出现缩放初始化通知断言失败；该用例单独复验和随后完整回归均通过，原始失败日志保留，未删除断言。

正常发行物没有 `_test_control` / `_measure`。当前配对仍为 `zipzi` / `zipzi-native` `0.1.0a1`、`zipzi-python2-18` / `zipzi-runtime-r5-observation-1`；实际环境为 Apple M4、macOS arm64、CPython 3.14。不扩展其他平台支持声明。

## A01–A20 对照

既有通过证据沿用对应阶段记录，本轮回归和新增原生流程复验相关路径；不将所有历史场景描述为本轮逐一重跑。

| 编号 | 当前证据与边界 |
| --- | --- |
| A01 | 更新共享清单及 host-only 分类；能力范围见 Python CAPABILITIES，性能余项独立列出 |
| A02 | Runtime 路径/保存预留回归，安装版工程往返和桌面保存重启 |
| A03 | Runtime 共享历史/no-op/取消回归，真实双客户端桌面历史及 R6 原生 Undo/Redo |
| A04 | Python 两后端、安装 recipe、Runtime 候选失败与原子发布回归 |
| A05 | SDK 地址/身份、Python P3B 及保存重开后的 generation 检查 |
| A06 | 干净修订上的安装位置自动发现/并发仲裁/桌面重开/重启通过；当前纯安装客户端另跑 16 项连接边界（独立 test-support CPU fixture），覆盖慢启动、残留 socket、安全及不匹配拒绝 |
| A07 | 完整调用恢复 16 项及既有连接故障注入；不自动重放写入 |
| A08 | Runtime Job/取消/panic、Editor runtime 回归及 R6 执行槽拒绝清理 |
| A09 | `runtime_ipc_acceptance` 的真实手势 Busy、结束后的旧版本拒绝 |
| A10 | Runtime 事件/慢订阅回归，R4/R6 对象、资源、Timeline 与属性投影 |
| A11 | R5 两 Preview/播放/Physics 隔离回归和安装版 Preview recipe |
| A12 | 纯 wheel 无 Native，正常两 wheel 隔离安装和相同领域报告 |
| A13 | 既有原生 Runtime 窗口/后台任务生命周期回归；本轮补 IO 后静止动画恢复 |
| A14 | 安装版 P4/R4 资源往返、100 轮取消/替换与历史保留；实际 GPU 冻结纹理来源 |
| A15 | 安装版 Modeling/CPU sweep、大网格/预算负例、项目/格式往返 |
| A16 | 五对原生 122 项、十对实际窗口 29 项通过，共 151 项；各场景独立判断，阈值及工作量检查保持 |
| A17 | Runtime 预算/保留、Python Preview 释放及 R4 资源存活检查；RSS 范围随性能原始产物保留，不据单次峰值宣称长期内存稳定 |
| A18 | 实际安装 app 上正常客户端退出、被杀客户端候选宽限回收、宿主仍驻留 |
| A19 | R6 原生 Python→Editor→属性/Timeline 人工输入→新 Python 接续、原子 Undo、资源清理 |
| A20 | 合成原生窗口的人工精修/接续/持久化/导出通过；正常 app 无窗口创作真实 Mao、保存重开、Motion3/runtime package 导出及重连通过；实际桌面呈现性能单列 |

## 原生完整流程与性能

原生交接验收在生产库的 `test-support` 窗口中使用正常纯 wheel，扩展为同一文档的 mesh/内存纹理/Motion 批次 → 人工 Properties 和 Timeline 精修 → 新 Python 接续 → 保存回执 `durable` → 原 Session 重开及 generation 更新 → 校验名称/像素/完整 Motion/人工 key → Motion3 和 runtime package 导出 → 校验 WAV → Editor 再显示。正常 app 的安装/发现/生命周期使用上表独立生产产物验收，二者边界不混用。

R5 新增 `--read-batches` / `ZIPZI_RUNTIME_INTERACTION_READS=1`：在同一 Runtime 的另一 Session 连续运行四姿态 CPU、独立 Preview 和 96×96 GPU 观察，交替向两个原生 Metal 视口注入指针输入。每种负载先完成一个预热批次，然后持续执行至输入循环结束；报告保留原始延迟、工作步数、完成数和可见历史不变断言。

首轮计时因新增驱动在 worker 线程操作 Runtime、与 UI 读锁重叠而失败。驱动改为在 UI 线程准入/调度/接收完成，worker 仅执行 Work，保持与实际宿主一致；没有修改 Runtime 的重叠访问检查。首轮数据保留且不参与通过判定。

修正后的比较使用公平调度前 `4c7666bf` 与 `b7faee0e` 加本轮 IO 图像恢复修复，两边使用完全相同的新增驱动、vendor 和固定 PurismCore `ad512766`。重新构建并复制六个 Release 程序后才开始计时，二进制 SHA 与 dirty 来源逐个记录；未混用首轮或 Debug 数据。模型导入和首批预热在计时之前。

**五对 AB/BA、共 30 个独立 Release 进程全部完成：CPU 属性链 94 项、Timeline/导航/停靠调整 18 项、双视口与后台负载 10 项，共 122 项全部通过。** 沿用原 P50 10%、P95 15% 与 CPU 0.2 ms/展示 1 ms 的最大绝对裕量，未修改阈值。每种原生场景为 10 次输入预热、60 次测量；比较五个进程各自分位数的中位数，并保留全部原始输入和进程范围。

| 连续后台负载 | 当前输入到两个视口当前 Metal 绘制 P50 / P95 |
| --- | --- |
| 四姿态 CPU sweep | 0.806 / 2.084 ms |
| 独立 Preview 四目标 | 0.753 / 0.850 ms |
| 四姿态 GPU 观察 | 0.769 / 0.882 ms |

`/usr/bin/time -l` 的每进程最大 RSS 中位数：双视口全部负载基线/当前为 803.80/803.14 MiB，CPU 控制为 232.31/232.00 MiB，Timeline 为 782.33/781.61 MiB。该进程峰值接近基线；它包含模型、GPUI 和 GPU 分配，不等同于 Runtime 的逻辑预算，也不单独证明长期内存稳定。资源保留/释放继续由 R4/P4 的重复轮次和 Runtime 预算回归验证。

原生测试窗口的终点为当前 Metal 绘制，排除截图读回与系统合成器。此前锁屏阶段未将缺失桌面数据计为通过；下节记录解锁后的实际窗口持续播放和呈现测量。窗口指标来自原生呈现事件，不是物理 input-to-photon 测量。

## 2026-10-27 最终窗口与发行复验

先将实现及已有验收提交为 `8a35de62`，工作区干净后重建 baseline/candidate 的 Release 窗口程序、正常 app、配对 sdist/wheel、生产 CPU host 和 DMG。baseline 仍为 `4c7666bf`，vendor、固定 PurismCore、Mao 输入、120 Hz 驱动、窗口配置及诊断开关一致。原 122 项原生测量的实现和驱动摘要与 `8a35de62` 一致，校验后沿用原始数据，没有改写其旧构建身份。

实际窗口首五对中 27/29 项通过；单顶点拖动的 handler/scene P95 增量分别为 0.297/0.207 ms，超过原 0.2 ms 裕量。进程间范围明显重叠，两边输入均为 120 次；整进程指令数中位数约 475.33/475.66 亿，未观察到对应的工作量增加。保留首五对及失败报告，将**全部四个窗口场景一次性固定扩至十对**，不换二进制、不改采样/预热或阈值、不挑选进程。最终结论基于完整十对。

| 实际窗口场景 | 指标数 | 十对结果 |
| --- | --- | --- |
| 属性数值拖动 | 4 | 全部通过 |
| 连续颜色拖动 | 4 | 全部通过 |
| 单/多顶点、旋转和 Warp 拖动 | 16 | 全部通过 |
| 持续动画播放 | 5 | 全部通过 |

单顶点 handler/scene 的最终 P95 增量为 0.149/0.094 ms，均在原裕量内。播放的呈现间隔 P50/P95 两边均为 8.333/8.334 ms；当前求值 CPU P95 为 0.782 ms、渲染提交 CPU P95 为 0.607 ms、UI 绘制 P95 为 3.081 ms、入队到采纳 P95 为 4.935 ms。**实际窗口 80 个进程、29 项全部通过；连同原生 30 个进程的 122 项，共 151 项通过。** 两组进程数和计时边界分别保留，不做跨场景平均。

窗口每进程最大 RSS 的基线/当前中位数（MiB）：属性 458.10/461.15、颜色 451.18/450.72、变形 450.91/441.69、播放 445.09/438.43。存活对象与字节的有界性依据上述 Runtime/R4/P4 重复轮次，RSS 作为进程资源报告单列。

`8a35de62` 的正常配对发行 12 步通过且 `dirty_files=[]`；两后端领域报告一致，离线 D01/D02/D04/D07/D08 全过。正常 app 的安装位置发现/并发冷启动/窗口重开/被杀客户端清理/保存重启再次通过。Mao 无窗口创作、Preview、保存重开、Motion3/runtime package 导出及新客户端重连通过，warnings/diagnostics 为空。完整源码包校验 2,399 个文件；app 与 DMG 内容及 ad hoc 签名通过。

收尾另纠正安装指南和仓库说明中“Editor playback 尚未接通”的过时文字，重新生成客户端来源摘要。该收尾提交仅改变文档/说明摘要，SDK、Runtime、Editor 逻辑及测量驱动保持与 `8a35de62` 相同；最后配对发行及正常 app 的实际提交身份、源码/产物 SHA 和复验结果由 `target/r7-final-release/verification.json` 记录。

## 产物与阶段结论

原始日志在 `target/r7-*`；源码/wheel/隔离安装在 `target/python2/r5-r7-closeout/distribution-final/`，比较程序、输入摘要、模型文件哈希和 DMG 在 `target/r5-r7-closeout/`。原始失败日志与修复后的日志分别保留，计划目录不提交二进制、模型或 JSON 测量数据。

最终窗口、干净修订的发行包、安装复验和正常 app 真实模型产物在 `target/r7-closeout-8a35de62/`；最终说明摘要对应的配对发行在 `target/r7-final-release/`。本阶段按现有支持范围核销，不扩大平台声明，不重新启用模型/服务身份门禁。正式公开发布、其他平台和后置 Proposal/Review 仍按各自需求处理。

真实模型流程可用安装纯 wheel 的解释器重跑（输出目录必须不存在，模型需含可解析参数轨道的 Motion）：

```sh
/absolute/venv/bin/python -I crates/zipzi-python2/tests/installed_model_workflow.py \
  --host "/absolute/Zipzi Editor.app/Contents/MacOS/zipzi-editor" \
  --model /absolute/mao_pro.model3.json --output /absolute/new-output
```
