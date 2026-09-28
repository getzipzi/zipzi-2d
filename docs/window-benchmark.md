# 实际窗口交互 benchmark

新增日期：2026-09-28。本测试补充 `interaction_benchmark`（独立 Metal）和 `canvas_ui_cpu_benchmark`（GPUI CPU）。它在实际 macOS 窗口中运行原有后台预览生产器和 GPUI Metal 合成器，并把同一次请求关联到 drawable 的显示时间。

## 运行

```sh
./scripts/window-benchmark.sh \
  models/local/mao/runtime/mao_pro.model3.json \
  target/window-benchmark-mao 300 120
```

参数依次是模型、输出目录、每种负载的正式样本数、输入频率。支持 `model3.json` 和已保存的 Kasane 项目；后者可覆盖保存状态下的 dirty 查询。默认每种负载先预热 30 次，可用 `KASANE_BENCH_WARMUP` 覆盖。

脚本先检查 Mac 已解锁，再编译 Release，关闭 Metal HUD，使用输出目录内的独立配置，然后启动并激活测试窗口。运行期间保持窗口在待测显示器上、可见且有焦点，避免操作它或并行运行编译。失去焦点/隐藏会中止测试并在报告中标为失败。结束后测试进程自动退出，不保存模型更改，不影响另一个编辑器进程。

三种负载使用同一个窗口、模型和真实渲染流程：

| 负载 | 入口 | 覆盖 |
| --- | --- | --- |
| pan | GPUI 中键按下、移动、释放事件 | 输入分发、视口变化、异步预览、窗口合成 |
| zoom | GPUI 滚轮事件 | 缩放、遮罩缓存、实际画布分辨率 |
| opacity | `EditorCommand::PreviewOpacity` | 真实 SDK 编辑、场景求值、资源同步和合成；不包含 Slider 控件分发 |

定时器按目标频率发出事件；迟到后不会补发一串积压事件。逐次记录定时器迟到量，并统计实际输入间隔，不能把设置为 120 Hz 当作实际达到 120 Hz。当前自动负载不包含全量 SDK 参数/变形器动画；原有 `deform_all` 也仍然只是修改求值后的位置数组。

## 输出与统计

输出目录包含：

- `trace.jsonl`：原始边界事件，包含预热、启动和没有显示的请求。固定容量异步写入队列；满时不阻塞 UI，但会记录丢失数。
- `requests.csv`：每个预览请求的关联计时，以及没有产生请求的自动输入。
- `summary.json`：按负载统计 P50/P95/P99/最大值、样本数、请求结果、显示间隔及超预算数量。预热仍保留在 CSV 中，摘要排除预热。
- `environment.txt`：系统、设备、显示器配置和代码版本；`build.log`、`application.log` 保存执行日志。

```sh
python3 scripts/frame-trace-report.py target/window-benchmark-mao/trace.jsonl \
  --output target/window-benchmark-mao
python3 -m unittest discover -s scripts -p 'test_frame_trace_report.py'
```

报告使用 process-wide ID 关联输入、预览请求和 GPUI CPU 帧；使用纹理身份及其发布时间关联预览和窗口合成。地址复用按时间区分。同一个预览多次合成只采用第一次实际显示的时间；多个窗口提交可能共用同一个 CPU 场景，提交仍有独立 ID。GPU/显示回调到达次序不会改变关联。

`coalesced`（未开始就被更新请求取代）、`rejected`（场景/尺寸等已失效）、`accepted_not_composed`（UI 接收后没有进入合成）、`submitted_not_presented`（已提交但没有显示时间）、错误和未完成请求均保留。只对拥有对应边界的样本计算该阶段的分位数，同时列出其 count。不能只看显示成功样本的低延迟而忽略请求丢失比例。

trace 缺少结束记录、写入丢失、负耗时、GPU 失败、自动负载缺失或没有关联显示时间时，报告退出码非零，仍输出已有数据用于诊断。Debug 构建也会标记为不适合 Release 验收。

## 计时边界

| 字段 | 起止 |
| --- | --- |
| `input_handler_ms` | 同步 GPUI 输入处理；opacity 为直接命令处理 |
| `edit_dispatch_ms` | 输入入口 → 开始取场景，包含编辑和前置分发 |
| `scene_ms` | `preview_if_needed()`，包含缓存命中或 SDK 求值、快照/命中几何准备 |
| `mailbox_ms` | 放入最新请求槽 → worker 取出请求 |
| `textures_ms` | worker 开始 → 纹理/mipmap/catalog 准备完成 |
| `sync_ms` | 纹理准备完成 → 输出纹理创建、resize、模型同步完成 |
| `encode_ms` | Metal 编码开始 → commit 前 |
| `preview_submit_ms` | worker 开始 → preview commit 返回 |
| `preview_queue_ms` | preview commit 调用前 → GPUStartTime；包含 commit 调用成本 |
| `preview_gpu_ms` | preview GPUEndTime − GPUStartTime |
| `preview_wait_return_ms` | preview GPU 完成 → worker 等待返回；也可能包含结果通道背压 |
| `delivery_ms` | 发布纹理 → UI 消费结果 |
| `ui_wait_ms` | UI 接受该结果 → 对应 CPU 场景开始绘制 |
| `ui_draw_ms` | GPUI CPU 布局和绘制 |
| `drawable_wait_ms` | 调用 nextDrawable 前 → 获取 drawable |
| `compose_encode_ms` | drawable 就绪 → 合成 commit 前 |
| `compose_queue_ms` | 合成 commit 调用前 → 合成 GPUStartTime |
| `compose_gpu_ms` | GPUI 合成 GPUEndTime − GPUStartTime |
| `input_to_submit_ms` | 输入入口 → 窗口合成提交/显示请求返回 |
| `input_to_present_ms` | 输入入口 → 该纹理首次出现的 drawable presentedTime |

所有 CPU 边界使用 `CACurrentMediaTime` 的 host clock，与 Metal GPU/显示时间戳处于同一时间域。`presentedTime` 来自 Metal 回调内读取的属性，**不是回调执行时间**；返回 0 时不作为成功显示。参考 [Apple presentedTime](https://developer.apple.com/documentation/metal/mtldrawable/presentedtime) 和 [GPUStartTime](https://developer.apple.com/documentation/metal/mtlcommandbuffer/gpustarttime)。

8.33 ms 是 120 Hz 的一帧预算；输入到显示可能跨越多个显示周期。延迟超过 8.33 ms 不等价于掉了一帧，低于它也不证明长期稳定 120 fps。摘要中的新预览显示间隔是另一项指标。各阶段可能重叠，不能相加阶段分位数得到端到端分位数。

自动输入直接进入 GPUI，**不包含硬件输入、AppKit 原生事件投递或屏幕扫描/像素响应**。因此结果是应用输入入口到系统报告显示时间的延迟，不是 input-to-photon。计时和异步日志本身有成本；本次用于定位瓶颈，跨版本对比应保持同样的追踪开关与负载。

## 手动交互采样

仅启用追踪，不启用自动负载，即可记录用户真实操作从 GPUI 事件入口开始的流程：

```sh
KASANE_FRAME_TRACE="$PWD/target/manual-frame-trace.jsonl" \
  ./target/release/kasane-editor
```

正常退出后运行相同报告脚本。输出文件必须不存在，避免覆盖旧数据。异步操作或延迟执行的命令可能没有同步输入 ID，这些请求仍记录内部阶段，但不伪造输入到显示延迟。没有设置 `KASANE_FRAME_TRACE` 时不安装计时 sink，不读计时钟、不创建日志线程或注册 GPU 显示回调。

## 边线对照与验证

边线默认按独立子路径批量绘制。仅在追踪模式设置 `KASANE_TRACE_BATCH_MESH_EDGES=0` 可恢复逐边路径，设置 `KASANE_TRACE_HIDE_MESH_EDGES=1` 可隐藏边线作排除实验。

自动采样时，画布忽略非注入的鼠标移动，避免原生悬浮事件取消模拟中键拖动。手动追踪和普通编辑器保留原有行为。

Workspace 72 项测试、报告工具 8 项测试通过；默认忽略的 GPUI CPU 性能测试未重复运行。格式、差异空白和 GPUI 补丁一致性检查通过。
