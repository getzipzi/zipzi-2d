# R0 基线与门禁证据

日期：2026-10-23。配对 revision、机器、预算、所有权和启动契约见
[实施记录](r0-preparation.md)。本文件只报告实际执行结果。

## SDK Release CPU 基线

完整摘要与来源哈希见 [`r0-cpu-baseline.json`](r0-cpu-baseline.json)。原始 JSONL、
构建输出、环境、模型哈希及逐进程 `/usr/bin/time -l` 在 SDK 工作树
`target/runtime-r0-clean/`，其文件摘要已登记。首次受其他编译干扰的探索数据仍保留在
`target/runtime-r0-baseline/`，不计入下表。

命令：

```sh
CARGO_TARGET_DIR=/Users/liangbohao/web/zipzi-2d/target \
  uv run --locked python tools/runtime_baseline.py target/runtime-r0-clean \
  --model /Users/liangbohao/web/zipzi-editor/models/local/mao/runtime/mao_pro.model3.json
```

每个 fixture 运行 5 个独立进程。数值为逐进程 P50/P95 的中位数，单位 ms；
各进程范围与最大分配增量保存在 JSON，未将两个模型合并平均。

| 操作 | 合成 P50 / P95 | Mao P50 / P95 |
| --- | ---: | ---: |
| 捕获只读快照 | 0.110 / 0.462 | 0.714 / 1.616 |
| 创建 owned candidate | 0.259 / 0.713 | 0.726 / 1.795 |
| Motion CreateGroup Plan | 0.014 / 0.048 | 0.574 / 1.296 |
| 元数据提交 | 0.527 / 1.571 | 2.588 / 3.982 |
| Undo | 0.331 / 0.902 | 2.298 / 4.127 |
| 独立 Motion seek 1 秒 | 0.019 / 0.049 | 3.303 / 4.667 |
| 首次保存（每进程仅 1 次） | 37.717 / 单点无尾分位意义 | 48.846 / 单点无尾分位意义 |
| 暖保存 | 28.209 / 31.937 | 42.233 / 60.544 |

合成模型：128 Mesh / 21,632 顶点 / 1 参数 / 1 Motion，4 MiB 内存图片；
Mao：260 Mesh / 6,012 顶点 / 128 参数 / 7 Motion，逐纹理解码总量 64 MiB。
后者是读取时的解码负载之和，不宣称这 64 MiB 均长期驻留在 SDK Session 内。

| 内存指标 | 合成 | Mao |
| --- | ---: | ---: |
| SDK 内容估计 | 988,179 B | 2,639,045 B |
| 创建/导入后 Rust heap 存活增量 | 7,019,727 B | 6,566,463 B |
| 快照最大新增存活字节 | 1,630,417 B | 2,459,180 B |
| candidate 最大新增存活字节 | 1,599,971 B | 2,396,633 B |
| 进程 RSS 高水位范围 | 58,884,096–59,801,600 B | 271,007,744–288,358,400 B |

AuthoringSession 的 `size_of` 为 4,640 B，只是容器本体，不能替代堆负载。
快照固定文档副本并共享内存图片；该表不证明 GPU/Observer/Editor 已共享全部缓存。
计数分配器的 retained 负增量截为零；零不表示无分配或无工作。

全部 10 个进程完成模型保存/重开、Mesh/Motion/纹理字节一致性检查；候选取消、只读
快照和 Plan 不改变正式 Version/历史；每次有变化的提交与 Undo 分别产生一个 SDK
事件。编译活动边界检查、源码与模型哈希漂移检查均通过。后台系统负载不能仅由边界
检查完全排除，故保留范围，不建立通用 RPC 或输入到显示时延承诺。

这些测量支持将保存、重型提交、Undo/Redo、长 seek 和资源操作纳入后台调度评估；
不把快照每次复制一份工程当作 UI 查询路径。正式性能比较在 R2 对相同 workload
交替运行 baseline/candidate，沿用计划中的 10%/15% 和绝对噪声裕量。

## 已执行聚焦测试

| 命令（仓库根目录） | 结果 / 证明范围 |
| --- | --- |
| SDK `uv run --locked python tools/runtime_inventory.py --check` | 952 条 API/记录源清单匹配；不等于已连接 |
| SDK `uv run --locked python -m unittest discover -s tools -p test_runtime_inventory.py` | 3 项通过：分模块/重导出/原生导出/领域意图发现，语法与注释/字面量隔离，以及损坏/Debug/缺样本报告拒绝 |
| SDK `cargo test --locked -p zipzi-sdk --test integration -- vertical_slice:: handles:: history_budget::` | 11 项通过；owned candidate/失败/取消/完整版本/旧句柄/预算/redo 基线 |
| Editor `cargo test --locked -p zipzi-editor-core --test integration workspaces::` | 8 项通过；模式切换、手势、共享 Undo、多视图、后台保存 |
| Editor `cargo test --locked -p zipzi-editor-core --test integration -- editor:: properties:: timeline:: viewport_isolation:: viewport_states::` | 99 项通过；保存成功/失败与路径预留、属性、Timeline 编辑与播放、独立视口/隔离状态 |
| Editor `cargo test --locked --workspace --features zipzi-editor/test-support --lib headless_tests::ui_performance` | 4 项通过；Timeline scrub/overview 复用、画布通知、属性列表虚拟化与历史刷新 |
| Editor `python3 scripts/runtime-migration-inventory.py --check` | 基线采集时 70 文件、669 个源引用；整合主分支后刷新为 72 文件、692 个引用并重新检查；未宣称已迁移 |
| SDK `cargo fmt --all -- --check`；两仓 `git diff --check` | 通过 |

Editor 测试均 unset `ZIPZI_DEV_SESSION_DIR`。Cargo 复用各主 checkout 的 target
缓存，配对源码仍来自隔离工作树。第一次误选 `--lib workspace::` 运行到 0 项，
不计通过数，随后改为实际 integration 的 `workspaces::` 并验证上表 8 项。
没有运行两仓全套或 Python Release wheel 验收；本阶段未改 SDK 正式行为或绑定。

旧检查单独执行：

```sh
uv run --locked python modules/zipzi-python/tools/check_coverage.py --require-complete
```

结果为**失败（既有清单过期）**：现有 scanner 发现若干未登记方法，且仍登记已删除
`Edit.replace_canvas` / `Session.canvas`；还将 AuthoringSnapshot 的方法归入 Session。
本阶段保留这项失败，既未伪造绑定完成率，也未删除旧断言来通过它。新清单独立扫描
完整模块和实际 owner；后续绑定重构时需明确替换旧 coverage gate。

构建日志出现已有 GPUI Cocoa deprecated warnings 和缺少可选 PurismCore headers
提示；没有把它们当作 GPU/官方 Core 验收已通过的依据。

## 原生窗口、Timeline 与双视口

详细报告、来源与 trace 摘要见 [`r0-native-baseline.json`](r0-native-baseline.json)。
Editor 工作树 `target/runtime-r0-native/` 保留构建 JSONL、冻结的可执行文件、
场景参数/编译干扰采样器 `run.py`、逐场景 trace、现有 report 输出和截图。
二进制 SHA-256 为 `3af330bc4d4fe142181e9f7446ca5a3f208381cf1958568dc6018e10cb0d8d5a`。

Release/test-support 的 7 组原生窗口场景均出现 `benchmark_done`、`lost_events=0`，
并通过既有 trace report。采集前等待编译空闲，运行期间每 250 ms 采样编译活动；
最终采用的场景未检出并行编译。每组仅一个进程，不能代替 R2 要求的 5 对交替进程。
画布/控件注入频率 120 Hz，配置 120 个测量输入与 30 个预热输入；播放/Animation
画布与 number 场景沿用既有 runner 的 500 ms 预热/时间窗口规则。

以下是注入 GPUI 输入到首次关联 presentation 的单次进程 P50/P95，单位 ms，
不含操作系统输入传递，不是物理 input-to-photon 测量：

| 场景 | 来源 | P50 / P95 |
| --- | --- | ---: |
| 画布 opacity | Mao | 39.469 / 48.781 |
| 画布平移 / 缩放 | Mao | 39.682 / 52.388；47.225 / 72.038 |
| 单顶点 / 8 顶点拖动 | 合成控制 fixture | 39.818 / 48.851；38.919 / 45.186 |
| Warp / Rotation 拖动 | 合成控制 fixture | 39.326 / 48.878；39.066 / 48.406 |
| Properties opacity number drag | Mao | 51.314 / 59.180 |
| Properties SV 颜色拖动 | Mao | 54.899 / 60.194 |

另 3 组为 Mao 动画播放、暂停 Animation 画布平移、Timeline number 连续拖动。
播放 CPU evaluate P50/P95 为 0.766/1.132 ms；number 拖动为 0.766/1.354 ms。
暂停画布平移复用已接受纹理，不产生新的动画求值批次，因此其 playback 分组为空；
不能将空分组表述为动画 CPU/GPU 耗时为零。原始 trace 保留其窗口 compose/present。

合成控制 fixture 通过 SDK 创建与保存：两份 12×12 网格、根级 2×2 Warp、独立
单位 scale Rotation、低 opacity framing quad 和非对称内存纹理。其范围按既有
540 模型单位 Rotation axis 和 benchmark 的 40 逻辑像素 inset 校准；正式编辑逻辑与
report 断言没有修改。源为 SDK `examples/runtime_native_fixture.rs`，产物为
`target/runtime-r0-native/fixture-fit-project/`；结构、模型坐标及保存重开 frame
相等均通过，随后 native 四种拖动均通过。

独立的 Release 原生 Timeline 验收也通过：显式指定 Mao、required asset、16 个配置
样本，覆盖自然/密集 motion、候选像素/取消、播放、seek、画布导航、dock resize、SDK
求值对照和保存重开。自然场景 134 tracks / 779 keys，密集场景 134 tracks / 34,438
keys。drag→当前 Metal draw P50/P95 分别为 36.895/46.546 ms 与 115.224/165.723 ms；
该 harness 不测 compositor presentation，不与上表混用。完整结果位于
`target/runtime-r0-native/timeline/results.json` 并收录于 JSON 证据。

Release `headless_screenshot` 的 `viewports` 场景通过，输出
`target/runtime-r0-native/viewports/`：Modeling/Animation 两个同时存在的视口分别
验证独立像素和调度，例如 Animation 修改像素计数为 `(219166, 0)`。这只是原有
Editor 基线，不证明尚未实现的 Runtime 客户端或 R5 Preview 已隔离。

可复现的正式命令（可使用 Cargo 构建再运行；本轮先 `--no-run` 后直接运行其报告的
可执行文件，将编译与测量分离）：

```sh
cargo build --locked --release -p zipzi-editor --features test-support
# 环境与场景参数完整保存在 target/runtime-r0-native/run.py / native-summary.json。
# SDK 命令生成合成控制 fixture；窗口基准使用其 manifest，而非修改 Mao 资源。
cargo run --locked --release --manifest-path ../zipzi-2d/Cargo.toml \
  -p zipzi-sdk --example runtime_native_fixture -- /absolute/fresh/native-fixture

env -u ZIPZI_DEV_SESSION_DIR ZIPZI_TIMELINE_MODEL=/absolute/mao_pro.model3.json \
  ZIPZI_TIMELINE_REQUIRE_ASSET=1 ZIPZI_TIMELINE_SAMPLES=16 \
  cargo test --locked --release -p zipzi-editor --features test-support --test timeline_acceptance
env -u ZIPZI_DEV_SESSION_DIR ZIPZI_SCREENSHOT_SCENE=viewports \
  cargo test --locked --release -p zipzi-editor --features test-support --test headless_screenshot
```

### 已知失败与 R0 退出项

Mao 默认 deformation runner 选中的 Warp 在拖动时返回
`The parent deformation is singular at this point`，其整组 report 拒绝为 incomplete。
原始负例保存在 `deformations-attempt-0/` 与 `run-retry.log`，没有按通过计数。
合成 fixture 成功不代表该问题已解决；R2/R4 继续涉及这条真实模型路径时必须保留此
负例并明确处理，不得删除断言或将合成结果代替该真实模型通过。初次被编译打扰的
canvas 与 fixture 可达性校准过程也保留，不并入最终时延。

R0 退出检查：

- A01 源接口/意图/结果分类、Native 类型与事务失败审计、正式写入迁移归属均有产物。
- 起始两仓 revision、工作区差异、工具链/硬件/模型与参数有可复核记录；正式源未变。
- 默认预算、资源统计/回收缺口、后台清单、Python 支持、endpoint 和启动规则已冻结。
- 可复用合成 CPU/动画/内存图片和 native 控制 fixture 可运行；聚焦功能、CPU 基线、
  7 组窗口采集、Timeline 与双视口验收按上述范围完成；旧 coverage 与 Mao Warp
  负例明确保留。
- R0 准备交付完成，可进入 R1。A02–A20 的 Runtime/Connected 及 A16 改动前后比较
  仍未验收；R1–R7 没有实现，不以本阶段基线作为整体完成。
