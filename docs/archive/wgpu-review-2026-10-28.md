# wgpu review、验收与性能复测（2026-10-28）

本次审查对象是 `cfab4bded1572d3f7753980159ac0197c07ea652` 的 wgpu 实现。
最终基线源码取自 `d9787e85`，其中 renderer 与 `cfab4bde` 相同；新增的参数面板提交保持不变。
结论限定于本机 Apple M4 / macOS 27.0.1（26A434），wgpu 29.0.4 的 Metal 后端，
Rust 1.99.0。Windows/Linux 的设备与窗口路径未验收。

## Review 后落地的修改

| 问题 | 修改与验证 |
| --- | --- |
| 几何上传将无稳定布局约定的 `Vec2` 直接转成字节 | 显式打包 XY float32，保留 slab 合并上传；GPU 图像、变形和在途帧测试通过 |
| 默认 device limits 将 M4 的二维纹理上限限制为 8192 | 请求 adapter 的 resolution limits；验证设备上限与 adapter 一致，并上传 8193×1 纹理 |
| 稳定画面逐帧重复创建 bind group | 按 immutable uniform buffer、source/mask view 身份及 sampler 模式缓存；逐帧淘汰未使用项，不缓存临时颜色/目标快照 |
| Replace 总是分配主画面中间纹理并执行最终拷贝绘制 | 主 target 无 destination read 且不采样宿主输出时直接绘制；Composite、destination read 和自采样保留中间纹理 |
| `wgpu_preview` 忽略 Editor 的初始 fit 视图，只展示局部放大内容 | 使用 `preview.view`，以适配后的视图为基准缩放；benchmark 使用同样的完整模型视口 |

新增回归覆盖同 revision 的纹理 view 替换、输出格式变化、稳定绑定复用、
自采样输出以及 adapter 纹理尺寸。缓存键使用 wgpu 资源的稳定身份，避免每次查找重复哈希
192 字节 uniform；uniform buffer 继续保持不可变，未引入跨在途帧的队列覆写。

## 验收证据

- `cargo test --locked -p zipzi-render-wgpu --tests`：20 项通过。
- `cargo test --locked -p zipzi-sdk-observe --features wgpu-backend --test capture`：11 项通过。
- `env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --features test-support,wgpu-backend --test native_render metal:: -- --test-threads=1`：10 项通过，1 项按原定义忽略（缺少指定 PSD）。该 feature 下测试实际使用 `WgpuMetalSceneRenderer`。
- `compare_wgpu_blends.py`：720/720 项通过，最大通道差 3/255。本次没有放宽参考图阈值；3/255 是被审查 commit 已设置的阈值。
- 完整 Mao（260 meshes）与 Shirousagi（24 meshes），512×512，适配视图的 1.0/1.1/0.6 倍：各视图最大通道差均 ≤1/255，超过 8/255 的差异像素均为 0；缩放没有几何重上传。两模型完整输出已目视检查。
- `cargo clippy --locked -p zipzi-render-wgpu --all-targets --no-deps -- -D warnings` 通过。包含依赖的严格 clippy 被 `zipzi-core` 现有的 21 项 lint 阻断，未宣称全工作区 lint 通过。
- 修改文件的 rustfmt、`git diff --check`、`tools/workspace/cargo_domain.py --check` 通过。

本机原始日志、PNG 和逐帧 CSV 在 `target/wgpu-review/`。完整模型图像在
`mao-fitted-pixels/`、`shirousagi-fitted-pixels/`，官方图像对照在 `blends/`。
该目录是本机产物，不随仓库分发第三方模型。

## Benchmark 方法

使用同一份 `wgpu_benchmark`，分别链接优化前和优化后 renderer，Release 构建。
基线通过临时源码副本构建，并使用相同的本地 PurismCore；未回退当前工作区文件。
模型通过 Editor 正常加载、适配视口，以 1280×800 RGBA8 输出。
每模型测 idle、pan、deform；每组 20 帧预热后记录 90 帧，重复 3 轮，轮间交替 wgpu/Metal 顺序。
deform 在计时外改变全部 drawable 顶点，以覆盖几何上传和 mask 失效成本；不包含参数求值。

CPU 时间从 scene adapter 提交开始至 queue submit 返回；完成时间从同一起点到对应帧完成。
两者都不包含加载、场景修改、PNG 编码或 readback；完成时间包含调度和 CPU 等待，
不是 GPU timestamp，也不是完整 Editor 输入到显示的延迟。测量期间无并行编译或测试任务。

早期使用默认 zoom=1 的结果只覆盖局部放大视图，保留在本机诊断文件中；以下正式结果仅使用
修正后的全模型视口。每个单元格合并 3 轮共 270 帧。分轮统计与二进制 SHA-256 见配套 JSON。

CPU submission，单位 ms：

| 模型 / 场景 | wgpu 前 P50 | wgpu 后 P50 | wgpu 前→后 P95 | Metal 前→后 P50 |
| --- | ---: | ---: | ---: | ---: |
| mao / idle | 0.663 | 0.515 | 2.223 → 1.387 | 0.199 → 0.218 |
| mao / pan | 0.807 | 0.844 | 2.013 → 2.722 | 0.316 → 0.255 |
| mao / deform | 1.164 | 1.265 | 1.644 → 1.774 | 0.489 → 0.480 |
| shirousagi / idle | 0.243 | 0.164 | 0.500 → 0.331 | 0.069 → 0.093 |
| shirousagi / pan | 0.370 | 0.393 | 0.679 → 0.775 | 0.109 → 0.096 |
| shirousagi / deform | 0.514 | 0.389 | 1.018 → 0.746 | 0.214 → 0.233 |

完成帧墙钟时间，单位 ms：

| 模型 / 场景 | wgpu 前→后 P50 | wgpu 前→后 P95 | Metal 后 P50 |
| --- | ---: | ---: | ---: |
| mao / idle | 2.303 → 2.067 | 8.321 → 7.218 | 0.662 |
| mao / pan | 2.462 → 2.480 | 6.655 → 8.281 | 0.830 |
| mao / deform | 2.772 → 3.891 | 7.120 → 8.539 | 1.520 |
| shirousagi / idle | 1.795 → 1.682 | 6.797 → 4.741 | 0.691 |
| shirousagi / pan | 1.927 → 1.972 | 6.509 → 6.888 | 0.738 |
| shirousagi / deform | 2.082 → 1.943 | 8.127 → 6.619 | 0.957 |

逐帧资源开销（中位数）：

| 模型 / 场景 | bind group 前→后 | render pass 前→后 | 私有 color bytes 前→后 |
| --- | ---: | ---: | ---: |
| mao / idle | 151 → 0 | 2 → 1 | 4.096e+06 → 0 |
| mao / pan | 151 → 132 | 2 → 1 | 4.096e+06 → 0 |
| mao / deform | 192 → 64 | 16 → 15 | 4.096e+06 → 0 |
| shirousagi / idle | 13 → 0 | 2 → 1 | 4.096e+06 → 0 |
| shirousagi / pan | 13 → 12 | 2 → 1 | 4.096e+06 → 0 |
| shirousagi / deform | 14 → 3 | 3 → 2 | 4.096e+06 → 0 |

稳定帧 CPU P50 在 Mao / Shirousagi 上分别下降约 22% / 32%；两者稳定帧绑定创建均为 0，
减少一次主画面 pass 和 4,096,000 字节私有颜色附件。原生 Metal 仍明显更快。

收益不是全面的：本轮平移 CPU P50 上升约 5% / 6%，Mao 变形 CPU P50 上升约 9%，
完成帧时间也有回退；Shirousagi 变形 CPU P50 下降约 24%。桌面负载下 P95 和分轮数据波动较大，
这组结果不能证明动态场景已加速，也不能将全部变化归因为 renderer 修改。保留全部场景和 Metal 对照，
不以分配计数减少替代耗时结论。下一阶段应针对平移的 uniform/binding 重建、Mao mask 重绘和 worker 完成等待继续分析。

[分轮统计、原始 CSV 校验值和二进制身份](wgpu-review-2026-10-28.json)。

## 复现

```sh
cargo run --locked --release -p zipzi-editor-render --features wgpu-backend \
  --example wgpu_benchmark -- models/local/mao/runtime/mao_pro.model3.json \
  target/wgpu-mao.csv 90 3
cargo run --locked --release -p zipzi-editor-render --features wgpu-backend \
  --example wgpu_preview -- models/local/mao/runtime/mao_pro.model3.json \
  target/wgpu-mao-pixels
uv run --locked python tools/validation/compare_wgpu_blends.py
```

## 仍有的边界

macOS Editor 的模型内容走 wgpu，窗口与交互 overlay 仍走原生 Metal。
当前桥接在渲染 worker 上等待 wgpu 完成，再交给 GPUI；没有图像 readback，也不在 UI 线程等待，
但这段完成等待仍影响交付帧延迟。多 GPU 设备上的 adapter 匹配、Windows/Linux 窗口集成、
真实 Vulkan/DX12 验收和完整交互延迟，需要独立测量。本次未进行打包分发或完整 Python wheel 验收。
