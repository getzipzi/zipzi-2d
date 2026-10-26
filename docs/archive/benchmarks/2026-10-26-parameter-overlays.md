# 参数拖动与全模型顶点：原生 UI 开销归因 — 2026-10-26

> **呈现指标更正：** 后续复核发现，旧版报告按 Metal 纹理地址关联模型帧，会把复用该地址的辅助层纹理误认成旧模型帧。因此下文涉及模型帧率及其关联延迟的结论已撤回，保留原始数据供审计；独立 CPU 分段、工作次数和全部窗口绘制时间仍有效。修正与复测见[更正报告](2026-10-26-selected-ui-release-recheck.md)。

已在当前 `mao_pro.zipzi` 的磁盘副本上复现开发构建的明显掉速：未选中模型时，新预览呈现频率的三轮中位数为 **109.61 Hz**；全选显示顶点后为 **32.13 Hz**。保持全选但完全跳过辅助层绘制，仍然只有 **36.40 Hz**。主要瓶颈是 UI 线程重新计算可编辑状态，尤其是 `mesh_blend_target` 为每个对象反复构建全部参数的 HashMap；状态提示查询还会生成整份顶点、边和控制器数据。GPU 顶点绘制和纹理创建不是这次骤降的主要来源。

Release 同一工作负载保持约 118 Hz，全选使 UI draw P95 从 3.93 ms 增至 5.00 ms；没有复现开发构建同等幅度的掉速。因此开发构建放大了这条额外 CPU 链路的成本，不能将 32 Hz 当作 Release 的性能结论。本次只增加诊断、测试驱动和报告，没有实施性能修复。

## 模型、环境和方法

- 从开发会话 `target/dev-session-zipzi-editor/session.json` 确认当前项目为 `/Users/liangbohao/mao_pro.zipzi/project.zipzi.json`。复制项目和纹理到独立目录，逐文件校验 SHA-256；测试使用副本，不包含运行中 Editor 未保存的修改。
- 文档有 260 个 Mesh、175 个 Transform、128 个 Parameter、124 个 BlendShape binding、7 个 Glue。全选夹具选择 150 个可见且未锁定的 Mesh，与从空选择开始框住整个模型的对象选择结果对应；没有选中每个单独顶点。叠加层为 5,159 个顶点、12,202 条网格边、2,825 条边界线。
- 所有进程拖动同一个真实滑条 `Angle_X`，范围 −30～30，使用既有 GPUI 指针派发路径。每进程 30 次预热、360 次正式输入，目标 120 Hz；结束时确认 93 个 Mesh 的位置发生变化，每进程求值次数均为 385，避免空负载。
- Apple M4，10 核 GPU，16 GiB，macOS 27.0.1；CB27U1 3840×2160，逻辑 1920×1080，120 Hz。默认独立窗口，模型预览为 941×1312 物理像素，关闭 Metal HUD。
- Release 和开发构建均启用 `test-support,bench-instrumentation`，各保存一份不可变二进制。正式对照关闭详细 Capture 和 GPU 观察器；详细归因另开进程。开发构建使用仓库默认 dev profile，是诊断数据，不参与 Release 验收。
- 项目 Release 五种条件各三轮，开发构建三种条件各三轮；偶数轮反转顺序。另有六个 Release 详细进程、三个开发详细进程，以及一个不带系统采样的开发详细复核进程。Mao runtime 导出模型的 15 个对照进程作为补充保留。
- 每进程使用独立配置和私有 Runtime endpoint，防止启动被转交给已有 Editor。用户暂停其他编译；驱动每秒记录进程快照，49 个完整进程均未检测到 Cargo、rustc、Clang 或 Xcode 编译。

下面主表均为三个进程各自指标的中位数，不合并原始样本。UI draw 是布局和绘制的墙钟时间；新预览 Hz 统计新模型纹理实际呈现，不等同于窗口刷新 Hz。嵌套阶段的 P95 不能相加。

## 当前项目：正式对照

| 条件 | Release UI draw P95 / ms | Release 新预览 Hz | 开发 UI draw P95 / ms | 开发新预览 Hz |
| --- | ---: | ---: | ---: | ---: |
| 未选中，不显示模型顶点 | 3.931 | 118.33 | 3.774 | 109.61 |
| 全选，显示全部顶点和辅助线 | 5.000 | 117.99 | 9.270 | 32.13 |
| 保持全选和 CPU 构建，完全跳过辅助层绘制 | 4.344 | 118.00 | 7.551 | 36.40 |
| 保持全选，只绘制顶点 | 4.662 | 118.00 | — | — |
| 保持全选，只绘制辅助线 | 4.851 | 118.33 | — | — |

开发构建每轮新预览 Hz：

| 条件 | 第一轮 | 第二轮 | 第三轮 |
| --- | ---: | ---: | ---: |
| 未选中 | 93.88 | 118.00 | 109.61 |
| 全选显示 | 32.13 | 31.99 | 32.25 |
| 全选但跳过绘制 | 36.40 | 36.56 | 36.21 |

全选显示时窗口呈现约 62.91 Hz，但新模型预览只有 32.13 Hz，不能用窗口刷新频率代替模型更新频率。跳过绘制只恢复约 4 Hz，全选相关的 CPU 成本仍在；这比单纯观察 GPU 利用率更能区分主要来源。未选中条件存在输入和显示相位波动，逐轮值保留，不把小幅 Hz 差异解释为收益。

## 函数级归因

Release 表列来自两次详细进程的 P95 中位数。开发表列来自额外的、不带系统 `sample` 的详细复核进程；计时本身会增加成本，应与正式对照分开解释。

| CPU 工作段 | Release P95 / ms | 开发 P95 / ms |
| --- | ---: | ---: |
| 每次 `EditAccess::build`：重算全选对象可编辑状态 | 1.490 | 10.074 |
| 其中 `mesh_blend_target`，按一次快照内全部调用聚合 | 1.159 | 8.260 |
| 状态提示查询所构建的完整 overlay | 1.665 | 11.182 |
| 画布内容所构建的完整 overlay | 0.185 | 1.206 |
| GPUI 辅助线构建和绘制提交 | 0.492 | 0.951 |
| GPUI 控制点/标记遍历和绘制提交 | 0.119 | 0.278 |
| Metal 辅助层单 pass 的 CPU 工作 | 0.229 | 0.679 |
| 其中 Metal 纹理创建 | 0.045 | 0.065 |

开发复核进程中，355 次可编辑性重建累计耗时 3,513.97 ms；其中 `mesh_blend_target` 累计 2,897.03 ms，占 **82.4%**。这个占比来自同一进程的累计嵌套时间，未用两个 P95 相除。

具体链路为：

```text
新参数预览 → 新 CanvasScene
  → receive_viewport_frame / sync_viewport_notice
  → Editor::overlay
  → ToolController::edit_access：scene 指针变化，缓存失效
  → EditAccess::build
  → ControlEdit::describe / mesh_drag_weight
  → mesh_blend_target
  → 遍历全部 128 个参数，clone 参数 ID，collect HashMap
```

一次重建会对 150 个 Mesh 做描述查询，对可写 Mesh 再查拖动权重。复核进程有 **95,186 次 `mesh_blend_target` 调用**，即每次新预览约 268 次；每次都重新整理相同的 128 个参数。即便 Mesh 没有相关 BlendShape，也先构建参数表，再检查绑定。

另外，状态提示实际只读取 `overlay.unavailable`，却调用完整 overlay 构建。355 张新预览对应 **710 次提示 overlay 构建**，还另有 **689 次画布 overlay 构建**；这重复遍历顶点和边。在 Release 下画布调用较易合并为约一次/输入，开发构建更容易形成额外绘制。

系统调用栈采样进一步确认相同热点：`sync_viewport_notice → overlay → EditAccess::build → mesh_blend_target → HashMap::from_iter / hashbrown::insert`，同时可见字符串 clone。带系统采样的那次进程 UI draw P95 为 16.61 ms，明显受诊断影响，**没有拿它作为主表或上面的开发阶段耗时**；后续无系统采样复核为 9.51 ms，与正式三轮的约 9.27 ms 接近。

## GPU 和资源创建

直接读取辅助层 Metal command 的 `GPUStartTime/GPUEndTime`，由单独诊断线程等待完成；UI 线程不等待 GPU。

| 独立辅助层 pass | Release GPU P50 / ms | Release GPU P95 / ms | 开发复核 GPU P50 / ms | 开发复核 GPU P95 / ms |
| --- | ---: | ---: | ---: | ---: |
| 网格线 | 0.348 | 0.486 | 0.556 | 4.426 |
| 顶点 | 0.224 | 0.394 | 0.256 | 3.829 |

开发构建 GPU 尾部确实更高，因此不声称 GPU 完全无成本；但跳过整个绘制链路仍然只有约 36 Hz，结合约 10 ms 的可编辑性重建，足以确认本次主要瓶颈在 CPU。纹理创建 CPU P95 只有约 0.065 ms，优先重构纹理分配不足以解决这次骤降。

## 后续优化顺序

1. 每个场景/参数版本共享参数查询表；合并对象描述和拖动权重查询，避免每个 Mesh 重复整理全部参数。
2. 状态提示使用独立的限制查询，不为一条提示生成完整顶点/边快照。
3. 将可编辑状态、选择状态、拓扑和动态坐标分开失效，按参数实际影响的对象更新。
4. 再处理重复画布快照、GPUI 辅助线、GPU buffer/纹理复用。它们有成本，但优先级低于上面已测出的热点。

## 完整性与复现

49 个进程共 17,640 次正式输入，全部有完整结束标记，丢失事件为 0。Release trace 的报告 issues 为空；开发进程只保留预期的“debug build，不用于 Release 验收”提示。原始请求中的拒绝、未合成和无请求状态均保留在 JSON，没有只筛选已呈现请求。15 个报告工具定向测试通过，Release 和开发构建通过；未运行昂贵的全套 Editor/SDK 测试。

- [机器可读报告](2026-10-26-parameter-overlays.json)：各进程指标、跨轮中位数/范围、终态、CPU/GPU 阶段、二进制/模型哈希及源码 provenance。
- 原始数据：`target/parameter-overlay-benchmark-20261025/`。包括已保存二进制、构建日志、模型副本及校验、每进程 trace/配置/Runtime endpoint 记录、审计、后台进程快照、系统 sample、与记录哈希匹配的驱动源码。目录日期为任务开始日期。
- 驱动：[parameter-overlay-benchmark.py](../../../tools/benchmarks/parameter-overlay-benchmark.py)；说明：[benchmark guide](../../../tools/benchmarks/README.md#whole-model-parameter-overlays)。

```sh
cargo build --locked --release -p zipzi-editor --features test-support,bench-instrumentation
python3 tools/benchmarks/parameter-overlay-benchmark.py \
  --binary target/release/zipzi-editor \
  --model /absolute/path/to/model-copy/project.zipzi.json \
  --output target/parameter-overlays-new --rounds 3

cargo build --locked -p zipzi-editor --features test-support,bench-instrumentation
python3 tools/benchmarks/parameter-overlay-benchmark.py \
  --binary target/debug/zipzi-editor \
  --model /absolute/path/to/model-copy/project.zipzi.json \
  --output target/parameter-overlays-dev-new \
  --cases unselected selected no_paint --rounds 3 --allow-debug
```

必须先完成构建，再运行保存的二进制；不在原有结果目录覆盖 trace。自动输入从 GPUI 分发开始，不覆盖硬件/OS 输入送达或屏幕像素响应。结论覆盖该项目磁盘副本、Angle_X 和上述窗口尺寸，不能外推为所有参数、模型和视口尺寸的精确耗时。
