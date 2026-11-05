# 2026-11-05 Windows 原生画布覆盖层首轮验证

分支 `codex/warp-viewport-display`，源 HEAD
`362e46eb83f9ecdcb8df145cd0d2bbe9e7fbc437`。本轮验证 Windows D3D11
统一覆盖层、DX12 模型预览交接及相关视口交互；未发现 GPU 像素断言或窗口渲染错误。
未将本轮结果视为完整 Windows 验收：完整画布像素对照、其他 GPU/驱动和下述挂起用例仍未完成。

## 改动范围与环境

重点阅读最近五个提交：未选中 Warp 外框显示/拾取，手柄尺寸与命中规则统一，
MOC3/CMO3 旋转单位修正，以及 `CanvasDrawList` 在 Metal、D3D11、GPUI 回退间共享的
64 字节实例。Windows 使用 D3D11 deferred context、4x MSAA、解析 marker 覆盖率，
发布不可变 surface；模型继续通过按 GPUI adapter LUID 匹配的 DX12 共享纹理展示。

- Windows 11 Pro，build 26300；AMD Ryzen 5 5600GT，约 32 GiB RAM。
- NVIDIA GeForce RTX 5060，驱动 `32.0.16.1742`；系统还列出 GameViewer 虚拟显示适配器。
- Rust/Cargo 1.95.0，uv 0.13.6，CPython 3.14.10。
- Release 特性：`test-support,bench-instrumentation`；窗口正式采样关闭详细计时。
- 固定二进制 SHA-256：`c50ea2ad9bd39c40eae1a8b144e51b6493b69938f712f1c34f97613a3d4429df`。
- 本轮源码修改仅修正两个画布测试文件的按键模拟，并补充本文及渲染 crate 文档。

## 自动检查

所有命令从仓库根目录运行。直接 Editor Cargo 测试前清除 `ZIPZI_DEV_SESSION_DIR`。

| 检查 | 结果与边界 |
| --- | --- |
| `uv sync --locked`；`uv run --locked python tools/workspace/cargo_domain.py --check` | 通过；20 SDK + 3 Editor packages |
| `cargo test --locked -p zipzi-editor-render --features wgpu-backend --lib canvas_overlay` | 6 项通过；顺序、裁剪、even-odd 填充、缓存失效与实例布局 |
| 同 crate，`--test windows_overlay` | 5 项 D3D11 硬件测试通过 |
| 同 crate，`--test windows_shared_texture` | 5 项通过；日志确认 RTX 5060 / DX12 |
| 同 crate，`--test wgpu_readback` | 2 项通过；该独立 context 自动选择 Vulkan |
| `cargo test --locked -p zipzi-render-wgpu --tests` | 17 项通过；Windows 上 Metal `parity` target 为 0 项，不计作 Metal 对照通过 |
| `cargo test --locked -p zipzi-editor-core --test integration canvas_objects::` | 138 项通过 |
| `cargo test --locked -p zipzi-editor --features test-support --lib headless_tests::canvas:: -- --test-threads=1` | 修正测试按键后 48 项通过 |
| 同 Editor gate，`headless_tests::ui_performance:: -- --skip physics_ --skip timeline_` | 聚焦的 10 项通过；完整组的另外 4 项见待查记录 |
| `uv run --locked python tools/test_tools.py -p test_viewport_ui_benchmark.py` | 4 项通过 |
| 同工具 gate，`-p test_windows_gpui_trace_report.py` | 9 项通过 |
| `cargo build --release --locked -p zipzi-editor --features test-support,bench-instrumentation` | 通过；有 unused/dead-code 警告 |

D3D11 覆盖层测试实际检查明/暗主题、1x/1.5x/2x DPI、分数原点、五类控件与八种
选择/悬停/限制状态、透明背景下的半透明边框填充、线/骨骼/控件层级、even-odd 套索、
缓存与 retained surface 不可变性。共享纹理 gate 检查预乘 alpha、opacity、裁剪、
重复合成、尺寸变化、池预算及复用、生产者销毁和在途 GPU copy 关闭。
测试最终目标的回读用于像素断言；窗口生产交接另行通过 trace 验证。

最初画布组有 3 项确定失败：全选、撤销/重做、取消选择的测试写死 `cmd-a/z/d`，
Windows 默认命令使用 Ctrl。将这五次按键模拟改为 GPUI 的 `secondary-*`，保留
macOS Command 语义。修正后这三个用例通过，最终整组串行通过。
中间一次并行复跑另出现 wheel zoom 的 observer 通知计数 1（预期 0）；该用例单独
重跑及最终整组串行均通过。保留失败日志，不将单次重跑解释为消除了潜在时序波动。

## 真实窗口检查与短轮次对照

使用同一个保存的 Release exe、Mao runtime 模型、独立 Runtime/config。
实际窗口截图确认模型纹理和密集控件可见，日志确认
`NVIDIA GeForce RTX 5060 / wgpu (Dx12) (D3D12 shared texture; no readback)`。

先运行全选参数外观检查：30 次预热、3,600 次测量输入、60 Hz 目标。
随后 12 个进程分别预热 30 次、每个 workload 测量 180 次、120 Hz 目标：
导航与 opacity 各一对，选中全部 Mesh 拖参数按 native→GPUI→GPUI→native，
无选中对象参数一对，原生 deformer 显示开/关各一轮，四种变形编辑各一对。
合计 13 个进程、7,560 次测量输入，全部正常退出、trace 完整、lost_events=0，
无 `render_error`、`benchmark_failed` 或任何 readback stage。
每秒采样后台进程，未观察到 cargo/rustc/link/cl/clang；其他后台负载未被完全排除。

各进程二进制前后哈希一致，模型资源哈希跨进程一致。配对的 Mesh 选择、拓扑计数、
参数身份、变形对象与组件 ID 均一致。模型有 260 Mesh、128 参数；重负载选中
150 个可见未锁定 Mesh，5,159 个顶点、12,202 条边、2,825 条边界线。
参数审计确认 93 个 Mesh 实际变化，history 保持 `[0, 0]`。

下面是每个进程的 UI draw P95，单位 ms。除全选参数有两对外，其余仅一对，
属于执行与路径验证的短轮次采样，未达到稳定性能结论所需的重复覆盖。

| 场景 | GPUI | 原生 D3D11 |
| --- | ---: | ---: |
| 平移 | 7.756 | 7.788 |
| 缩放 | 7.428 | 7.875 |
| Opacity | 6.880 | 6.835 |
| 参数：无选中对象 | 7.735 | 8.382 |
| 参数：全选 Mesh，第 1 对 | 11.431 | 10.006 |
| 参数：全选 Mesh，第 2 对 | 11.308 | 9.935 |
| Mesh 单点拖动 | 6.143 | 6.489 |
| Mesh 8 点拖动 | 6.246 | 6.503 |
| Warp 控制点拖动 | 6.659 | 6.471 |
| Rotation 拖动 | 6.919 | 6.422 |

全选参数两轮原生均较低，无选中对象与部分轻负载原生较慢；不宣称全面性能改善。
全选参数 input→首次关联纹理的平台调用返回 P95，原生为 35.05/33.44 ms，
GPUI 为 37.00/36.50 ms。关联输入分别为 169/180、169/180 与 170/180、173/180；
未关联输入保留，不能按零延迟处理。平台 CPU 返回不代表 DWM/屏幕呈现完成，
120 Hz 是输入目标，不能作为显示 FPS。

原生覆盖层最大 22,746 个实例、1,455,744 字节上传、每次更新 1 个 instanced draw。
Deformer 显示开启时审计为 116 个未选中 Warp 外框、59 个 Rotation 骨骼；关闭后均为 0。
选择、Mesh 几何、history 和应用显示设置时的 evaluation count 保持一致。
关闭后的测量区间只有 2 个原点线实例，172 次覆盖层绘制均命中缓存、draw_calls=0。
开/关 UI draw P95 为 8.057/7.496 ms，此单轮结果不作为稳定收益结论。

本轮 Warp fixture 为 `36d63e33-4a9e-4613-8ab4-644f7ed85041`、组件 `2147483658`，
影响 4,933 个顶点；Rotation fixture 为 `f7039c46-e890-4f08-be96-9890ce222bd9`，
影响 358 个顶点。与 macOS 报告使用的 Warp 控制点不同，不直接横比变形性能。

## 待查与未覆盖

- 完整 `ui_performance::` 输出 10 项通过后持续无进展，约 3 分钟后主动停止该测试进程。
  未完成的是 3 个 `physics_resource_*` 和 1 个 `timeline_channel_scrubs_*` 用例；原因未定位。
  缩小过滤器后的 10 项完整通过，不将完整组列为通过。
- core/部分 UI 测试输出 `Project cache cleanup` 的 Windows 访问被拒绝/路径不存在警告。
  来源为原生 archive cache maintenance；本轮未定位或修复，也未出现对应断言失败。
- Windows D3D11 已执行独立像素断言，但完整 Editor surface 与 GPUI 回退的自动逐像素
  对照仍未移植；现有 `native_render` 完整对照和 Metal GPU parity 只在 macOS 运行。
- 未采集 PresentMon 或 Windows GPU 时间戳；未做实际 OS DPI/多屏切换、其他厂商/混合
  显卡、强制设备丢失和长期运行资源增长验收。

## 本地证据与复现

原始资料位于仓库根目录下 `target/windows-warp-viewport-20261105/`（不纳入 Git）：
`machine.json`、各 Cargo 日志、修正前/中间失败与最终通过日志、固定 exe、`source.diff`，
以及每个窗口进程的 `trace.jsonl`、`summary.json`、`run.json`、`background.json`、
`application.log` 和几何/参数审计。`validation.json` 汇总全部进程并核验配对与哈希。

现有 `viewport-ui-benchmark.py` 启动器依赖 macOS 系统命令；本轮保留独立 Windows
启动脚本 `probe.py`，使用现有生产自动输入驱动和 `windows-gpui-trace-report.py`。
每次复现使用尚不存在的 label，避免覆盖证据，例如：

```powershell
uv run --locked python target/windows-warp-viewport-20261105/probe.py selected native rerun-native
uv run --locked python target/windows-warp-viewport-20261105/probe.py selected gpui rerun-gpui
```
