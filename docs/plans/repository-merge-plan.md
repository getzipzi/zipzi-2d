# Kasane 仓库合并计划

状态：待实施。本次只编写计划，没有移动代码、修改远端或执行合仓。

编写日期：2026-10-23。目标是将 `kasane-editor` 并入 `kasane-2d`，形成一个 Git 仓库和一个主 Cargo workspace，同时保留 SDK、Python wheel 与 Editor 的独立职责、验证入口和发布产物。

## 1. 推荐决策与范围

| 项目 | 推荐方案 |
| --- | --- |
| 承载仓库 | 以现有 `kasane-2d` 为主线；本次不改 GitHub 仓库名或 URL |
| Editor 位置 | 整体导入 `apps/editor/`，保留内部 `crates/`、`scripts/`、`assets/`、`docs/`、`vendor/` 布局 |
| Rust 工作区 | 一个根 workspace、一个 `Cargo.lock`；移除导入后的 Editor 嵌套 workspace 清单与锁文件 |
| 历史 | 保留两仓完整历史和原提交对象；不用 squash 或改写现有远端历史 |
| 模块 | 保留 crate 名称、公开 API 和业务职责；本次不实施 Runtime 架构重构 |
| 发布 | SDK、Python wheel、Editor 继续分别构建和发布，不要求同步发版 |
| 许可证 | 保留 SDK MIT、Editor GPL-3.0-only、第三方各自声明；根目录同时展示两份许可证及组件适用范围，不标成全仓 MIT 或任选双许可 |
| 模型 | 根 `models/local/` 作为唯一约定位置，仍为本地忽略内容；外部 SDK 保持其原有目录结构 |
| 工具与文档 | SDK 路径优先保持；Editor 路径整体增加前缀；更新当前有效引用，保留历史证据 |
| Agent 导航 | 根目录提供短架构地图，模块说明和聚焦测试分层维护 |

相较于最初建议的根 `vendor/` 布局，本计划将 Editor vendor 留在 `apps/editor/vendor/`。这样能保留补丁的相对路径及 Editor 内部文档关系；根 Cargo patch 指向新位置即可。不要为目录外观同时搬迁所有 crate。

本次不包含：依赖大版本升级、统一所有 crate 的 edition、业务重构、清理历史大文件、改写旧基准数据、删除旧 checkout、归档远端仓库、建立新的自动发版系统。

## 2. 已核对的现状

以下是编写计划时的快照，实施前必须重新记录，不能当作已冻结的迁移基线。

| 项目 | kasane-2d | kasane-editor |
| --- | --- | --- |
| HEAD | `ec60e46268eafaf32861169350b0acc719743664` | `ddcf7889a96e2d0979729f1825f0387906985648` |
| 分支 | `main` | `main` |
| 主 workspace 成员 | 14 个 modules crate + 1 个 benchmark crate | 3 个 Editor crate |
| resolver / 默认 edition | 2 / 2021 | 3 / 2024 |
| workspace license | MIT | GPL-3.0-only |
| Rust 版本约定 | `rust-version = 1.94`；CI 使用 1.94.0 | 根清单没有统一 rust-version |
| Python | 根 uv 环境、Python 3.14、独立 Maturin wheel | Python 工具与脚本测试 |
| CI | Linux / macOS / Windows，当前使用 `--workspace` | 未发现受跟踪的 `.github` workflow |
| 普通受跟踪文件 | 831 个，9,679,690 字节 | 1,101 个，29,376,208 字节 |

文件体积依据当前工作树中受跟踪普通文件统计，合计约 37.2 MiB，不含 Git 历史、子模块内容、构建缓存和忽略的模型。Editor 的 vendor 约 10.1 MiB、文档约 9.0 MiB。规模本身不需要通过历史清理解决。

发现的迁移约束：

- Editor 通过相邻 `../kasane-2d` 引用多个 crate；锁文件不固定相邻源码版本。
- SDK 的 `modules/purism-core` 是 Git 子模块，基线 gitlink 为 `ad5127666ccffb34552d8e660852995858bee437`，不是需要并入主 workspace 的 Rust 包。
- 两仓 `.cargo/config.toml` 都是通过本地 exclude 排除的机器配置，包含 `/opt/homebrew/bin/sccache` 和本机并行度，未受 Git 跟踪。
- Editor 的 GPUI vendor 包及其独立 workspace 必须保留；主 workspace 不能自动吸收这些包。
- Editor 测试通过 `CARGO_MANIFEST_DIR`、`../..` 和相邻仓库路径寻找模型、SDK fixtures 与输出目录。
- Editor 的构建脚本从当前目录取 `LICENSE`、图标、二进制和应用输出；路径变动可能使构建成功但打包内容错误。
- `scripts/property-chain-benchmark.py` 记录两仓提交、源码哈希并打包源码；合仓后必须改为一个仓库的完整来源记录。
- Runtime 的两侧 inventory 工具和已生成索引包含源码路径；迁移必须保持其检查能力。
- 初次检查时 Editor 有两份已有未提交修改：`unified-object-authoring-ui.md` 和 `unified-object-authoring-ui-implementation.md`；文档检查结束时，还出现了 ordinary binding 相关源码与测试改动，说明当前仍有开发工作进行。本计划不接管这些修改，P0 应以实施时的新清单为准。

## 3. 目标目录与边界

```text
kasane-2d/                         # 远端名称暂时保持
├── Cargo.toml                    # 唯一主 workspace；统一 patch/profile
├── Cargo.lock
├── README.md                     # SDK / Python / Editor 开发入口
├── AGENTS.md                     # 全局边界与测试选择
├── MONOREPO.md                   # 完整模块地图
├── LICENSE-MIT                   # 原 MIT 正文及版权声明；适用 SDK 等既有 MIT 内容
├── LICENSE-GPL-3.0                # GPLv3 完整正文；Editor 明确为 GPL-3.0-only
├── LICENSING.md                   # 按组件/目录列明范围，解释不是任选许可
├── THIRD_PARTY_NOTICES.md
├── .github/workflows/
├── modules/                      # 现有 SDK、格式、渲染、Python；路径不变
│   └── purism-core/              # 现有子模块；保持 gitlink
├── apps/editor/
│   ├── README.md
│   ├── AGENTS.md
│   ├── DEPENDENCIES.md
│   ├── LICENSE                   # 保留 Editor GPL 正文
│   ├── crates/{editor,editor-core,editor-render}/
│   ├── assets/
│   ├── scripts/
│   ├── docs/
│   ├── models/README.md          # 指向根模型目录，保留 Editor 场景说明
│   └── vendor/                   # GPUI 补丁与第三方声明
├── docs/                         # 现有 SDK / 公共架构文档
├── tests/                        # 现有共享 fixtures
├── tools/                        # 现有 SDK 验证与新导航入口
├── models/local/                 # 忽略，不随 Git 分发
├── benchmarks/
├── third_party/                  # 保留原本受跟踪说明；外部依赖仍忽略
├── pyproject.toml
├── uv.lock
└── target/                       # 忽略；不同 worktree 使用自己的目录
```

依赖规则：Editor 可以依赖 SDK、project、core 和 render；SDK 与底层模块不得依赖 Editor 或 GPUI。Python 调用 Rust SDK，继续能在没有 GUI 的环境构建和使用。保持 renderer 的平台边界。

先保留两侧文档的原相对布局。根 `MONOREPO.md` 链接到 Editor 与公共 Runtime 设计的权威入口，不为统一位置制造两份可编辑副本。

## 4. 分阶段实施

### P0：建立迁移基线

交付：两仓准确 SHA、状态清单、受跟踪文件清单、已知验证结果、可恢复的 Git 备份引用或 bundle。

1. 重新读取两仓 README、AGENTS、HEAD、工作树状态、正在进行的任务与 worktree 情况。
2. 选择相互匹配的已提交源码作为导入基线。正在进行的工作先与其执行者协调落定；不擅自提交、stash 或丢弃其他任务的修改。
3. 记录两个 lockfile、Cargo dependency/features 基线、Python 配置、子模块 SHA、vendor 补丁清单。
4. 记录本地模型清单与哈希；本机配置和本地资产单独记录，不加入代码导入。
5. 复用适合的隔离 worktree，或创建 `codex/merge-editor-monorepo` 分支与隔离 worktree；后续操作只在迁移 checkout 内执行。
6. 按已有结果和本次变更风险补充聚焦基线测试；不要为了“有基线”重复跑所有昂贵套件。已有失败应明确记录，不能归因给合仓或靠删断言消除。

通过条件：两个源码基线已明确、可恢复，导入内容不依赖其他 checkout 的未提交文件。

### P1：保留历史导入 Editor

交付：只涉及历史接入和目录导入的独立提交。

1. 从固定 Editor 基线进行不带 `--squash` 的 subtree 导入，前缀为 `apps/editor/`；若工具不可用，使用等价的保留双亲历史导入方法。
2. 不改写原仓库提交，不强推旧分支；记录原 SHA 与导入提交的关系。
3. 逐文件比较导入前后的路径映射、blob 内容、可执行位和符号链接。此步骤只导入受跟踪内容。
4. 验证两侧基线提交均可在新历史中访问，并为 Editor 文件记录一次跨导入点的历史查询方法；不假定 `git log --follow` 对所有路径都自动贯穿 subtree 导入。
5. 保持 SDK 根 `.gitmodules`、gitlink 和 CMake/probe 路径不变。

通过条件：Editor 受跟踪内容完整落到前缀目录，SDK 内容未被覆盖，两仓历史可追溯。此阶段允许暂时存在双 workspace；不能作为最终交付状态。

### P2：合并 Cargo workspace 与构建配置

交付：一个可解析的主 workspace、统一锁文件和明确的包选择。

1. 根 workspace 加入三个 Editor crate。主成员共 18 个是当前基线预期，若实施前新增模块，按实际清单更新。
2. 使用 resolver 3；SDK 保持 edition 2021，Editor 三个 crate 显式保留 edition 2024，不在本次顺带升级 edition。
3. 保留根 SDK 的 MIT 默认，Editor 三个 crate 显式声明 GPL-3.0-only；检查 `cargo metadata` 实际结果，避免继承变更导致误标。
4. 根 `[workspace.dependencies]` 合并依赖与 features，保留 SDK 的 serde `rc`、serde_json `float_roundtrip` 等既有能力。Editor 的 crate 相对依赖与 dev-dependency 全部改为仓内路径。
5. 将 GPUI patch 放在根清单，路径指向 `apps/editor/vendor/...`；显式排除 vendor 包及嵌套 vendor workspace，检查实际依赖仍命中补丁和固定 Git revision。
6. 合并 profile：保留 Editor 的开发优化策略及本地 crate 的低优化设置，也保留 `kasane-python` release `strip = "none"`。SDK 从依赖变成 workspace 成员后，逐项核对实际生效策略。
7. 删除导入的 `apps/editor/Cargo.toml` workspace 清单和 `Cargo.lock`；不要删除 vendor 内部仍用于继承的清单。
8. 以 SDK 根锁文件为基础解析一次统一 lockfile；对照两份旧锁文件审查版本、来源和 features 变化。避免无关升级，无法保持的版本变动单列原因。
9. `default-members` 选择少量通用 CPU 核心包（建议 core/project/sdk），使根默认命令不启动 Editor、Python 扩展或压力测试。SDK 完整验证另有明确包集合。
10. 检查 Rust 1.94.0 是否仍能构建统一后的目标；若依赖确需更高版本，再同步 rust-version、CI 和文档并记录理由，不默认追随本机最新编译器。

通过条件：`cargo metadata --locked` 在根目录和 Editor 子目录识别同一 workspace/锁文件；包许可、补丁来源与目标集合符合预期。

注意：单锁文件并不保证单一编译产物或完全离线的 SDK 构建。Cargo 解析可能涉及其他成员依赖，features/profile/target 差异也可能产生独立产物。SDK 独立构建的验收指无需编译或运行 GPUI/Editor；不要宣称首次解析完全不接触其依赖来源。

### P3：改造路径、开发命令与基准工具

交付：所有当前有效的构建、测试和基准入口不再依赖相邻 checkout。

| 范围 | 具体处理 |
| --- | --- |
| Cargo 依赖 | 根依赖、Editor dev-dependency 全部指向 `modules/` |
| `dev.sh` | 区分仓库根与应用根；观察 Editor 源码、SDK modules 和根清单/锁文件；保留成功构建才重启、锁归属和进程退出处理 |
| dev 输出 | 继续隔离到根 `target/dev/`；每个 worktree 的 PID、锁、应用 bundle 均放在自己的 target 下 |
| `build-app.sh` | 从正确 target 查找二进制；图标取应用 assets；许可取 Editor LICENSE，不能误复制根 MIT 文件 |
| Cargo 输出定位 | 明确默认根 target；对支持的 `CARGO_TARGET_DIR` 与 target triple 使用 Cargo metadata/构建产物信息定位，不依赖调用目录猜路径 |
| Rust 测试 | 审计所有 `CARGO_MANIFEST_DIR`、`../../models`、`../../target` 和跨仓 fixtures；内部 `include_str!`/`include_bytes!` 路径仍需编译验证 |
| 本地模型 | 统一根 `models/local` 发现规则；显式指定不存在的模型路径仍失败，可选素材缺失仍明确 skip |
| benchmark | 一个仓库 SHA + dirty 状态、统一锁文件、源码哈希、模型哈希与环境；移除两次 Git 查询和重复归档 |
| 源码归档 | 用显式源码目录清单收集；不把受限模型、缓存、整个工作区或其他 checkout 混入归档 |
| Runtime inventory | 保留 Editor 应用根下的索引生成逻辑，修复跨侧引用；路径确实改变的生成索引重新生成并检查 |
| Python / CMake | 保持根 uv、wheel 模块和 native probe 路径；检查脚本工作目录改变没有影响调用者传入路径的含义 |

优先沿用各脚本现有参数，不额外构建通用任务框架。必要的根路径助手只覆盖反复出现的真实需求；crate 内的 fixtures 保持 crate 相对定位，不能依赖机器绝对路径。

同时更新已有 `test_dev.py`、`test_test_runner.py`、`test_property_chain_benchmark.py` 的测试夹具。验证退出码、参数透传、路径和进程隔离等行为，不能只把旧字符串断言改成新字符串后视作完成。

通过条件：从根目录、Editor 目录以及另一个工作目录用绝对脚本路径启动，入口均按其文档定位正确；测试不会误读旧相邻仓库，也不会操作另一个 worktree 的应用进程。

### P4：拆分测试范围与 CI

交付：聚焦入口保留，CI 三个平台的 SDK 验证继续存在，Editor 单独在 macOS 验证。

合仓后的 `--workspace` 包含 SDK、Python、benchmark 和 Editor，原命令不能原样照搬。`default-members` 不会缩小显式 `--workspace` 的范围。

| 入口 / CI job | 目标范围与要求 |
| --- | --- |
| `tools/test.sh fast/mesh/stress` | 保持现有 SDK 目标、筛选和压力矩阵语义 |
| `tools/test.sh regression/full` | 选择原 SDK workspace 的完整包集合，包含原先的 Python/benchmark 成员；不意外扩展到 Editor |
| `tools/test.sh python/wheel` | 保持开发扩展与 Release wheel 的隔离验收；wheel 在源码目录之外导入 |
| `apps/editor/scripts/test.sh fast` | 仍只运行 editor-core unit 与 synthetic integration |
| Editor `ui/regression/acceptance/full` | 显式选择三个 Editor 包；保持 test-support、原目标集合与 feature 统一关系；继续清除 `KASANE_DEV_SESSION_DIR` |
| SDK CI | Linux/macOS/Windows；保留 clippy、decoder safety、Rust regression、wheel、typing、negative controls 与现有平台条件 |
| Editor CI | macOS；构建、工具测试、headless 行为、synthetic Metal/native rendering；先验证 runner 的 Metal 能力 |
| 授权素材验收 | 在具备对应本地资产的环境跑；不上传素材到公开 CI，也不把素材缺失产生的 skip 写成完整验收通过 |
| 格式检查 | 按项目包集合或仓库入口统一执行；不扩大到 vendor 原始源码 |

SDK 包集合应集中定义，并与 Cargo metadata 检查其覆盖关系，避免各 workflow 复制多份名单后漂移。新增包应归入 SDK、Editor 或显式独立工具组。

变更触发规则：

- 仅 Editor 源码改变：Editor 对应门禁。
- SDK 模块改变：SDK 对应门禁，并执行 Editor 接入验证；不能因目录过滤漏掉消费者。
- 根 Cargo.toml、Cargo.lock、公共脚本、CI 配置改变：两侧门禁。
- Python facade 改变：Python 门禁；Rust 绑定或共享 SDK 改变还需相应 Rust 验证。
- 初次合仓先保证正确覆盖，再优化路径过滤。所需检查的 skipped 状态不能造成分支保护永久等待。

保留原有 `full` 的域内含义和无参数行为。README 和 AGENTS 明确日常使用聚焦入口，不能把默认全套悄悄缩减却仍宣称是原完整门禁。若需要仓库全量入口，明确命名并只在受支持环境执行。

通过条件：测试入口的失败正确向上传播；SDK CI 不编译 Editor；所有原门禁都有去向；需要真实 GPU 的验证有已确认可用的 runner 或明确的本机验收记录。

### P5：许可证、资产、文档和 Agent 导航

交付：清楚的许可范围、统一资源约定、可发现的开发入口。

1. 将根原 MIT `LICENSE` 重命名为 `LICENSE-MIT`，保留正文和版权声明；根增加 `LICENSE-GPL-3.0` 完整正文，Editor 自身的 LICENSE 继续保留。新增 `LICENSING.md` 列出 SDK、Editor、共享工具/文档以及第三方的适用范围；归属依据原文件来源与现有声明，不因目录移动重新授权。更新根 third-party notices 对 GPUI vendor 的索引。
2. 根 README 开头明确写出：SDK 使用 MIT；Editor 使用 GPL-3.0-only；本仓库按组件分别授权，Editor 不提供 MIT 选项。若使用徽章，分别标记 `SDK: MIT` 和 `Editor: GPL-3.0-only`，不得用全仓 `MIT` 或 `MIT OR GPL-3.0-only`。逐包审查 license 元数据；对 Editor 自有源码添加准确的 SPDX 标识，保留第三方原声明。检查 Editor app bundle、wheel/package 的许可文件和 notices 内容，尤其检查 MIT 文件改名后的打包包含规则。合仓不顺带改变授权条款。
3. 迁移前比较两仓 `models/local` 文件哈希；只补充缺失文件，相同路径但不同内容的文件先记录并明确选用版本，不覆盖或删除原件。验证 model3/纹理/CMO3/CAN3 的相对引用。
4. 根 `.gitignore` 覆盖 target、output、Python 扩展、缓存、授权素材和 Editor 子目录可能产生的输出；保留 `.gitattributes` 对 patch 空白的约定。
5. 本机 `.cargo/config.toml` 不直接提交；新 worktree 要么用可移植配置，要么使用显式的本机环境配置。以无本机配置的 fresh clone 验证构建入口。
6. 更新根 README、MONOREPO、Editor README/DEPENDENCIES、两侧 models README、当前 Runtime 文档互链、运行命令与产物位置。
7. 历史文档、旧 benchmark JSON 和旧提交 SHA 保留事实原貌；修复必要导航或加历史路径说明，不全局替换归档里的所有旧路径。
8. 根 AGENTS 仅保留架构地图、依赖方向、验证选择和局部说明入口；`apps/editor/AGENTS.md` 使用明确路径指向 Editor README，避免其 `Read ./README.md` 产生歧义。
9. 按需要给 SDK、Python 添加局部 AGENTS。避免全仓强制阅读所有历史文档，也不屏蔽 vendor 的按需审查。
10. 给常用搜索写出目录范围：编辑器任务先搜 `apps/editor/crates`，SDK 先搜 `modules`，当前架构查 MONOREPO 指向文档；vendor 和 archive 在有关任务时再展开。

GitHub 展示验收：在迁移分支推送并由 GitHub 完成识别后，实际检查 About 的许可证列表与许可证入口，确认 MIT 和 GPL 正文均可发现；检查 README 首屏能看到按组件授权的说明。GitHub 支持展示根目录的多份许可证，但自动识别不表示理解了目录适用范围，不能只凭一个徽章判定授权完整。若页面识别不理想，调整文件布局与说明，不修改标准许可正文来迎合检测。推送前的本地检测只是预检查，不替代线上页面验收。

通过条件：fresh clone 可以按文档开始工作；无相邻 checkout 依赖；本地资产保持未跟踪；包元数据和应用打包许可正确；Agent 可由根说明直接定位模块和最小验证命令。

### P6：总体验收与仓库切换

交付：验收记录、可审查的迁移提交序列、最终两侧来源 SHA 和新提交映射。

1. 在隔离迁移 checkout 中完成下节验收矩阵。迁移期间若原仓库继续前进，重新记录差异、接入尾部提交并重跑受影响验证，不能发布过期快照。
2. 确认本轮只有仓库组织、构建、路径、测试入口和文档适配，没有混入 Runtime 或其他业务功能。
3. 在一次干净 clone 和第二个 worktree 验证；不能靠原 checkout 的模型、环境或旧编译产物掩盖路径问题。
4. 按项目通常流程审查、合并迁移分支；迁移前后记录保存在主仓。
5. 主线切换后，新 SDK / Editor 改动统一进入主仓；旧 Editor 仓库转为历史入口的说明、远端归档和仓库重命名作为后续外部管理动作处理。
6. 原 checkout、Git 备份和本地资产保留到验收与正在进行的任务都完成迁移；不要通过清理磁盘来完成“切换”。

## 5. 最终验收矩阵

下面是实施时的要求，本文没有运行这些测试或宣称已通过。

| 编号 | 验收 | 完成标准 |
| --- | --- | --- |
| A1 | 文件与历史 | P1 导入内容和模式一致；两原始基线 SHA 可访问；Purism gitlink 未意外升级 |
| A2 | Workspace | 根和应用目录指向同一 workspace/lock；成员清单准确；vendor 排除正确；依赖图无 SDK → Editor/GPUI |
| A3 | SDK | 聚焦验证通过后，执行一次保留原覆盖范围的 SDK Rust 完整门禁；跨平台 CI 通过 |
| A4 | Editor | 工具与聚焦 UI 验证通过后，执行一次 Editor 完整门禁，保留 examples/doctests/native targets |
| A5 | Python | Debug 开发测试、Release wheel 安装、仓外 import 与 CPU acceptance；原 GPU/observe 门禁按原条件执行 |
| A6 | 应用 | debug/release app bundle 均成功；图标、许可、二进制位置正确；开发热重载成功，构建失败不会重启旧应用 |
| A7 | 本地素材 | 已提供模型的导入/保存/导出与动画验收成功；缺素材与显式错误路径的行为保持；列清 skip |
| A8 | 基准 | 至少一个代表性基准短跑能找到模型、产物、源码和提交信息；性能与工作量选择可比较的既有基线核对 |
| A9 | Runtime inventory | SDK 与 Editor inventory 的检查通过；索引路径与实际文件一致，不改变语义状态来掩盖缺项 |
| A10 | 隔离 | fresh clone 无相邻源码仍能构建；第二 worktree 不共用 PID/锁/应用输出，不读取另一个工作树的 SDK |
| A11 | 文档与许可 | 当前入口链接、命令可用；旧路径仅剩说明过的历史记录；包元数据与产物许可检查通过；根同时提供两份许可证，README 首屏明确范围，GitHub 页面展示已实际核对 |
| A12 | 资产与缓存 | 没有新增受限资产、原生扩展、target、机器绝对路径配置；本地模型原件未丢失 |

执行顺序：静态清单/路径检查 → 已有工具测试 → Cargo metadata 与聚焦构建 → 受影响模块/原生 smoke → 一次两域完整回归及 wheel 验收 → fresh clone/worktree 验证。只有新失败、新修改或未解决问题才扩大或重复昂贵测试。

构建配置改变时，至少核对一次代表性编辑链的结果和工作量；时间比较使用相同硬件、模式、素材、features 与预热条件。若只运行了短跑，应标明它验证的是入口可用，不能据此宣称没有性能回退。

## 6. 提交组织与回退

建议保持以下审查边界；P2/P3 中相互依赖的改动可以同提交，不能为了凑提交数量留下无法解释的状态：

1. 保留历史导入 Editor。
2. 合并 workspace、依赖、锁文件、profile 与 license 元数据。
3. 修复构建、开发、资源、测试 fixtures 与 benchmark 路径。
4. 拆分测试包集合与 CI，适配现有工具测试。
5. 完成资产约定、文档、Agent 导航和验收记录。

迁移分支阶段失败：保留诊断，在隔离分支修复或放弃该分支；原仓库和本地文件仍可继续使用。

主线合并后需要回退：用正常 revert 回退迁移范围，不 reset/强推公共历史。历史导入可能包含 merge commit，先根据实际提交图确定 mainline 与逆序，再执行回退；不能把它当普通单亲提交批量回退。

若合仓后已有新功能提交，先处理其依赖关系；不能仅恢复两个旧 SHA 就声称保留了后续工作。Git 历史恢复不覆盖忽略的模型和机器配置，这些使用 P0 的单独清单与原件恢复。

## 7. 实施时需要更新的决策记录

- 最终导入的两仓 SHA，以及尚在进行的工作如何接入。
- 统一依赖解析的版本差异、最终最低 Rust 版本、profile 与 features 差异。
- SDK 包集合、默认成员和 Editor CI 的实际 runner 能力。
- 同路径本地模型内容冲突的处理结果。
- 各验收项的命令、结果、skip、已知基线问题与尚未验证的平台。

仓库重命名、旧仓库归档和大规模目录整理不阻塞本次技术合并，留到主线稳定后分别决定。

## 8. 依据

- 本仓库：根 README、DEPENDENCIES、Cargo.toml，三个 crate 清单，scripts 开发/构建/测试/基准入口，vendor/README，models/README，当前对象创作与 Runtime 计划。
- 相邻 kasane-2d：根 README、MONOREPO、Cargo.toml、pyproject.toml、.github/workflows/rust.yml、.gitmodules、THIRD_PARTY_NOTICES、tools/test.sh、Python 工具和 Runtime inventory。
- [Cargo workspace 官方说明](https://doc.rust-lang.org/cargo/reference/workspaces.html)：共享 lock/target，根级 patch/profile，成员、default-members 与继承规则。
- [GitHub 多许可证展示说明](https://github.blog/changelog/2022-05-26-easily-discover-and-navigate-to-multiple-licenses-in-repositories/)：根目录多许可证的发现和导航。
- [GitHub 许可证识别说明](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/licensing-a-repository)：自动识别方式及复杂许可布局的限制。
