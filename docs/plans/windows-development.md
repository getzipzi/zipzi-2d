# Windows 原生开发支持实施方案

状态：P0–P4 实施与验收中；Windows 分发闭环已通过，干净 checkout 和双平台最终验收待完成。更新日期：2026-10-28。

目标是在 Windows 原生环境中完成 Rust SDK、Python SDK 和桌面 Editor 的开发、
聚焦测试与调试，并保持 macOS 开发能力。日常开发不依赖 WSL、Git Bash 或 MSYS。
项目尚未发布，允许破坏性重构；不设计旧开发入口、旧协议或旧配置的迁移层。

执行顺序固定为：**开发工具 → 平台服务与文件语义 → Runtime IPC → Python 开发与安装验证
→ 基础设施验收 → GPUI/WGPU 与桌面 Editor 接入**。
Windows 图形原型、GPUI 合成改造和真实窗口验收全部放在最后阶段，不作为前期工作的前置条件。

## 当前基础与差距

本清单依据当前源码和开发配置整理，不代表已通过 Windows 实机验收。
实施前应复核相关文件，保留工作区已有改动；阶段完成必须记录实际执行证据。

| 范围 | 当前基础与需要处理的事项 | 主要位置 |
| --- | --- | --- |
| 开发入口 | 日常测试、Editor 启动和 SDK 实验依赖 shell；路径辅助函数仅识别 `/` 开头的绝对路径；开发进程依赖 `ps`、Unix 信号和 `.app` 目录 | `tools/test-sdk.sh`、`tools/test-editor.sh`、`tools/dev-editor.sh`、`tools/dev-python`、`tools/workspace/workspace.sh` |
| Python 环境 | `fcntl`、`venv/bin/python`、`cwd="/tmp"`、无扩展名的 host 路径仍存在；部分基准工具只发现 `_native*.so`；已有 Cargo JSON 产物发现逻辑可复用 | `tools/python/dev_python.py`、`crates/zipzi-python2/tools/`、`tools/python/` |
| Runtime IPC | endpoint、权限、所有权锁、冷启动和连接都依赖 Unix；Python 拒绝非 POSIX；协议认证上下文及二进制处理也依赖 `local` 和 `cfg(unix)` | `crates/zipzi-runtime/src/local.rs`、`src/protocol/`；`crates/zipzi-python2/client/python/zipzi/_endpoint.py`、`backends/connected.py` |
| 文件与源码身份 | 项目锁已有 Windows 分支；目录同步、覆盖发布、文件占用需要实测；SDK 构建身份使用本机路径字符串和原始文件字节 | `crates/zipzi-project/src/filesystem.rs`、`crates/zipzi-runtime/build.rs`、`.gitattributes` |
| 平台服务 | Settings 已有 `%APPDATA%` 分支，键盘策略已有 Windows 分支；目录解析与 host 查找仍需统一；Windows IPC 尚未实现 | `crates/zipzi-editor/src/settings.rs`、`project.rs`、`transfer.rs`、`menus.rs`、`ipc.rs`；`crates/zipzi-editor-core/src/keyboard.rs` |
| 场景渲染 | 已有共享 WGPU 后端、`WgpuSceneRenderer` 和 macOS WGPU→Metal 预览接入；Editor 的帧、设备、overlay 与显示仍直接使用 Metal | `crates/zipzi-render-wgpu/`、`crates/zipzi-editor-render/src/wgpu.rs`、`wgpu_macos.rs`；`crates/zipzi-editor/src/preview.rs`、`presentation.rs`、`viewports/` |
| GPUI 合成 | 锁定版本的 Windows 后端使用 D3D11，`draw_surfaces()` 尚无绘制实现；本地 `SurfaceSource` 只包含 Apple 平台类型，不能直接显示 WGPU 输出 | `vendor/gpui-fast/src/elements/surface.rs`、`vendor/README.md`；锁定依赖 `gpui-fast-windows 0.1.7` |
| CI 与交付 | SDK CI 已有 Windows 矩阵但仅手动触发，且跳过部分入口测试；Editor 和 Python 包验证仍以 macOS 为主 | `.github/workflows/rust.yml`、`editor.yml`、`package-python.yml`、`tools/tests/` |

本机 `.cargo/config.toml` 中的 Homebrew/sccache 路径已被忽略，不属于提交到仓库的阻塞项。
不把现有 macOS WGPU 验证、空实现的成功退出或条件编译排除视为 Windows 功能完成。

## 架构约定

- **工具只有一份实现。** 使用 Python 组织开发任务，shell 如保留仅作薄包装；不维护另一套 PowerShell 业务逻辑。Rust、uv、CPython 版本沿用根 README，Windows 使用 MSVC 工具链与 Windows SDK。
- **平台差异集中。** 路径、文件锁、进程生命周期和本地传输分别使用明确接口，不让操作系统判断散落到业务逻辑，也不先建立无实际调用方的大型平台框架。
- **协议独立于传输。** Runtime 的会话、请求恢复、二进制帧和预算语义跨平台一致；Unix 使用 Unix socket，Windows 默认采用 Named Pipe。端点标识不再要求所有平台都是 `<profile>.sock` 路径。
- **保留纯 Python 客户端。** Connected 的 Windows 支持不依赖安装 Local 原生 adapter；Win32 调用封装在客户端的 Windows 传输模块内。
- **验证随阶段加入。** 基础设施在 Windows 原生 runner 上执行；有 GPU 的测试单列，缺少设备时明确报告缺失覆盖，不以跳过冒充验收通过。
- **最后接图形。** 前期用无 UI 的 Runtime fixture 验证连接与生命周期。保留当前 macOS Metal/WGPU 路径，Windows GPUI/WGPU 接入在 P5 处理。

## P0：跨平台开发工具与最小 CI

交付统一入口，建议为 `tools/dev.py`。下面是拟议命令，实施后才成为可执行接口：

```text
uv run --locked python tools/dev.py test sdk fast <filter>
uv run --locked python tools/dev.py test editor fast <filter>
uv run --locked python tools/dev.py test tools -p test_<tool>.py
uv run --locked python tools/dev.py python script.py
uv run --locked python tools/dev.py editor
```

工作内容：

- 先迁移测试分派和通用辅助工具，复用 `cargo_domain.py` 的完整包域校验以及 `workspace_tools.py` 的 Cargo JSON 产物解析。
- 保留聚焦测试模式、过滤器透传、`--` 分隔、退出码及 Editor 测试清理 `ZIPZI_DEV_SESSION_DIR` 的语义。无参数使用帮助或低成本入口，不默认为昂贵全量测试。
- 统一调用方相对路径、工作目录、环境变量、`os.pathsep`、venv 解释器位置、临时目录、可执行文件后缀和文本编码。尊重自定义 Cargo target 目录与 target triple。
- 提取开发进程启动、归属判断、退出与清理；Windows 显式处理进程树。保留多 checkout 隔离、日志和 readiness 语义，不能杀死复用的常驻 host。
- Editor launcher 的平台进程控制先用合成子进程验证；此阶段不要求启动真实 Windows Editor。
- 为跨平台工具和 SDK/Editor Core 的聚焦 CPU 测试增加 Windows CI，保留现有 macOS 覆盖；修改相关文件的 PR 运行低成本门禁，昂贵全量测试仍单独触发。

验收：Windows 原生 shell 可运行工具测试和选定 CPU 测试；空格/中文路径、仓库外调用、
参数与退出码透传、自定义 target 路径通过。测试替身不依赖 POSIX shebang、`chmod +x` 或 Bash。
Python SDK 环境的真实构建与运行在 P3 验收。

## P1：平台目录、文件语义与源码身份

工作内容：

- 集中解析配置、缓存、临时目录、用户主目录与 host 候选位置。保留显式环境变量覆盖；区分开发 host 和安装 host，避免依赖 `/Applications` 或 `$HOME`。
- 整合现有 `%APPDATA%` 分支，明确目录缺失时的错误/回退策略，避免把持久设置静默保存到临时目录。UI 使用这些服务的最终接线留在 P5。
- 保留已有 Windows 项目锁，验证多个进程的排他性、文件被占用时的失败语义、临时文件清理、覆盖保存和失败恢复；明确目录同步在各平台的持久性保证。
- 覆盖盘符、UNC、中文/空格路径、大小写别名、相对路径和越界路径；符号链接/reparse point 用例明确权限前提，不整组跳过普通文件行为。
- SDK 源码身份使用规范的仓库相对路径（`/` 分隔）及稳定排序；制定文本 LF 策略，并处理已有 CRLF checkout。区分规范源码身份与产物原始字节哈希，不改写二进制内容。
- 补齐配置写入和 recent files 路径测试，检查所有依赖 `/` 或 `HOME` 的回退逻辑。

验收：同一组规范源码输入在 Windows/macOS 得到同一源码身份，真实源码改动仍改变身份；
两平台通过锁与保存失败恢复测试，明确记录不能承诺的文件持久性语义。

## P2：跨平台 Runtime IPC 与无 UI host

工作内容：

- 拆分 `local.rs`：通用 framing、连接身份、调度消息、预算与断开逻辑独立于 OS；Unix/Windows 模块只负责端点、权限、读写、超时与关闭。
- 将协议层的 `Auth`、请求处理和二进制处理移出 Unix 条件编译边界；同步协议测试，确保 Windows 真正执行相同的请求恢复与二进制契约。
- Windows 使用本机 Named Pipe，并显式限定当前用户访问、拒绝远程客户端、校验对端身份。权限校验不能以 Unix mode bits 的无效模拟替代。
- 重新定义启动仲裁接口，不把 Unix FD 继承暴露为公共契约。Unix 可保留现有实现；Windows 的所有权与就绪握手必须覆盖父进程退出、竞争启动、启动失败与接管。
- Python transport 提供相同的有界读写、连接超时、握手、关闭与错误映射；调整 endpoint 参数和生成文档，不要求 Windows 调用方提供 `.sock` 文件路径。
- 让 `local_host_fixture` 在 Windows 真正运行 Runtime，作为前期验收 host。Editor 的窗口唤起与 UI 线程接线留在 P5。

验收：安装纯客户端后，可在 Windows/macOS 完成连接、并发冷启动、多客户端、断线重连、
超时、host 崩溃后恢复、大二进制分块、错误身份拒绝和预算检查。客户端退出不终止常驻 host，
启动竞争不会产生两个有效所有者。测试不通过空 `main()` 或整体 `cfg(unix)` 排除来通过。

参考：[Named Pipe 权限模型](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)。

## P3：Python SDK 开发环境与分发验证

工作内容：

- 用 P0 的锁、路径和产物工具替换 `dev_python.py`、`_support.py`、`test.py`、`connected.py`、`distribution.py` 的 POSIX 假设。
- 保留脚本、`-c`、`-m`、stdin、REPL、工作目录和退出码行为；构建输出留在 stderr，清理 `PYTHONPATH`/`PYTHONHOME`，开发与正式测试环境继续隔离。
- 为原生扩展采用版本化发布目录或等效机制，避免覆盖 Windows 中正在加载的 `.pyd`；新进程使用新构建，旧进程继续使用原版本，清理不能删除仍被使用的版本。
- 所有 host 和 native adapter 从 Cargo 产物消息定位；扩展名来自解释器配置，不硬编码 `.so`、`.dylib` 或无扩展名可执行文件。
- 在 Windows 构建并安装纯客户端和 native wheel；从源码归档构建，在隔离环境中验证 Local 与 P2 的 Connected 路径及正负静态类型测试。
- 清理日常测试和交付门禁所依赖的基准辅助代码。macOS 专用性能/沙箱研究工具标明平台限制，不让其阻塞通用开发入口。

验收：Windows 可执行源码 SDK 实验、并发准备、失败后重试、新旧扩展进程并存；
安装 wheel 后从仓库外运行 Local/Connected 工作流。此阶段不要求连接桌面 Editor，也不要求 GPU observation。

## P4：基础设施收口与开发文档

工作内容：

- 汇总 Windows/macOS 的工具、SDK、Editor Core、IPC、Python 安装验证门禁；删除已被跨平台测试替代的整组 Windows skip。
- 更新根 README、tools README、`dev-python` 参考、Python SDK 使用说明和相关 CI 调用；文档明确哪些命令可用，哪些仍等待 P5。
- 在干净 Windows checkout 上复现依赖同步、CPU 测试、源码 Python 运行、wheel 安装、fixture 冷启动与重连；记录 OS、工具链、命令和结果。
- 保持包域覆盖完整；平台专项测试单列，不用删除包域成员规避尚未完成的 Editor 图形工作。
- 记录待接入的 Editor host 接口和图形错误清单；此时仍不要求整个 Editor `--all-targets` 在 Windows 通过。

验收：Windows 开发者可仅依照文档完成非图形开发闭环；必需门禁在原生 Windows runner
实际执行。P0–P4 完成后才开始新的 GPUI/WGPU 集成工作。

## P5：最后接入 GPUI/WGPU 与桌面 Editor

前置条件：P0–P4 已验收。复用已有 `WgpuSceneRenderer` 和 macOS WGPU 路径，
先复核届时锁定的 GPUI Windows 后端能力，再实现显示与合成。

工作内容：

- 建立 Editor 图形后端边界，收拢 Metal 设备、纹理、队列、完成通知与目标格式；UI 持有与平台无关的帧描述和生命周期，保持不可变帧、过期结果拒绝与双视口隔离。
- 调整 Cargo 平台依赖、测试和 example 声明，避免 Windows 无条件编译 Apple crate；实现 Windows WGPU 场景输出和 GPUI 显示连接。
- 首先验证窗口合成原型。WGPU 回读再上传 GPUI 图片可作为功能接通路径；GPU 直接互操作或 GPUI 渲染后端调整作为性能路线，在此阶段根据实际兼容性和测量选择，不能假定 WGPU 纹理可直接交给 D3D11。
- 接入 overlay、网格预览/缩略图、动画、双视口、裁剪、透明度、圆角和弹出菜单遮挡；若采用回读路径，记录拷贝开销、帧延迟和内存上限，并据验收结果决定是否需继续 GPU 直连。
- 把 P1 平台服务、P2 IPC 和 P0 launcher 接到真实 Editor：支持 `--no-window`、重复启动唤起、连接客户端时关闭最后窗口后的常驻行为，以及显式退出与未保存文档处理。
- 补 Editor Windows manifest、图标、DPI 和窗口初始化；验证中文 IME、快捷键、文件对话框、拖放、多显示器缩放和开发重启。
- 建立 Windows Editor 编译与原生 GPU/窗口验收；生成可运行的开发便携产物，检查资源与 notices。正式安装器、签名和公开发布另立工作，不阻塞本轮开发支持。

验收：Windows 可启动真实 Editor，导入、编辑、预览、保存并重开模型；Python Connected
与人工编辑共享 Runtime、Undo/Redo 和动画状态；双视口及 overlay 正确显示；开发重启可用。
真实 GPU 验收记录机器、后端、像素结果与性能，macOS 现有 Metal/WGPU 路径保持回归覆盖。
若显示原型可用但交互延迟或资源增长未达到约定标准，P5 保持未完成。

## 验证与进度记录

每阶段先使用根 [README 测试选择表](../../README.md#tests)，聚焦修改的模块和工具；
测试模式与 CI 覆盖见 [tools README](../../tools/README.md#test-coverage)。
Editor 测试继续清理 `ZIPZI_DEV_SESSION_DIR`。只有宽范围改动或阶段收口需要昂贵全量回归。

| 阶段 | 状态 | 完成证据 |
| --- | --- | --- |
| P0 工具与最小 CI | 实施中 | 统一测试分派、跨平台锁和进程生命周期、shell 薄包装与 CI 配置已实现；Windows 聚焦工具与 CPU 测试通过，远端 CI/macOS 回归待验证 |
| P1 平台与文件语义 | 实施中 | 源码身份规范化 2 项、平台配置目录 2 项、配置保存 2 项 Windows 聚焦测试通过；完整文件语义与两平台身份比对待完成 |
| P2 Runtime IPC | 实施中 | Named Pipe、纯 Python 传输和冷启动仲裁已接线；Windows 真实 fixture 握手、4 进程竞争启动与崩溃重启通过；完整安装后契约待验证 |
| P3 Python 开发与分发 | 实施中 | Windows 源码 Local adapter 构建、版本化发布、仓库外调用及并发准备通过；wheel/sdist 与完整测试工具迁移待完成 |
| P4 基础设施收口 | 实施中 | 已补充根 README、工具入口与 Python Windows 文档；干净 checkout 和双平台 CI 最终验收待完成 |
| P5 GPUI/WGPU 与桌面验收 | 待实施 | 待记录 |

完成证据应包含变更范围、实际命令、操作系统/工具链、结果和未覆盖项。源码调查、
macOS 交叉编译或 CI 配置存在都不能单独证明 Windows 运行行为。

### 2026-10-28 首批实施

分支：`codex/windows-development-p0-p4`。环境：Windows x86_64、Rust
1.95.0 MSVC、uv 0.13.6、工作区 CPython 3.14。`uv sync --locked` 成功。

`uv run --locked python tools/dev.py test tools -p test_dev_entrypoint.py -v`
通过 6 项测试，覆盖空参数不构建、过滤器与分隔符、失败短路、Editor session
隔离、真实子进程退出码、中文/空格目录中的跨进程排他锁及释放。

当前 `tools/dev.py test` 可分派已有测试模式；空参数显示帮助。Python SDK
相关模式的底层工具仍待 P3 移植，真实 Windows Editor 仍待 P5。旧 shell
入口在后续批次收敛为薄包装；本记录不代表 P0 或 P0–P4 已验收。

继续验证：入口控制已扩充至 9 项并全部通过，新增 full 格式失败不安装客户端、
调用方相对 target 路径与仓库外帮助测试；`test_tool_discovery.py` 的 1 项测试通过。
`uv run --locked python tools/dev.py test editor fast timeline::` 在 Windows 原生
执行并通过 50 项集成测试（库目标 52 项被过滤）。包域校验仍覆盖 17 SDK + 3 Editor。
新增 `.github/workflows/development.yml`，相关 PR 在 Windows/macOS 执行这些工具门禁
和 SDK physics/Editor Core timeline 聚焦测试；远端 CI 尚未执行，不能视作两平台验收。

测试入口迁移：`test-sdk.sh` 和 `test-editor.sh` 现仅转发到 `tools/dev.py`；无模式
显示帮助，完整门禁须显式指定 `full`。替换原有 POSIX stub 测试，Windows 原生
`test_test_entrypoint.py` 4 项、`test_test_runner.py` 6 项全部通过；SDK CI 的入口测试
不再跳过 Windows。SDK physics 聚焦门禁实际通过 7 项，1 项原有性能测量忽略。
进程 launcher 仍待迁移，P0 尚未完成。

进程平台层：新增 `tools/workspace/dev_process.py`，以 PID、创建时间与可执行文件
验证归属；只终止已确认 owner 及其子进程树。开发依赖增加锁定的 psutil。
Windows `test_dev_process.py` 验证错误身份拒绝终止、真实子进程树清理与无关
进程保留、中文 cwd/UTF-8 日志及退出码。该平台层尚待接入 launcher 的 readiness
与常驻 host 复用流程；Windows 真窗口关闭/保存握手仍属于 P5。

生命周期核心：新增 `dev_lifecycle.py`，用原子 JSON 归属记录与每次启动独立的
PID/readiness 文件处理重启、启动超时与恢复；`reused:<pid>` 不会转化为 launcher
所有权。Windows `test_dev_lifecycle.py` 4 项通过，覆盖构建失败保留旧进程、重启、
超时恢复、checkout 隔离与复用 resident。后台 reaper 不绑定 host 生命周期。
尚需将核心与 Cargo 构建、macOS bundle、watch 以及统一 `editor` 命令接线；
旧 `dev-editor.sh` 尚未替换，P0 保持实施中。

统一 Editor 接线初版：`tools/dev.py editor watch|reload|run|stop` 已连接 Cargo JSON
产物定位、独立 `target/dev`（可用 `ZIPZI_DEV_TARGET_DIR` 覆盖）、发布、macOS bundle
构造与文件监听。发布在旧进程退出后进行；未归属的同路径进程会阻止重启。
Windows `test_dev_editor.py` 3 项通过，生命周期 4 项复测通过。尚未完成 watcher
竞争/退出与 macOS 真 bundle 回归；旧 shell launcher 的替换将在这些控制补齐后进行。

Watcher 控制：旧 `test_dev.py` 的 C/POSIX fixtures 已迁移为原生 Python 合成进程，
5 项 Windows 测试通过，覆盖重启后退出、第二 watcher 拒绝、并发 reload 拒绝、
仅有 PID 不算 readiness，以及 resident 复用后 watcher 不认领进程。修复观察记录
与删除记录的竞争。`dev-editor.sh` 已转为统一入口薄包装，shell 文件固定 LF。
macOS 实机 bundle 行为尚待验证；旧测试中的中断恢复、拒绝 TERM 等细项仍需补齐。

异常恢复补充：生命周期与 watcher 各 6 项 Windows 测试通过，新增真实 launcher
中断后恢复、停止失败保留归属且不启动新进程，以及外部 reload 空档内 watcher
存活。停止失败使用注入控制；尚未在 POSIX 验证真实忽略 TERM 的子进程。

P1 源码身份初版：build script 按 `/` 分隔的仓库相对路径排序与编码，已知文本输入
统一 CRLF→LF，孤立 CR 保留；产物哈希未改动。源码扩展和 Cargo 文本增加 LF
checkout 策略。聚焦 `source_identity` 测试验证换行等价、真实内容差异及中文相对路径；
两平台完整源码身份比对仍待执行。

平台目录与配置：Editor Core 新增 `platform_paths`，集中处理配置、缓存、用户目录，
保留 `ZIPZI_CONFIG_DIR` / `ZIPZI_CACHE_DIR` 覆盖，缺少持久目录时明确报错。
`ConfigStore::from_platform` 提供调用入口；UI 最终接线留给 P5。配置读写拒绝
带路径分隔符/盘符的文件名，保存关闭临时句柄后发布。Windows 原生
`cargo test --locked -p zipzi-editor-core --lib platform_paths::` 2 项通过，
`cargo test --locked -p zipzi-editor-core --lib storage_tests::` 2 项通过；均清理
`ZIPZI_DEV_SESSION_DIR`。独占占用测试确认失败保留旧内容、清理临时文件且释放后可重试。

文件语义继续验证：Windows recent files 原有 2 个 Unix 根路径断言失败，已改为
原生路径测试，并用 `std::path::absolute` 处理相对路径，取消 `/` 回退；3 项复测通过。
`zipzi-project --lib filesystem::tests` 3 项通过，包含真实子进程的锁排他与释放、
中文/空格路径、Windows 盘符与 verbatim prefix。新增测试使用 workspace tempfile
开发依赖，以 `cargo test --offline -p zipzi-project --lib filesystem::tests` 更新锁文件并执行。
`cargo test --locked -p zipzi-project --test project_tests
package_failures_rollback_and_report_recovery_copy_when_rollback_fails` 通过 1 项，覆盖
多种注入的发布/回滚失败。UNC、大小写别名及 reparse point 权限专项仍待补齐。

Windows 路径专项：文件系统控制扩充至 6 项并通过，实际验证大小写别名、
占用目标保留原文件且解除占用后可覆盖，以及符号链接创建/拒绝/解析（本机未触发
权限缺失分支）。recent files 4 项通过，新增盘符与 UNC lexical root 及越根 `..`
规范化。UNC 网络共享实际读写、非符号链接 reparse point 仍未验收。已在项目 README
明确 Windows 目录持久性限制，并将这些聚焦测试加入 Windows/macOS Development CI。

P2 首批：`local.rs` 保留公共 Auth、Inbound、framing、预算与连接分派；Unix 端点、
权限、所有权与 socket 配置移至 `local/unix.rs`。协议 receive/receive_encoded 与
receive_binary 移出 Unix 条件边界，binary 契约测试取消整组 Unix 限制。Windows
`cargo test --locked -p zipzi-runtime --test binary --test protocol` 实际通过 6+11 项。
framing 与 Unix endpoint 权限测试已拆分，前者在 Windows 也执行。
Named Pipe、Windows host fixture 和 Python transport 尚未接入，不能据此宣称 IPC 完成。

P2 Win32 安全基础：添加 windows-sys 平台依赖及 `local/windows_security.rs`，
读取当前 token SID、校验指定进程的 token 用户，并构造当前用户专属且不继承的
安全描述符。`cargo test --offline -p zipzi-runtime --lib windows_security::` 在 Windows
通过 1 项真实 Win32 测试（身份匹配/错误身份拒绝、有效 DACL 且仅 1 条 ACE）。
依赖变更已写入 Cargo.lock。此时安全模块尚未用于真实管道，仍需管道访问拒绝、
远程客户端拒绝和对端身份测试，不能替代 IPC 验收。

P2 Named Pipe I/O：新增 `local/windows_io.rs`，创建显式私有 DACL、拒绝远程客户端的
byte pipe，使用 overlapped ReadFile/WriteFile/ConnectNamedPipe；超时 CancelIoEx 后
等待完成回收，保证缓冲区与 OVERLAPPED 生命周期。首实例创建带
FILE_FLAG_FIRST_PIPE_INSTANCE。`cargo test --locked -p zipzi-runtime --lib windows_io::`
在 Windows 实际通过 2 项，覆盖双向读写、读超时后重用、断开、首实例冲突和
2 MiB 阻塞写超时取消。当前尚未连接 Runtime listener、所有权仲裁或 Python 客户端。

P2 listener 接线：新增 `local/windows.rs`，以用户 SID、runtime directory 的规范字符串
哈希及 profile 组成管道名；首实例句柄取得所有权，accept 后先建下一实例再交付连接，
避免所有权空档。对端 PID token 必须属于当前用户，最多 8 个连接线程；关闭服务
取消接收并回收连接线程。Windows `--lib local::windows::` 的真实帧分派、重复 claim
拒绝及退出后重新 claim 测试通过。fixture 移除 Windows 空 main，接入共同 Runtime
循环。Python 冷启动仲裁、真实 fixture 握手与多客户端压力验证仍待执行。

P2 纯 Python 传输：`_windows_pipe.py` 通过 ctypes 封装 token SID、服务端进程身份、
本机 pipe 名、overlapped 有界读写、超时取消回收与关闭，无 native adapter 依赖。
设置 `ZIPZI_PYTHON2_HOST` 指向已构建的 Windows fixture 后，运行
`python crates/zipzi-python2/tests/test_windows_transport.py -v`，真实握手与 idle timeout
1 项通过。公开 ConnectedBackend、endpoint 冷启动流程和严格静态类型尚待接线验证。

Connected 接线：新增 Stream 协议、平台工厂和 Windows endpoint 分派，纯 Python
传输进入原有 framing/恢复路径。Windows 冷启动由当前用户私有命名 mutex 仲裁，
host 自行取得首 pipe 所有权；客户端释放启动锁不终止 host。Windows transport
测试扩至 2 项并通过，新增冷启动、断开首客户端后仍常驻、第二连接复用同 PID/Runtime。
当前 start=True 的显式 endpoint 要求匹配 runtime directory/profile 环境配置；
并发冷启动、崩溃恢复、安装后公开 API 与跨平台类型门禁仍待完成。

2026-10-28 后续 Windows 原生验证：`test_windows_transport.py -v` 3 项通过。
新增 4 个独立进程通过公开 `zipzi.connect` 同时冷启动，以 fixture PID 日志确认仅一个
有效所有者和同一 Runtime；全部客户端退出后 host 存活，强制终止后新连接得到新 Runtime。
这些测试仍加载源码客户端，不能代替安装纯 wheel 后的验收。

P3 开始：`dev_python.py` 使用共享跨平台文件锁和 venv 路径，扩展按内容哈希发布到
不可变目录，通过原子 `.pth` 切换供新进程使用的版本；保留旧版本，不删除加载中的文件。
统一 `tools/dev.py python` 已开放 Windows。实际 MSVC/CPython 3.14 构建原生 adapter
后，Local Runtime 创建、查询和关闭通过；两个并发 launcher 在仓库外运行，均保持 cwd，
分别精确保留退出码 0/23。版本发布聚焦测试 1 项、统一入口测试 9 项通过；原有 POSIX
launcher 的 8 项跳过不计 Windows 覆盖。`_support.py` 已改用共享锁，其他 Python
测试/分发脚本的 host 产物定位、临时目录和安装流程仍需迁移。

P3 launcher 测试补齐：将 POSIX shebang 工具替身改为按平台生成入口，并统一 PATH/PATHEXT 查找。
`tools/test_tools.py -p test_dev_python.py -v` 在 Windows 为 8 项通过、1 项 Unix inode
专项跳过，覆盖脚本/cwd/argv/stdin、-c/-m、退出码、Python 环境清理、并发准备、
构建失败及源码更新；已纳入双平台 development CI。严格 pyright 为 0 errors。

P3 安装验证推进：`test.py`、`connected.py` 和 `distribution.py` 已改为使用 Cargo JSON
定位 fixture，适配 Windows venv、可执行文件后缀和平台临时目录。开发测试 host 按内容
哈希复制，避免覆盖仍在运行的 Windows executable；构建元数据记录实际 host 路径。
Windows `tools/dev.py test sdk python2 PythonSdk.test_manual_mesh_source_contract_and_existing_large_writes`
通过 Local/Connected 行为及完整正负静态类型门禁；4 项 Python 工具测试、9 项入口测试通过。

安装纯 wheel 的连接验收揭示慢启动缺陷：客户端在 listen 之前连接又超时退出，会让
ConnectNamedPipe 返回 ERROR_NO_DATA，原 listener 退出导致所有权丢失。现已断开并复用
原 pipe 实例，保留所有权；`cargo test --locked -p zipzi-runtime --lib local::windows::`
2 项通过，包含该确定性回归。Windows 测试清理同时等待进程退出，并正确处理已退出 PID。
源码归档验证正在 `target/python2/windows-distribution-20261028` 运行：两份 sdist、归档
生成器校验及 pure wheel 已通过，native wheel/生产 host/安装工作流结果尚待确认。
本轮归档生成后又修复了 listener 与 Windows 保存 durability 断言，需刷新归档后重验最终状态。

修复后 Windows 完整 python2-connected 门禁通过：16 项中 13 项执行通过、3 项 Unix socket 专项跳过。覆盖安装纯 wheel 后的竞争冷启动、慢启动超时复用、启动失败重试、身份/构建检查和请求恢复；Windows 伪协议服务器/权限负例仍需补齐，不能以这些跳过证明对应安全覆盖。

后续 Windows 验证：完整 `test_api.py` 的 96 项 Local/Connected 测试通过；修复测试中将
Named Pipe 当目录的重启逻辑及 `/tmp` 假设。恢复专项 `--recovery` 16 项通过，正负类型
门禁通过。保存恢复断言现在明确要求 Windows `durable=False`，不丢弃完成/缓存/状态验证。
安装纯 wheel 的真实协议不兼容拒绝和启动客户端退出后复用原 host 两项新增测试通过。
Python Named Pipe 的 sendall 使用跨所有分块的总预算；Windows transport 4 项通过。

首轮归档成功构建 CPython 3.14 Windows native wheel 和 Release CPU host，安装后
Connected recipe 在已知的旧 durability 断言处失败；不是完整分发通过。使用该 native wheel
的隔离 Local 环境执行当前完整 installed_recipe 成功（包含 R4/P3B/P4/R5/preview/spatial
和保存重开）。第二轮 `windows-distribution-20261028-r2` 已刷新源归档，继续使用独立的
`target/python2/distribution-cargo` 缓存，最终配对分发结果仍待确认。

P4 文档开始同步：根 README 新增 PowerShell 非图形开发命令，tools README 和 Python SDK README/guide 明确 Named Pipe、CPU fixture 与 P5 桌面边界。基准辅助入口改用当前解释器扩展后缀、Windows host 后缀与系统临时目录；既有计时/统计逻辑未改动。第二轮归档已通过源码生成器检查和 pure wheel 构建；Python tooling 模块回归仍在执行，暂未宣称通过。

后续工具与 CI：Python 工具组 75 项测试结束（74 通过、1 跳过）；基准配对/迁移 6 项与 contract 工具 6 项聚焦复验通过。discovery 7 项及类型门禁通过。development CI 增加双平台 Runtime IPC/协议和 Python behavior/installed/discovery job，尚未远端执行。平台配置/缓存目录 4 项 Windows 测试通过；runtime/editor-core/project 的格式检查通过。安装测试 host 采用临时文件加原子发布，复制失败重试与旧版本保留回归通过，分发工具聚焦共 4 项通过。

第二轮分发 `target/python2/windows-distribution-20261028-r2/results.json` 已记录 passed：
12 个步骤均为退出码 0，含两份 sdist、归档生成器校验、pure/native wheel、Release host、
两个隔离 venv 的安装及完整 Connected/Local recipe。真实 native 扩展版本并存测试也通过：
旧子进程持续加载旧目录，新子进程加载新目录，两边均能查询 Runtime。新增用例已加入 CI。
额外 strict Clippy 检查未通过：9 项告警来自既有大型枚举、参数数量及取模写法；日志为
`target/windows-runtime-clippy.log`，不能据此宣称 SDK Clippy 全绿。

干净 Windows 验证快照 `f83091aa49b1e53938cad8558e346a2a820b1cb3` 已创建，当前开发分支
与工作区未被提交或重置；独立工作树位于 Codex 管理的 windows-verify/zipzi-2d。
该树 git status 为空，已重新初始化子模块、uv sync --locked、通过包域检查和 8 项
Windows source launcher 工具测试（1 项 Unix inode 专项跳过）。CPU、真实 native
发布及安装连接闭环仍在该树执行中。另修复统一入口的 Python --output/--editor-host
相对路径，10 项入口测试通过。

源码身份验收补强：build.rs 与 source_identity.rs 测试共用实际 digest 实现；固定 fixture 的跨平台期望 SHA-256 为 763a744031f58117890a73c612f3e02df911a4ed874b464311af6a78527f71c9。Windows 验证 LF/CRLF、输入顺序、中文相对路径和真实源码变化共 3 项通过；macOS 同一黄金样本仍需 CI 实际执行。此改动发生在验证快照之后，已在主工作区执行聚焦测试。
