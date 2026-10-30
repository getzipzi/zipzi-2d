# 观察纹理策略实施方案

日期：2026-10-30。状态：首版实现完成，相关门禁已通过。

实施进度（2026-10-30）：

- Rust 契约、CPU 规划器、可配置 limits、Original/FitBudget、不可行诊断均已实现。
- 已实现逐张有界读取与 PNG 解码、元数据/内容校验、共享内存像素、直接面积缩图和源 alpha 样本。
- 入队计划传递到观察与 Preview 捕获；批内共享派生纹理，按实际尺寸/像素更新 GPU 缓存与摘要。
- 批级纹理报告、预估、峰值准入、完成收缩及保留结果的生命周期计费已接入 Runtime。
- Session/Edit/Preview 的 Python Local/Connected 接口、生成类型、二进制签名及 inspection schema 2 已同步。
- 已修复 macOS 默认 Metal 的依赖边界：可选 `zipzi-observe-backend` 负责平台后端选择，
  默认 Metal 不引入 WGPU，非 macOS GPU 或显式 `wgpu-backend` 启用 WGPU；CPU 规划不依赖渲染器。
- 本地大模型观察、生成与隔离、客户端、Metal/WGPU 和 Editor 集成编译均已通过。
  DeepSeek 无窗口原生交互已通过；该模型的 Preview 创建触发默认状态预算，见追加验收记录。

### 已通过门禁

| 门禁 | 结果 |
| --- | --- |
| `cargo test --locked -p zipzi-contract` | 14 项通过 |
| `zipzi-core --lib image::` | 5 项通过，含压缩辅助数据和有界完整性校验 |
| `zipzi-project --lib texture::tests` / `resource_budget` | 8 + 3 项通过 |
| `zipzi-render --lib texture::` | 9 项通过 |
| `zipzi-observe --no-default-features --lib` / `--lib` | 19 / 35 项通过 |
| `zipzi-observe --test capture` / `--features wgpu-backend --test capture` | Metal / WGPU 各 9 项通过 |
| `./tools/test-sdk.sh runtime` | CPU Runtime 回归通过 |
| GPU Runtime `observation` / `observation_estimates` / `previews` / `scheduling` | 3 / 4 / 6 / 9 项通过，含 ignored GPU 用例 |
| CPU Runtime `runtime::observation_estimates::tests::` | 2 项通过，检查无副作用及细长输出 readback 对齐 |
| `./tools/test-sdk.sh python2 --observation` | 10 项 Local/Connected 观察与恢复用例、正反类型检查通过 |
| `./tools/test-sdk.sh python2` | 104 项常规用例 + 16 项恢复用例、正反类型检查通过 |
| `./tools/test-sdk.sh python2-connected` | 21 项已安装纯客户端测试通过 |
| `generate.py --check --no-cache` | 348 个生成文件一致 |
| `tools/test_tools.py -p test_python2_codegen.py` | 33 项生成与隔离测试通过 |
| 默认 Metal / 显式 WGPU / 非 macOS GPU / CPU 依赖矩阵 | 5 项通过 |
| `env -u ZIPZI_DEV_SESSION_DIR cargo check --locked -p zipzi-editor --tests` | Editor 集成编译通过；未运行 GUI 验收 |

### 本地大模型补充实测

Apple M4、macOS arm64、debug native Metal，512×512 输出。
使用未提交的本地 model3，导入后引用 715 个 asset ID；相同物理像素仍可由多个 asset 引用。
这些数据只记录该次运行，不作为性能或 RSS 保证。

| 指标 | 结果 |
| --- | --- |
| 引用集合源基础 RGBA | 703,790,836 字节 |
| Original | 元数据预估返回 `OBSERVATION_BUDGET_EXCEEDED` |
| FitBudget 预算 | 134,217,728 字节（128 MiB） |
| 实际渲染基础 RGBA | 133,964,220 字节 |
| 缩图数 / 整数最长边 | 185 / 468 |
| 非基础 mip 逻辑字节 | 44,350,288 字节 |
| 保守单源派生工作区 | 1,752,544 字节；内存源不重复计入原始 Arc |
| 保守总任务计费 | 2,559,038,520 字节（含 GPU、冻结源和输出等） |
| 导入、CPU 取景与预估合计 | 约 13.68 秒 |
| 观察准备与渲染合计 | 约 24.85 秒 |
| PNG 缓存 / 状态 | 观察后仍为零；文档版本和历史未变 |

已检查完整画面截图；实测脚本、JSON 和 inspection 均留在忽略目录 `target/python2/scratch/`，
不提交第三方模型与图像。临时峰值和结果释放另由可控 Runtime 测试验证。

在 Rust 观察管线中提供显式纹理策略，让大型模型可以在指定基础 RGBA 预算内生成观察用缩图。
Python SDK 提供类型化入口、执行前预估和执行后报告。默认仍使用原图；观察不创建 Edit，
不替换正式或候选资源，不改变文档版本、撤销历史和 Editor 播放状态。

首版覆盖参数姿态观察与动画 Preview、Local 与 Connected，完成批内纹理共享。
跨任务缓存、逐行解码缩图、ROI 可见性裁剪和按屏幕像素密度分配预算不在首版范围内。

## DeepSeek 模型追加验收（2026-10-30）

使用 `models/local/DeepSeek/大肥鱼.model3.json`，Apple M4、debug native Metal，
512×512 输出。导入后的 715 个引用 asset 的逻辑 RGBA 为 703,790,836 字节。
脚本通过 `./tools/dev-python` 运行；模型、截图和完整 JSON 均保留在忽略目录。

| 策略 | 实际基础 RGBA | 缩图数量 | 本次观察耗时 |
| --- | ---: | ---: | ---: |
| Original | 入队前按默认 256 MiB 上限拒绝 | — | — |
| FitBudget 256 MiB | 268,268,464 字节（255.84 MiB） | 82 | 63.66 秒 / 1 帧 |
| FitBudget 128 MiB | 133,964,220 字节（127.76 MiB） | 185 | 54.98 秒 / 2 帧 |
| FitBudget 64 MiB | 66,796,528 字节（63.70 MiB） | 269 | 18.80 秒 / 1 帧 |

本次部分观察与 Editor 编译并发，耗时只记录该次运行，不用于预算间的性能比较。
三档均验证预估/执行尺寸逐项一致、纹理表排序与基础总量、源身份及场景摘要稳定，
并检查了非空完整画面。128 MiB 的重复姿态得到相同像素；预算加一字节且实际尺寸不变时，
像素、`render_digest` 和 `input_sha256` 均相同。
观察未填充源 PNG 缓存，完成后准备和传输计费归零，文档版本和历史未变。
独立取消测试约 0.37 秒返回 `CANCELLED`，资源统计恢复到执行前。

**发现的默认限额限制：** 创建该模型的 Preview 返回
`BUDGET_EXCEEDED: Runtime budget exceeded: preview bytes`，默认 Preview 保留状态
上限为 134,217,728 字节。此时尚未进入纹理准备，FitBudget 不能豁免这项独立额度。
拒绝后 GPU 未初始化、Preview 数量及资源统计没有增加；本模型的 Preview 图像验收
因此未完成。首轮综合脚本保留原始失败记录，后续独立脚本确认了该限制和清理行为。

**Editor 原生交互：** `runtime_interaction_acceptance` 使用同一 DeepSeek 模型。
默认 10 秒首次就绪等待未通过；显式配置 120 秒后完成双视口切换及另一会话后台提交，
每项预热 10 次并测量 30 次，前台历史保持不变。两项输入到当前 Metal 绘制的 p95
分别为 10.40 ms、8.89 ms。再次使用默认 10 秒等待复测仍会超时；
Shirousagi 小模型对照在默认等待下通过，确认无需全局放宽普通验收。
这是注入 GPUI 输入的无窗口验收，不包含系统合成器与
截图读回，也不等于 Windows 或人工窗口验收。验收工具新增可配置就绪等待及视口错误诊断。

产物：

- `target/python2/scratch/deepseek-texture-validation/20261030-205555/`：三档截图、inspection 和首轮完整检查记录。
- `target/python2/scratch/deepseek-texture-validation/followup-20261030-210223/`：Preview 默认额度与取消清理记录。
- `target/python2/scratch/deepseek-editor-validation/extended-20261030-210450/results.json`：原生交互指标。

## 实施前基线与改造目标

实施前的 [Runtime 入队估算](../../crates/zipzi-runtime/src/runtime/observation.rs)
累加文档全部图片的基础 RGBA，超过 256 MiB 时拒绝请求。
[观察捕获](../../crates/zipzi-observe/src/lib.rs) 对实际引用纹理独立检查 256 MiB RGBA，
并在解码后再次检查 PNG 与 RGBA 合计。降低截图宽高不能绕过这些纹理限制。

改造后，由共同的纹理尺寸计划决定引用集合、目标尺寸和基础 RGBA 总量。
预估、入队和执行使用同一规划算法；执行使用入队时生成的计划，并重新验证资源。
128 MiB 基础纹理预算不表示整个任务只消耗 128 MiB 内存，也不保证资源、设备条件满足。

## 公共接口

以下接口已实现。Rust 契约和生成的 Python 类型是最终声明来源。

```python
from zipzi import observation as o

view = o.View(
    width=1024,
    height=1024,
    roi=roi,
    texture_policy=o.FitBudget(max_rgba_bytes=128 << 20),
)

estimate = session.modeling.estimate_observation(poses, view)
result = session.modeling.observe(
    poses, view, expected=estimate.source,
).wait()

# Edit.modeling 提供相同入口，使用该 Edit 的候选 Source。
# Preview 始终使用自身冻结的 Source。
preview_estimate = preview.estimate_observation(times, view=view)
animation_result = preview.sample(times, view=view).wait()
```

`ObservationView.texture_policy` 是带标签的策略类型，不接受任意字符串或字典代替类型化值。

| 策略 | 行为 |
| --- | --- |
| `Original()` | 默认策略；保留源尺寸与像素，超过 host 基础纹理上限时失败 |
| `FitBudget(max_rgba_bytes)` | 允许缩图，基础 RGBA 总量不超过指定预算；原图已满足时保持原样 |

预算使用整数，必须为正且不超过 host 暴露的观察基础纹理上限；Python 拒绝布尔值作为预算。
Rust 仍独立校验来自所有客户端的输入。无纹理场景也执行策略参数校验。
不隐式降低截图分辨率、拆分批次或调整策略。

`sample`、`seek`、`advance_to` 只要传入 View，就使用相同纹理语义。
没有 View 的 Preview 保持 CPU 采样行为，不执行纹理准备。

## 纹理集合与确定性尺寸规划

首版从冻结文档的网格及相关遮罩引用收集纹理，保留隐藏网格的引用，按 asset ID 去重并排序。
不把文档中完全未引用的图片加入观察纹理预算。规划器与捕获器共享引用遍历规则；
执行验证所有实际绘制和遮罩引用都被计划覆盖，缺失时失败，不临时追加未计费纹理。

这一保守集合不依赖姿态、ROI 或遮挡结果，因此无需运行 GPU 或回放动画即可规划。
同一批所有姿态共享一份尺寸表。不同 asset ID 即使内容相同，首版仍分别计入逻辑纹理预算。

对源尺寸 `w × h` 和候选整数最长边上限 `L`，令 `M = max(w, h)`：

```text
M <= L：w' = w，h' = h
M > L ：w' = max(1, floor(w × L / M))
         h' = max(1, floor(h × L / M))

render_rgba_bytes = Σ(w' × h' × 4)
```

使用检查溢出的整数运算，禁止浮点尺寸舍入。原图总量已满足预算时直接返回原尺寸表。
否则在 `[1, 最大源最长边]` 内二分搜索满足预算的最大 `L`。
若有 N 张纹理且预算不足 `4 × N`，返回预算不足；空集合总量为零、最长边上限为空。
非法零尺寸或溢出的资源描述直接报错。

规划只受基础 RGBA 预算影响。FitBudget 不根据 GPU 型号再次隐式缩图。
实际目标尺寸超出设备单边上限时明确失败；即使细长纹理满足字节预算，也可能发生这种失败。

## 缩图与 mipmap

从 [现有纹理过滤](../../crates/zipzi-render/src/texture.rs) 抽出通用面积平均函数。
每张缩图直接从源像素生成目标尺寸，不先降到某个 mip 层再缩放。
源与派生像素保持 straight RGBA8；RGB 按 alpha 加权，alpha 按面积平均，透明像素的 RGB
不贡献颜色。延续现有通道数值空间，不在首版引入新的色彩空间转换。

固定面积边界、舍入、零 alpha 输出、奇数尺寸和单像素轴行为，使用确定性遍历顺序。
无缩图时直接复用原像素，包括完全透明像素的原始 RGB，避免无意义的内容变化。

mipmap 从实际渲染基础层生成，延续每轴 `max(1, floor(axis / 2))` 的规则直到 1×1。
`estimated_mipmap_bytes` 只统计基础层之外的全部 mip 层逻辑 RGBA 字节数。
它不是 GPU 驱动实际分配量，也不能使用固定的三分之一比例代替逐级求和。

规划规则、基础层过滤和 mipmap 分别具有版本标识。修改舍入、颜色处理或缩图路径时升级相应版本。
CPU 派生像素通过固定输入输出测试维持确定性；不同 GPU 后端的最终截图不承诺逐字节一致。

## 预算与资源限制

| 类别 | 预算口径与所有权 |
| --- | --- |
| 基础 RGBA | 按 asset ID 去重后的目标尺寸总量；是策略约束，即使共享内存也不减少逻辑计数 |
| mipmap | 按目标尺寸逐级求和；CPU 生成缓冲区与 GPU 驻留分别进入工作集估算 |
| 读取与解码 | 编码数据、解码器工作区、输出像素及格式转换中同时存活的缓冲区 |
| 缩图与质量 | 当前纹理的目标像素、计算工作区和保留的源 alpha 样本 |
| GPU 上传与渲染 | 预乘副本、staging、纹理、mask、offscreen、输出目标及 readback 的保守估算 |
| 截图与结果 | 每帧 RGBA、PNG、报告、序列化和传输所需空间 |
| 源文档与图片 | 保留 Runtime 既有快照、图片、候选和历史计费；观察预算不豁免这些限制 |

总预留根据同时存活的数据计算。逐张解码阶段使用单张峰值加累计派生纹理，
不能只取最大原图 RGBA，也不能把所有原图解码量按批次累加为必需工作区。
共享的不可变像素沿用现有图片预算池计费，避免重复物理计费；报告仍保留完整逻辑字节数。
Runtime 的计费是保守资源估算，不宣称等于 RSS 或精确 GPU 显存使用。

扩展 `runtime.limits()`，至少暴露观察基础纹理、输出字节、帧数、输出宽高、
编码资源读取和解码缩图工作区上限。沿用现有 256 MiB 基础纹理、128 MiB 输出、64 帧、
4096 输出单边的默认策略；编码读取默认 64 MiB、单源准备工作区默认 1 GiB、
规划/报告元数据默认 8 MiB。工作区按编码、解码/转换和行缓冲的有界上界准入；
原图轴长至 16K 的既有图片限制对应的保守峰值可落入默认工作区，极端长宽比仍可能超限。
所有验证路径使用这些配置，移除对应的重复隐藏常量。

host 策略限制与 GPU 设备能力分别表达。设备未初始化时，设备上限标记为未知，
预估与 limits 查询不为了获得它而初始化 GPU。执行仍校验实际设备。

## 逐张准备与分配前校验

执行顺序为：

1. 校验请求和 expected Source，冻结源描述与内存图片引用，生成共同尺寸计划。
2. 完成任务准入并预留预算，再进入大块读取、解码、派生和 GPU 分配。
3. 逐张有界读取；核对编码大小、PNG 头部、预期尺寸和源内容身份。
4. 在大块分配前检查解码器输出大小与工作区；完成解码及完整性校验。
5. 按需提取源 alpha 样本，缩图并计算渲染像素哈希，释放该张临时原始数据。
6. 全批共享派生纹理，上传 GPU 并逐帧渲染、生成报告和输出。

[磁盘读取](../../crates/zipzi-project/src/store/assets.rs) 和
[PNG 解码](../../crates/zipzi-core/src/image.rs) 现已支持预算约束。
不能先用无限制 `fs::read` 或分配整个解码输出，再发现计划与文件不匹配。
文件长度预检后仍需有界读取；解码器内部工作区、PNG 辅助数据和颜色转换临时空间也须受约束。
资源尺寸变化或哈希不匹配时失败，不在执行过程中静默重算计划。

首版允许完整解码单张原图。单张解码峰值超过工作区时失败，即使缩图结果很小也不绕过限制。
取消检查覆盖纹理之间、哈希处理和缩图循环；已进入且不支持中断的解码完成后再响应取消。

内存图片优先复用已有 `PreparedImageContent.texture()` 的不可变 `Arc` 像素。
在 `zipzi-project` 提供经过元数据和内容身份校验的读取路径，避免观察触发 PNG 编码与 RGBA 复制。
观察侧 `ResolvedTexture` 保留源身份、源尺寸、渲染尺寸、渲染像素及必要 alpha 样本，
不长期携带完整 `AssetData { PNG, RGBA }`。
冻结源仍可能因其他所有者保留原图；逐张释放仅指观察新增的临时原始数据。

## 内容身份与 Source 保证

源内容身份使用带类型的值，不能把 PNG 字节哈希与 RGBA 像素哈希混为一个无类型字符串：

| 类型 | 含义 |
| --- | --- |
| `png_sha256` | 经验证的原始 PNG 字节 SHA256 |
| `rgba8_sha256` | 带版本域、明确尺寸和像素约定的源 straight RGBA8 内容 SHA256 |

内存生成图片使用像素身份，无需为了身份生成 PNG。同一输入路径的身份不因纹理预算变化而变化。
文件与内存表示之间不承诺源身份相同；实际派生像素另由 `render_rgba_hash` 表达。

`Source` 保证文档或候选快照身份，不保证外部文件字节被冻结。
预估仅报告元数据中的声明身份及其验证状态，不读取整张纹理验证哈希。
执行对有声明哈希的磁盘资源验证一致性；对没有声明哈希的资源，允许读取并报告本次实际身份，
同时明确预估到执行之间没有文件字节一致性保证。实际尺寸仍须匹配源描述。
需要严格复现时，调用者必须使用已有内容身份的资源。

## 源 alpha 质量检查

[现有覆盖率算法](../../crates/zipzi-observe/src/quality.rs) 使用源尺寸和原图 alpha，
采样步长为 `max(1, ceil(max(width, height) / 256))`，阈值为 alpha ≥ 8。
缩图前提取同样坐标的 alpha 样本，保留源尺寸、步长、网格布局和采样规则版本。

后续仍在源尺寸对应的 UV 空间计算三角形覆盖，保持原来的采样位置、阈值、计数及工作量上限。
样本按纹理共享，并计入预算；只在请求需要源 alpha 覆盖率时保存。
仅请求 strain 等几何质量功能不必保留 alpha 样本。
同一源和姿态下，Original 与 FitBudget 的源 alpha 覆盖率报告必须一致。

## 报告与摘要

每批返回一次纹理报告，分别放入 `ObservationBatch.texture_report` 和
`PreviewBatch.texture_report`；没有观察 View 的 Preview 报告为空。
每帧不重复序列化完整纹理表，帧通过批次及 capture_id 关联报告。

报告包含以下字段，具体 Rust 类型按既有 ApiShape 和 BinaryOutput 机制定义：

```text
texture_report
  policy
  effective_max_rgba_bytes
  longest_edge_limit
  source_rgba_bytes
  render_rgba_bytes
  estimated_mipmap_bytes
  resized_count
  filter
  algorithm_version
  planning_version
  mipmap_version
  textures[]
    asset_id
    source_size
    render_size
    source_content_identity { kind, hash }
    render_rgba_hash
```

纹理表按 asset ID 排序。`source_rgba_bytes` 只统计本批引用集合。
`render_rgba_hash` 采用版本域、明确宽高和 top-left、row-major、straight RGBA8 像素约定，
描述基础层渲染输入；GPU 上传的预乘和 mipmap 规则另由版本标识固定。
这些字段报告逻辑内容，不将预留额度当作实测内存峰值。

摘要契约如下：

- `scene_digest` 描述冻结源场景、源纹理内容和姿态，不包含纹理预算与派生尺寸。
- `render_digest` 和 `input_sha256` 包含实际尺寸、派生像素身份、过滤与 mipmap 版本，
  以及各自既有的渲染输入；需要升级摘要格式版本。
- 两个预算产生相同实际渲染输入时，渲染摘要保持一致。请求预算变化由报告体现。
- 原尺寸且像素相同的 Original 与 FitBudget 不因策略标签不同而产生不同渲染摘要。

首版只做批内共享，GPU 批内复用也必须按实际渲染内容判断，不能仅按 asset ID 或源 PNG 哈希。
后续跨任务缓存使用源内容身份、目标尺寸和算法版本作为键，采用受 Runtime 预算约束的 LRU。

## 预估语义

`estimate_observation` 返回带 `Source` 的类型化值，读取元数据，
不解码纹理、不初始化 GPU、不回放动画、不预留任务或 GPU 额度，也不创建持久化计划句柄。
必要的短期元数据分配仍受普通请求大小限制。
CPU 规划模块与 GPU 初始化隔离；host 缺少 GPU 能力时仍可返回估算及不可渲染原因。

Session 与 Edit 入口接收 poses 和 View；Preview 入口接收 times、View 及影响结果大小的
geometry、queries 选项，使用 Preview 自身的源和当前控制状态，不改变时钟或 sequence。
Preview 在 busy 状态拒绝预估；预估返回 preview_id 和当时的 sequence 供诊断。
后续控制操作或资源条件变化可能使执行估算改变，因此 sample 仍重新检查，不把预估当作预约。

结果包含源、有效策略、引用集合、原始与目标尺寸、逻辑 RGBA 与 mipmap 总量、
保守解码缩图工作区、GPU 逻辑工作集、输出和总任务计费估算，以及资源与设备检查状态。
仅凭元数据不能得知的编码或解码开销使用有界读取和受限解码对应的保守上界，
不得把未知开销记为零。预估报告区分静态可行性、当前可用额度和未执行的设备验证。

对于语法正确但不可执行的请求，仍返回诊断：Original 超限时返回源尺寸表及超限原因；
FitBudget 连每张 1×1 都放不下时返回最低所需字节，目标计划为空。
预算非正、超过 host 策略上限或资源元数据非法等无效输入直接返回结构化错误。
执行不接受客户端提交的尺寸表作为可信输入。

## 生命周期与错误

入队预留峰值，工作完成后将计费收缩为实际保留对象的额度。
临时 CPU/GPU 资源真正销毁后才释放其预留；取消信号发出时不能提前归还仍在使用的工作区。
Job 结果、错误、冻结源、传输 blob 及 Preview 状态继续按其实际生命周期计费，
直到对应对象释放。成功返回不意味着 Runtime 内所有相关额度立即归零。

失败和取消清理部分派生纹理、部分输出及上传资源。Preview 沿用现有 completed seek 保留语义，
不因新纹理策略承诺动画状态全批回滚；文档和撤销历史始终不变。

错误至少区分无效策略、基础 RGBA 不足、编码读取超限、解码工作区不足、
资源尺寸或哈希不匹配、设备尺寸不支持、总 Runtime 额度不足和取消。
复用现有错误体系，按需扩展字段，提供 asset ID、预算类别、请求量或最低需求、适用上限。
错误不自动回退到更低预算、其他策略或 CPU 渲染。

## 分层职责与实施顺序

| 层 | 改动 |
| --- | --- |
| zipzi-contract | 策略、预估、批级报告和 limits 类型，明确 Source 与摘要文档 |
| zipzi-render | 提取通用 alpha 加权面积缩图函数，固定算法版本和边界行为 |
| zipzi-core 与 zipzi-project | 受预算约束的读取解码、分配前验证、共享内存像素和内容身份 |
| zipzi-observe | CPU 尺寸规划、逐张准备、源 alpha 样本、派生内容摘要和批内共享 |
| zipzi-runtime | 统一准入、实际计费与释放，Session、Edit、Preview 的预估与观察接入 |
| zipzi-python2 | 生成类型与编解码，Local/Connected 入口、公开文档和 inspection 报告 |
| Editor 与协议宿主 | 使用共同 Runtime 配置与新签名，验证集成与资源生命周期 |

按以下顺序实施，每步完成对应聚焦测试后继续：

1. 契约与纯规划：确定类型和 limits，完成引用集合、整数尺寸、溢出与不可行诊断。
2. 纹理准备：有界读取和解码、共享像素、通用缩图、源 alpha 样本与内容身份。
3. 捕获与摘要：替换 ResolvedTexture，接入共同计划、批级报告和 GPU 内容复用。
4. Runtime：两条执行路径共同准入、峰值预留、完成收缩、取消失败回收及无 GPU 预估。
5. Python 与宿主：更新 schema、生成代码、二进制签名、接口文档和 inspection，验证 Local/Connected。
6. 回归与资源验证：核对画质、Preview 状态、内存计费和真实大模型效果。

项目尚未发布，直接调整契约并同步升级 Runtime、Editor 和客户端，不保留旧格式分支。
实施需检查 zipzi-observe 的 GPU 可选依赖与 Runtime feature 边界，确保 CPU 规划不会触发 GPU 初始化。

## 验收与测试选择

| 范围 | 必须验证的行为 |
| --- | --- |
| 默认与缩图 | Original 超限失败；资源与设备条件满足时 FitBudget 成功，实际基础 RGBA 不超预算 |
| 规划边界 | 空集合、恰好预算、4N 最小预算、不足 4N、极端长宽比、奇数尺寸、溢出、稳定排序 |
| 引用集合 | 未引用大图不计入；隐藏网格与遮罩引用保留；多姿态引用去重且计划覆盖实际执行 |
| 过滤与质量 | 透明边缘、部分 alpha、奇数边和单像素轴正确；源 alpha 报告不随预算改变 |
| 预估 | 不解码、不初始化 GPU、不改变 Preview 状态、不留预留；资源未变时尺寸计划与执行一致 |
| 资源验证 | 元数据错误、文件在预估后替换、缺失哈希、异常大编码输入和单图工作区超限明确处理 |
| 内存路径 | 内存图片不触发 PNG 编码或整图复制；共享图片不重复物理计费 |
| 摘要与报告 | 源摘要不随预算变化；实际尺寸或像素变化影响渲染摘要；等价渲染输入摘要相同 |
| 生命周期 | 排队取消、解码后取消、缩图中取消、上传失败、正常完成及结果未释放时的额度正确 |
| 状态隔离 | 文档版本、候选内容、撤销历史、资源事件和 Editor 播放不变；Preview 保持既有失败语义 |
| 集成 | Session/Edit、观察/Preview、Local/Connected、Metal/WGPU 使用相同策略与报告语义 |

先按根 README 的测试选择表运行变更模块测试：规划与过滤使用纯 CPU 单元测试；
读取解码使用小型合成 PNG 和受控预算；Runtime 用可控 executor 验证计费与取消。
测试通过“未调用解码器、编码器、GPU 初始化入口”的可观察证据验证预估和共享路径，
不只比较最终图片。

契约与生成阶段运行 `cargo test --locked -p zipzi-contract` 和
`uv run --locked python tools/test_tools.py -p test_python2_codegen.py`。
集成阶段按变更范围选择 `./tools/test-sdk.sh runtime`、`python2`、`python2-connected`，
实际 GPU 观察使用 `./tools/test-sdk.sh python2 --observation`。
运行前检查各脚本 `--help` 和 crate README，避免无参数触发全量测试。
Editor 仅运行相关聚焦门禁；直接执行 Editor Cargo 测试时清除 `ZIPZI_DEV_SESSION_DIR`。

新增夹具优先使用可再分发的小型合成图片。大型本地模型只作补充验证，记录源/目标逻辑字节、
解码准备耗时和任务计费变化，不把单个模型的缩图比例或内存占用当作通用保证。
