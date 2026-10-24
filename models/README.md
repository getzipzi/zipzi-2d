# Live2D 模型目录

这里记录仓库实际使用的完整 Live2D 模型。`local/` 是唯一的本地模型源目录，已被 Git 忽略；不要提交受第三方许可限制的模型文件。小型、由本项目生成的 MOC3 回归夹具不放在这

| 模型 | 模型源位置 | 使用处 |
| --- | --- | --- |
| Nijiiro Mao (`mao_pro`) | `models/local/mao/runtime/mao_pro.model3.json` | `benchmarks/cubism-matrix` 的六种对照运行，以及可选的 MOC3 导入、导出测试与示例，CMO3/CAN3 导出集成测试 `mao_cubism`。 |
| Hiyori | `third_party/CubismSdkForNative-5-r.5/Samples/Resources/Hiyori/` | 可选 MOC3 导入测试。 |
| Rice、Mark | 同一 SDK 的 `Samples/Resources/Rice/`、`Samples/Resources/Mark/` | 可选旧版本 MOC3 导入测试。 |
| Ren | 同一 SDK 的 `Samples/Resources/Ren/` | 可选 MOC3 导入与导出测试。 |

将 Mao 的完整模型包放到 `models/local/mao/`，保留 `runtime/` 内的相对路径。运行 `uv run --locked python benchmarks/cubism-matrix/tools/matrix.py prepare-godot` 或构建渲染 benchmark 时，工具会把模型复制到 benchmark 工程的已忽略 `assets/live2d/mao/`；该目录是生成副本，不作为模型源。Core-only benchmark 与 MOC3 测试直接读取 `models/local/mao/`。

Cubism SDK 样本保留在 SDK 安装包原位，避免复制模型并破坏 SDK 自身的目录结构。缺少本地 SDK 或 Mao 时，相关可选测试会跳过；正式 benchmark 需要按 [`benchmarks/cubism-matrix/README.md`](../benchmarks/cubism-matrix/README.md) 安装依赖。

SDK 包还含有 Haru、Mao、Natori、Wanko 样本；仓库当前代码没有直接使用这些模型。这里的 Nijiiro Mao (`mao_pro`) 与 SDK 包中的 `Mao` 是不同的输入。


## Editor model inventory (imported record)

# 编辑器性能基准模型

当前统一位置是仓库根 `models/local/`。脚本和测试不再查找相邻仓库或应用目录下的副本。
本地资源仍忽略，不随代码提交；P5 已核对并汇集原 checkout 的素材。
以下清单和复制日期保留为原 Editor 素材记录，其中 `local/` 现在对应根模型目录。

| 模型 | 导入入口 | 文件数 | 总大小（字节） | 纹理 |
| --- | --- | ---: | ---: | --- |
| Nijiiro Mao | `local/mao/runtime/mao_pro.model3.json` | 24 | 80,242,471 | 1 × 4096 × 4096 |
| Shirousagi | `local/Shirousagi/Shirousagi.model3.json` | 5 | 5,345,828 | 1 × 2048 × 2048 |

Shirousagi 同时保留 `local/Shirousagi/Shirousagi.psd`，可用于 PSD 导入场景。
2026-09-28 复制时逐文件核对 SHA-256，并确认两个 model3 的所有文件引用均存在。
文件数和大小不包含 `.DS_Store`。这些是资源信息，不是运行性能测量结果。

2026-10-18 将 `~/Downloads/mao_en` 的配套 Cubism 工程合并到 `local/mao/`，
22 个共有文件经 SHA-256 核对一致。新增 `local/mao/mao_pro_t06.cmo3` 和
`local/mao/mao_pro_t06.can3`，用于 Cubism Editor 工程导入验收；新增副本同样完成
SHA-256 校验。该素材统一使用 `local/mao/`，不保留单独的 `local/mao_en/`。

## 基准场景

优先保证前台编辑、渲染和模型切换性能。多文档功能接入后，以两个模型同时打开为基本工作负载。

- 导入每个 model3，记录导入耗时和首帧可见耗时。
- 连续缩放、平移画布，记录帧耗时 p50/p95/p99 和输入到画面更新延迟。
- 同时打开两个模型并反复切换，分别记录首次显示与缓存命中时的切换延迟。
- 编辑属性并撤销、重做，记录交互延迟及历史增长后的内存占用。
- 分别记录单模型、双模型和关闭标签页后的进程内存及 GPU 资源占用。
- 后台标签页停止持续渲染；优先保留近期使用的预览缓存，基于测量结果决定回收预算。

顶点工具的首期范围仅覆盖无参数绑定、变形器、BlendShape 和 Glue 依赖的基础网格。
这两个完整模型用于导入、渲染及多文档基准，不预设其网格符合顶点工具的编辑条件。
顶点拖动另用 SDK 生成的基础网格夹具，或经检查满足条件的 PSD 导入结果验证。

测量时记录机器、显示缩放、画布尺寸、构建模式与 Metal HUD 状态；不同配置的数据不直接比较。
已采集 [CPU 与原生 Metal 基线](../docs/editor-foundation-baseline.md)，包含导入、场景求值、
单/双模型原生渲染与切换的分位数、RSS 和 GPU 分配；据此设置 128 MiB 近期纹理缓存预算。
无窗口测量不包含 AppKit/GPUI 输入到屏幕显示的端到端延迟，该项仍未验证。

## Zipzi consolidation — 2026-10-24 (P5)

The SDK's 27 source files and Editor's 51 source files were hashed before copying.
Their overlap had no content conflicts. The merged root `models/local/` retains
48 asset files after excluding `.DS_Store`, `.zipzi.lock` and `cat_import.af~lock~`;
original model assets remain untouched. A source application lock changed externally
during verification; it is excluded from the asset comparison. All 26 model3 file references resolve.
Private per-file hashes and source provenance are retained in
`.git/merge-baseline/p5-models.json`, not committed or included in source releases.
CMO3/CAN3 source bytes are preserved; portable import tests normalize authored
model references only in temporary copies, as the existing acceptance tests require.
