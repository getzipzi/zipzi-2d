# Zipzi 2D 仓库整合计划（修订版）

修订日期：2026-10-23。用户要求更彻底地合并：平铺所有自有 crate，统一 Editor 资产、工具和文档。**最终不保留 `apps/editor/` 项目壳，也不保留 `modules/` 与 `crates/` 两套源码布局。**

状态：原 P0–P3 已完成，其结果是可恢复的中间状态；新增 **P3F：目录平铺与二次路径适配**，作为下一阶段，完成后再执行修订后的 P4–P6。本次修订只移动并更新计划及导航，没有搬动业务代码、资源或子模块，以下目标布局尚未实施。

权威计划现位于 `docs/plans/repository-merge-plan.md`。执行事实和测试结果见[实施记录](../repository-merge/README.md)。旧版计划可在提交 `9e3c98cc` 的 `apps/editor/docs/plans/repository-merge-plan.md` 查看；不另存一份可编辑的旧计划。

## 1. 已完成基线与保持不变的边界

| 项目 | 当前事实 / 后续约束 |
| --- | --- |
| 工作仓库 | `~/web/zipzi-2d`，分支 `codex/merge-editor-monorepo`，upstream 为 kasane-2d；未创建或推送新 GitHub 仓库 |
| SDK 导入基线 | `e27230ab2fed1df4b4d0743d28fb1ae3fca3405e` |
| Editor 导入基线 | `b4e4abc737f88bdb895dfec960444dd3183369b7` |
| 历史 | P1 已保留双方完整历史；继续追加正常提交，不重新 subtree 导入、不 squash、不重写原提交 |
| workspace | 18 个自有成员，一个根 Cargo.toml / Cargo.lock，resolver 3 |
| 工具链 | Rust 1.95 已实测；1.94 无法编译现有 GPUI 的 `cold_path`，不回退已确认的最低版本 |
| 兼容性 | 未正式发布，直接更新仓内调用者；不维护旧目录镜像、兼容 symlink 或转发脚本 |
| crate 身份 | 保留包名、公开 API、职责和 edition：SDK 2021、Editor 2024；目录名统一采用包名 |
| 依赖方向 | Editor → SDK/core/project/render；底层与 SDK 不依赖 Editor/GPUI；Python 可无 GUI 构建 |
| 发布 | SDK、Python wheel、Editor 的构建、测试和发布产物仍独立，不要求同步发版 |
| 许可 | SDK MIT，Editor GPL-3.0-only，第三方保留原许可；位置统一不改变授权 |
| 外部工作 | 原 Editor 有另一任务的未提交基准改动；不代提交、不 stash、不丢弃。切换前接入其正式提交并复验 |

P0 的备份 refs、bundle、原始树清单继续保留。P1 的导入验证仍针对其不可变提交；P2/P3 的记录是历史证据，不修改成仿佛已经验证过新目录。新布局必须补跑对应验证。

本次仍不包含：业务或 Runtime 架构重构、依赖大版本升级、统一所有 edition、清理历史大文件、改写旧 benchmark 数据、删除旧 checkout、归档远端和自动发布系统。

## 2. 最终目录

```text
zipzi-2d/
├── Cargo.toml / Cargo.lock
├── README.md / AGENTS.md / MONOREPO.md
├── LICENSE-MIT / LICENSE-GPL-3.0 / LICENSING.md
├── THIRD_PARTY_NOTICES.md
├── crates/                         # 18 个自有 workspace 成员直接同级排列
│   ├── kasane-core/
│   ├── kasane-live2d/
│   ├── kasane-animation/
│   ├── kasane-project/
│   ├── kasane-moc3/
│   ├── kasane-cmo3/
│   ├── kasane-psd/
│   ├── kasane-moc3-psd/
│   ├── kasane-render/
│   ├── kasane-render-wgpu/
│   ├── kasane-render-metal/
│   ├── kasane-sdk/
│   ├── kasane-sdk-observe/
│   ├── kasane-python/               # 保留 python/、pyproject.toml 与独立 wheel
│   ├── kasane-editor/               # 原 crates/editor，包名不变
│   ├── editor-core/
│   ├── editor-render/
│   └── cubism-matrix-kasane/        # Rust benchmark runner 也平铺
├── assets/editor/                  # 所有 Editor 运行时/打包资产
│   ├── app-icons/
│   ├── icons/
│   └── locales/
├── vendor/                         # GPUI 补丁；保留第三方内部结构与声明
├── tools/                          # 合并两侧开发、测试、报告和 inventory 工具
├── docs/
│   ├── README.md                   # 合并后的文档导航
│   ├── ARCHITECTURE.md
│   ├── development/                # editor.md、dependencies.md 等具体指南
│   ├── plans/                      # 所有当前计划，本计划也在这里
│   ├── references/
│   ├── runtime/                    # Runtime 清单与当前公共说明
│   ├── repository-merge/           # 迁移清单和逐阶段验证证据
│   └── archive/                    # 历史文档与基准原始数据
├── tests/fixtures/                 # 共享合成 fixtures；crate 私有 tests 留在 crate
├── models/local/                   # 唯一本地授权模型约定；忽略
├── benchmarks/                     # 场景、协议、非 Rust runner；不再藏自有 crate
├── third_party/purism-core/        # 受跟踪 gitlink，版本不变
├── third_party/...                 # 其他外部 SDK 保持本地忽略
├── .github/workflows/
├── pyproject.toml / uv.lock
└── target/                         # 编译、应用、日志、PID、锁、测量输出
```

“平铺”指所有自有 workspace crate 同级放在根 `crates/`，不把每个 crate 的 src/tests 再打散。第三方 vendor 内的 crate/独立 workspace 不拆开、不并入主成员；Purism 是非 Rust 子模块，放在 `third_party/`。这两个例外不保留旧 Editor 项目边界。

## 3. 明确迁移映射与冲突策略

| 当前路径 | 目标路径 / 处理 |
| --- | --- |
| `modules/kasane-*`（14 个成员） | `crates/<package.name>/` |
| `apps/editor/crates/editor` | `crates/kasane-editor/`，包名仍为 kasane-editor |
| `apps/editor/crates/editor-core`、`editor-render` | `crates/editor-core/`、`crates/editor-render/` |
| `benchmarks/cubism-matrix/runners/kasane` | `crates/cubism-matrix-kasane/`；更新矩阵工具引用，其他 runner 不搬 |
| `modules/purism-core` | `third_party/purism-core`；更新 .gitmodules、CMake/probe/build.rs；gitlink SHA 保持 `ad5127666ccffb34552d8e660852995858bee437` |
| `apps/editor/assets/*`（当前 5 文件） | `assets/editor/app-icons/*` |
| `apps/editor/crates/editor/assets/icons/*`（当前 2 文件） | `assets/editor/icons/*` |
| `apps/editor/crates/editor/locales/*`（当前 2 文件） | `assets/editor/locales/*`；调整 i18n 宏及编译期路径 |
| `apps/editor/vendor/*` | `vendor/*`；Cargo patch/exclude、补丁说明与相对资源引用同步改 |
| 两侧工具 | 统一到 `tools/`，无 `tools/editor/` 整仓复制 |
| 原 SDK `tools/test.sh` | `tools/test-sdk.sh` |
| 原 Editor `scripts/test.sh` | `tools/test-editor.sh`；两个 full/no-arg 继续保留各自原覆盖范围 |
| Editor `dev.sh`、`build-app.sh` | `tools/dev-editor.sh`、`tools/build-editor-app.sh` |
| Editor 其余脚本与工具测试 | 合入根 tools，保留有意义的原名；统一一个根定位助手，消除 editor_root 假设 |
| `apps/editor/docs/{plans,references,archive}` | 合入 `docs/{plans,references,archive}`；当前未发现同路径文件冲突 |
| Editor README / DEPENDENCIES | 操作说明整理进 `docs/development/editor.md`、`dependencies.md`；主入口与公共依赖约定并入根 README/MONOREPO |
| Editor docs README | 合并成根 `docs/README.md` 的综合导航，不用 Editor 首页覆盖 SDK 导航 |
| Editor models README | 合并到根 `models/README.md`，保留素材来源、日期和场景记录 |
| Editor LICENSE | 根 `LICENSE-GPL-3.0` 保留完整正文；crate 的 Cargo 打包许可规则按产物验证 |
| Editor AGENTS | 公共原则合入根 AGENTS；GUI/核心测试特定约束放对应 crate 的局部 AGENTS |
| Editor `.gitignore` / `.gitattributes` | 合入根规则，调整 patch 匹配范围；不要直接覆盖现有规则 |

实施前生成完整的受跟踪文件映射，覆盖上表之外的文件；不能凭目录名称猜归属。对同名文件逐项比较：相同内容按来源记录去重，不同内容合并或按用途重命名，禁止覆盖。当前明确的脚本冲突是两份 `test.sh`，按表中名称解决。

公共文档按主题合并，不建立一个包含原来全部内容的 `docs/editor/` 新壳；Editor 专属指南可以是主题页。历史文件可原样移动，但保留原始命令、SHA、路径和数字，并增加统一历史路径说明。当前有效链接按目标文件的位置计算，不能全仓字符串替换历史数据。

资源集中并不包括把 crate 私有测试 fixtures 全部抽到 assets；应用图标、界面图标和 locales 集中，合成测试素材按是否共享留在 crate tests 或根 tests。授权模型始终只在忽略的 `models/local/`。

## 4. 修订后的实施顺序

### P0–P3：已完成，作为中间基线保留

对应导入、workspace 统一和第一轮路径改造。P3 完成提交 `9e3c98cc` 是此次结构调整的起点。原有测试通过不自动代表 P3F 后仍通过；保留旧记录，并新增验证结果。

### P3F.1：完整映射与目录平铺

1. 重新记录新仓库和两原仓 HEAD/status/worktree；保护外部任务未提交工作。
2. 生成逐文件 old→new 映射，包含 blob、模式、symlink、许可证来源、合并/删除理由；记录上述冲突的实际处理。
3. 用正常 Git 移动 18 个成员、vendor、Purism gitlink、运行时资产、工具与文档；不改写历史。
4. 保留并核验两基线祖先关系和 Purism 对象身份。更新 `.gitmodules` 与根 ignore，使 Purism gitlink 可跟踪而其他外部 SDK 仍忽略；在已初始化和未初始化状态验证路径迁移。
5. 移除空的 `apps/editor/`、`apps/`、`modules/`；不留下 symlink、备用副本或旧入口包装器。未处理文件必须在映射中有明确归属。

通过条件：原受跟踪内容都可由映射追溯，无文件静默丢失；18 个自有包全在 `crates/*`，第三方边界清楚。只有必要的路径/清单修改，不混入功能重构。

### P3F.2：构建、资源和路径二次适配

1. 根 members/default-members、workspace dependencies、crate path dependencies、patch/exclude 全部指向新位置；保持 P2 版本、features、profile、edition 和许可结果。纯搬迁不应引发依赖升级，意外 lockfile 差异必须解释。
2. Python wheel 的 pyproject/Maturin、uv 配置、开发环境、validator、仓外 import、typing 路径同步更新。检查 package include/license、README/API 文档链接和 CI wheel 收集路径。
3. 更新根 CMake、MOC3 build.rs、native probes、Purism 工具、Rust benchmark runner 对旧 modules/runner 的依赖，保留平台和原生 Core 可选条件。
4. 工具统一按根目录定位；开发 watcher 明确监视所需 crates、assets、vendor 及根 Cargo 文件；保留失败不重启、锁归属、late PID 恢复和两 checkout 进程隔离。
5. 重新调整 Rust `CARGO_MANIFEST_DIR`、`#[path]`、`include_*`、i18n 宏和资源打包路径。开发运行、headless test、debug/release bundle 都能找到根 assets；不要求启动目录有素材，也不依赖旧 Editor crate assets 副本。
6. root target/dev、PID/锁/日志约定不变；继续使用 Cargo artifacts 定位自定义 `CARGO_TARGET_DIR` / build target 下的二进制。caller-relative 参数语义保持。
7. 基准来源清单改为新 crates/assets/vendor/tools/tests 等显式目录；记录一个 SHA/dirty、锁文件、模型与源码哈希，不归档本地模型或整个工作区。
8. 两个 Runtime inventory 都从根目录定位源码，更新分类器中依赖旧路径的条件，重新生成路径与行号。合并后的 Editor 索引统一报告根相对路径，消除“同一份 JSON 中路径根不明”的问题；语义状态不因搬迁改变。
9. `verify-workspace.py` 等迁移工具更新到新布局；`verify-import.py` 保持验证旧 P1 的不可变树，另加 P3F 的文件移动/合并核验。
10. 更新工具测试的布局与调用路径，验证真实行为，不能只替换字符串断言。

通过条件：根目录、`crates/kasane-editor/` 和外部 cwd 的入口都按文档运行；metadata 的 18 个成员和 patch 正确，无 sibling 源码；SDK 独立构建与 Editor 全目标编译通过；工具、资源、fixture、inventory 聚焦测试通过。

### P3F.3：文档、资产与许可落位

1. 合并文档目录和 README，更新当前链接、命令、根 MONOREPO、AGENTS。用主题页保留专属知识，避免两份可编辑权威说明。
2. 根提供 MIT 与 GPL 正文，更新按组件/文件来源的 LICENSING 和第三方 notices。`crates/editor-*`、`crates/kasane-editor` 与共享 tools/docs/assets 中来自 Editor 的内容保留原 GPL 归属；不能因为放入根公共目录就标成 MIT。
3. 对合并文件记录来源和适用许可；必要时保留单独文件，避免错误的统一授权。第三方声明、vendored workspace 和 patch 不能被自有 crate 的 license 默认覆盖。
4. 使用文件映射、当前链接检查、旧路径检索及包元数据证明没有活动的旧目录依赖。历史证据里的旧路径按统一规则明确豁免。

通过条件：工程与文档不再以旧 Editor 项目根为边界，许可证仍可准确追溯。P5 负责素材与产物的最终核验，不把基本许可标识拖到结构合并之后才补。

### P4：按包集合划分测试与 CI

目录同级后不能再用 `modules/` / `apps/editor/` 区分测试范围。集中定义 SDK（原 15 成员，含 Python/benchmark）与 Editor（3 成员）集合，并用 Cargo metadata 检查完整覆盖、不重叠和新增包归属。

- `tools/test-sdk.sh` 保留 fast/mesh/stress/regression/python/wheel/full 的原范围与压力矩阵。
- `tools/test-editor.sh` 保留 fast/ui/regression/acceptance/tools/full 的原范围；ui 等显式选择三个 Editor 包，维持 test-support 特性与清除开发 session。
- 两个 full/no-arg 都是各自域内完整门禁；若加仓库全量入口则显式命名，不能偷换 full 含义。
- SDK Linux/macOS/Windows CI 保留 clippy、decoder safety、Rust regression、wheel、typing、negative controls 和已有平台条件，不编译 Editor。
- Editor macOS CI 覆盖构建、工具、headless、synthetic Metal/native；先确认 runner 的 Metal 能力，真实授权素材验收保留本机入口并记录 skip。
- 触发规则依据包依赖与公共路径：SDK 改动也触发 Editor 接入验证；root Cargo、公共 tools、CI 改动触发两域；assets/editor/、Editor crate 和相关 vendor 触发 Editor。格式检查不扩大到 vendor 原始源码。

通过条件：所有原门禁都有去向，失败向上传播，源码目录平铺后不会漏掉消费者或误编译 GUI。先正确覆盖，再优化路径过滤。

### P5：本地资产、发布产物与导航验收

1. 再次比较两原仓模型哈希，只补缺失文件到 root models/local；相同路径不同内容先记录和决策，不覆盖原件。验证 model3/纹理/CMO3/CAN3 相对引用。
2. 验证源码包/wheel/app bundle 的许可正文、notices、图标、locales 和 binary 路径。Cargo package 对根资产的包含方式必须实际验证，不能假定 crate 外文件会自动打包。
3. 根 ignore 覆盖新路径下的 Python native 扩展、target/output/cache/授权素材；不提交机器 `.cargo/config.toml`。
4. 根 README/MONOREPO/AGENTS 展示最终地图和明确包选择。局部 AGENTS 只保留真正局部的约束。
5. 若迁移分支已获授权推送，在线检查 GitHub 两份许可证与 README 首屏；本地检测不能代替线上识别。未推送时明确将该项列为未验证。

### P6：总体验收与切换

1. 接入原仓期间新增的已提交工作，使用 old→new 映射将尾部变更放入最终目录；记录最终来源 SHA 与新提交关系，不直接重新复制旧根目录。
2. 依次运行下述矩阵，先聚焦再扩大；仅新失败、新改动或未解决风险才重复昂贵套件。
3. 在干净 clone 与第二个真实 worktree 验证，使用各自 target/PID/锁/app，不借用旧 checkout 源码或构建缓存掩盖路径问题。
4. 按正常流程审查合并；新开发统一进入本仓。旧 checkout、bundle、原始模型保留，远端归档/改名是另外的外部管理动作。

## 5. 最终验收矩阵

| 编号 | 完成标准 |
| --- | --- |
| A1 历史和文件 | P1 原树验证不变；P3F 文件映射覆盖全部来源、合并和删除；两基线可达；Purism 移路径不升级对象 |
| A2 平铺 workspace | 自有 18 包全部 `crates/<package.name>`，无 apps/editor、modules、旧入口 symlink；单锁；license/edition/features/profile/patch 正确；无 SDK → Editor/GPUI |
| A3 SDK | 聚焦后一次原完整 Rust SDK 门禁；三平台 CI；Python/benchmark 没从范围中掉出 |
| A4 Editor | 工具与聚焦验证后一次完整 Editor 门禁，保留 examples/doctests/native targets |
| A5 Python | 新 crate 路径的 Debug 开发测试、Release wheel、仓外 import、CPU acceptance；GPU/observe 按原条件执行 |
| A6 应用与资源 | debug/release bundle、i18n、图标、GPL 正文正确；无旧目录资源依赖；热重载成功，构建失败不重启旧 app |
| A7 模型 | 唯一 root models/local；已提供模型导入/保存/导出/动画成功；显式缺失失败、可选缺失 skip；原件完整 |
| A8 基准 | 新入口短跑能找到模型/二进制/来源；源码归档含新目录且无授权模型/cache；代表性编辑链结果和工作量符合既有基线，短跑不证明性能无回退 |
| A9 Inventory | 两侧生成器统一根定位，分类与源路径一致；生成后 check 通过，不改变未迁移状态掩盖缺项 |
| A10 隔离 | fresh clone 不依赖相邻源码；第二真实 worktree 的 PID/锁/app/target 独立；Purism 初始化/未初始化按约定工作 |
| A11 文档与许可 | 当前链接与命令可用，只有明确标注的历史保留旧路径；根双许可及各包/产物许可正确；GitHub 展示单独记录验证状态 |
| A12 资产与缓存 | 未新增受限素材、native 扩展、target 或机器配置到 Git；源归档边界有实际检查 |
| A13 非 Rust 路径 | CMake/native probes、Python/Maturin、benchmark 场景与 runner 全部使用新路径；保持原 optional native Core 行为 |

执行顺序：映射/路径/许可静态检查 → 工具测试 → metadata/聚焦编译 → fixtures/资源/原生 smoke → 一次两域完整回归与 wheel → fresh clone/worktree → 外部页面验收（若已推送）。

## 6. 提交与回退

保留已完成 P0–P3 的提交；后续建议按可审查边界提交：目录/清单及必要构建路径、工具/资源/inventory、文档/许可落位、测试与 CI、最终验收。目录移动与其必需适配可以同提交，不为纯 rename 提交留下无法构建的长期状态。

记录每次搬迁/合并后的文件映射。失败时在迁移分支修复；不 reset 或强推原仓。若主线之后需要回退，用正常 revert，并区分 P1 双亲导入与后续普通提交。Git 回退不恢复忽略的模型或机器配置，这些保留原件与 P0 清单。
