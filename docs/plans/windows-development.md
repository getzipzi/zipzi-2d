# Windows 原生开发支持实施方案

状态：P0–P4 验收收尾；P5 已提前开始，Windows 桌面原型和开发重载已接通，性能与双平台最终验收待完成。更新日期：2026-10-28。

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

原定前置条件：P0–P4 已验收。2026-10-28 用户明确要求直接开始 P5，故提前推进；
P0–P4 未完成的验收继续保留，不视为已通过。复用已有 `WgpuSceneRenderer` 和 macOS WGPU 路径，
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
| P5 GPUI/WGPU 与桌面验收 | 实施中 | 原生窗口、WGPU 像素、Python 共享编辑/保存重开、开发安全重载通过；端到端延迟、资源上限与完整桌面验收未完成 |

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

### 2026-10-28 P5 首批接线

按用户要求提前开始 P5。Metal 渲染器、Apple 开发依赖及原生比较 example 已隔离；
Windows Editor 默认启用 WGPU，模型帧在独立线程完成渲染和回读，再转换成 GPUI
D3D11 所需的 straight BGRA 图片。请求与结果仍按视口合并，保持不可变帧及过期拒绝。
图片由共享所有者保留，最终释放后通过 UI mailbox 回收 GPUI atlas；未显示帧不入 atlas。
网格 overlay 使用现有 GPUI paths/markers，候选预览在生成线程栅格化后合成，最长边
2048 像素（每图最多 16 MiB）；候选放大时会放大该栅格，尚需高 DPI/缩放质量验收。

Editor IPC 已复用 P2 listener，并新增跨平台有界连接入口，用于重复启动和真实 readiness
握手；Windows 客户端验证 Named Pipe 服务端用户。配置和文件对话框接入 P1 目录服务。
Windows icon 来自现有 vector.icns 的 256px PNG；DPI/权限 manifest 沿用锁定的 gpui-pre
依赖自带 PerMonitorV2/asInvoker/common-controls 资源。额外嵌入 manifest 会触发资源 ID 1
重复链接错误，已移除重复资源。开发入口补充许可证/notices 复制。

已执行：Windows `cargo check --locked -p zipzi-editor`；
`cargo build --locked -p zipzi-editor`；
`cargo test --locked -p zipzi-editor-render --features wgpu-backend --test wgpu_readback -- --nocapture`
1 项通过，RTX 5060 / Vulkan，覆盖透明像素、非对齐行宽、视图变换复用、纹理更新和已提交
图像不可变性；`cargo test --locked -p zipzi-runtime --lib local::windows::` 2 项通过；
`tools/test_tools.py -p test_dev_editor.py -v` 最终 6 项通过（含拒绝退出、无回复超时、
确认后等待进程退出）；`test_dev_lifecycle.py` 7 项和 `test_dev_entrypoint.py` 10 项通过。
Windows Editor/renderer 的 `--all-targets` 加 `test-support,wgpu-backend` 检查通过；
颜色转换单测 1 项、UI presentation 调度测试 10 项、worker queue 测试 3 项通过。
CI 新增 Windows/macOS Editor all-target 编译矩阵和 Windows 聚焦逻辑测试，尚未取得远端
运行结果；不能把本机 Windows 结果作为 macOS 回归通过。

原生桌面验收机器：Windows 11 Pro 10.0.26300，AMD Ryzen 5 5600GT，NVIDIA GeForce
RTX 5060；场景渲染 WGPU/Vulkan，窗口图片合成 GPUI/D3D11，Rust dev 未优化构建。
真实 Editor `--no-window` 启动及 readiness 握手通过，`show_window` 唤起后确认：

- 自有合成棋盘模型显示、透明区域、中文名称、网格线/顶点、缩放与裁剪、动画工作区静态
  姿态、弹出菜单遮挡均可见。中文名称来自 SDK，不代表中文 IME 已验收。
- Python 修改名称在窗口中同步；实际 UI Ctrl+Z 撤销后，Python 读到相同 Runtime、原名称、
  history `(0, 1)` 及 `modified=False`。重复启动复用同一 host，而非新建 Runtime。
- 安装版 pure-client 的真实 Editor 验收现已跨平台：并发冷启动、共享 mesh/parameter/
  binding 与 CPU pose、窗口复用、被终止客户端的 grace 回收、保存/重启/重开，共 1 项通过。
  共享传输测试辅助代码后，CPU fixture Connected 回归共 18 项：15 通过、3 项 Unix 专属跳过。
  复现：`uv run --locked python tools/dev.py test sdk python2-connected --editor-host <absolute-exe>`。
- Windows 开发重启新增显式退出握手：拒绝未保存文档、连接中的 Python 客户端、待完成任务、
  活跃对话框/手势/无效输入，失败和超时均保留原进程。原生脏文档拒绝及干净文档退出已验证。
  `tools/dev.py editor reload` 实际发布便携 exe、正常退出旧进程并启动新进程，恢复中文路径
  项目并保持未修改；`editor stop` 正常结束。四份许可证/notices 与仓库源文件逐字节一致。

### P5 回读成本与未完成项

将透明/不透明像素转换改为直接赋值，避免每像素嵌套循环和不必要的除法。实际便携版窗口
执行 12 次编辑/撤销循环，trace 共 25 个回读帧（含冷启动帧），`lost_events=0`：

| 采样项 | 结果 |
| --- | --- |
| 每帧 RGBA/BGRA 数据量 | 6,272,632 bytes（约 5.98 MiB） |
| 冷启动回读 / 转换 | 40.05 / 14.64 ms |
| 24 个暖机帧回读中位数 / 最大值 | 2.994 / 5.352 ms |
| 24 个暖机帧转换中位数 / 最大值 | 15.123 / 15.649 ms |

以上为初版合成小模型、未优化 dev 构建的 worker 阶段计时，**不包含 GPUI 图片上传、屏幕呈现
或输入到显示延迟**，不能作为 60/120 Hz 或复杂模型的达标证据。初版相同尺寸转换约
127–132 ms，第一次优化后暖机阶段合计仍超过 16.7 ms；后续修复与测量见下。
日志位于本地 `target/windows-p5/`（不随源码提交）。

2026-10-28 回读瓶颈修复：主要开销来自未优化的 CPU 逐像素去预乘／通道交换；旧回读
每帧还新建 staging buffer 并逐行去除 padding。现在 Editor renderer 使用 compute shader
按原 CPU 整数舍入与饱和规则直接生成紧凑 straight BGRA storage buffer，再复制到可复用的
staging buffer。CPU 只复制最终字节。主画布与候选预览共用该路径，返回图像仍各自独立，
macOS Metal／WGPU texture handoff 未改。每个 renderer 最多缓存一对转换／回读缓冲区，
字节数变化时替换；每个缓冲区受 device storage binding／buffer 上限约束（当前默认
storage binding 128 MiB），不能据此声称预览总内存有硬上限。

新增 `wgpu_readback_benchmark` 对照 harness：同机 RTX 5060／Vulkan，1061×1478，
6,272,632 bytes，混合透明度纹理三角形，20 帧预热后采样 60 帧。每帧平移视图，旧／新
路径读取同一提交图像，交替先后顺序并逐字节比较结果。下表仅为桥接阶段，即旧路径
「回读＋CPU 转换」与新路径「GPU 转换＋回读＋CPU 复制」，均排除 CPU 场景提交与 GPUI：

| 构建 | 旧路径中位数 / P95 / 最大值 | 新路径中位数 / P95 / 最大值 |
| --- | --- | --- |
| dev | 19.632 / 20.342 / 22.749 ms | 2.321 / 3.160 / 6.591 ms |
| release | 6.058 / 6.471 / 7.147 ms | 2.143 / 2.723 / 6.410 ms |

dev 旧回读／CPU 转换中位数为 3.287／16.362 ms；release 为 3.093／2.936 ms，说明
优化构建能减轻但未消除桥接开销。GPU 转换后不再单独记录 CPU `conversion_ns`；trace
`readback_ns` 包含 GPU 转换及最终 CPU 复制，并以 `gpu_conversion=1` 标记。此处回读包含
排在前面的场景命令完成等待，**不是独立 GPU 时间戳，也不是输入到屏幕延迟或 FPS**。

独立 Windows dev Editor 原生窗口也已验证：自有棋盘模型显示与透明合成正常，60 次
编辑／撤销后名称、未修改状态恢复。1061×1478 的 60 个暖机帧中位数／P95／最大值
2.984／5.535／9.926 ms，冷启动首次转换 47.291 ms（包含首次 shader/pipeline 建立）。
最大化后 1654×1865、12,338,840 bytes 的 60 个暖机帧为 4.447／6.374／6.467 ms；
该尺寸首次为 5.387 ms。trace `lost_events=0`，测试实例安全退出。隐藏启动探针未产生
画布请求，不计入这些结果；修正启动方式后才采集实际窗口帧。原生 trace 同样只计 worker
桥接阶段，不涵盖 GPUI 上传／屏幕呈现。GPU 互操作及端到端测量继续保留为后续路线。

聚焦验证：`zipzi-editor-render --features wgpu-backend --test wgpu_readback` 实际通过
2 项 GPU 测试，覆盖全部颜色通道／alpha 组合、RGBA/BGRA、零 alpha、越 alpha 饱和、
非对齐宽度、缓冲区复用／resize、sRGB 拒绝及多视口旧帧不可变性；Editor／renderer
`--all-targets` 加 `test-support,wgpu-backend` 检查通过，Editor dev／release build 与 UI performance
10 项测试通过。CI 移除已不存在的 CPU 转换测试过滤器，GPU 转换由本机真实 GPU 门禁验证。
相关修改文件的格式检查及 `git diff --check` 通过；工作区全量 `cargo fmt --all --check`
仍报告其他既有文件格式差异，未为本次修复改写这些文件。原始 CSV、汇总 JSON 与原生 trace
保留在本地 `target/windows-p5/readback-*`、`target/windows-p5/perf-fix/`，不随源码提交。

当前单 worker 串行完成 GPU/回读，请求与结果按视口合并；图片最终所有者释放后从 atlas
回收，网格候选最长边 2048px / 每图最多 16 MiB。**这些局部限制尚不构成预览总内存硬上限**；
需要补齐冻结文档/多窗口图片预算和长时间动画资源增长验收。

P5 保持实施中。尚需完成：双视口真实窗口同时显示（已有不可变图像 GPU 测试和调度逻辑
测试不替代此项）、动画播放、候选网格高 DPI 质量、GUI 导入/文件对话框/拖放、中文 IME、
多显示器缩放、完整关闭最后窗口/连接客户端常驻交互、端到端帧延迟与内存上限，以及 macOS
Metal/WGPU 原生回归。正式安装器和签名不属于本阶段。

### P5 GPUI 性能基线（2026-10-28）

本轮先测量，不调整渲染、缓存或调度策略。GPUI 公共窗口层补充 opt-in trace：
root build、prepaint/layout、paint、图片 atlas lookup/upload 和 platform draw CPU 边界；
Windows 预览以 `compose_image` 将不可变图片关联到 UI 帧。未启用 trace 时不读取新增
atlas 时钟。`vendor/gpui-diagnostics.patch` 保存该补丁，可反向校验。

使用同机 Windows／RTX 5060，GPUI D3D11 与 WGPU Vulkan；WMI 报告主输出 3840×2160、
60 Hz，另存在 GameViewer 虚拟适配器。原生窗口为默认 1100×740 logical，画布图像
1061×1478、6,272,632 bytes。对自有棋盘项目与本地 Mao model3，各执行 Dev／Release
两轮，第二轮交换构建顺序；每轮平移、缩放、透明度三种 workload 各预热 30 次、测量
180 次。构建结束后复制并固定二进制，测量时没有 Cargo/rustc。每份 trace 完整结束、
`lost_events=0`。窗口可见／active 由 driver 每次输入检查。Mao Dev 第一轮额外截图确认
模型确实绘制，不把该轮截图期间的尾部峰值解释为纯应用成本；下面主表采用无截图的第二轮，
第一轮及所有峰值仍保留在原始数据中。

以下为第二轮**平移**的每帧 CPU 中位数／P95，单位 ms：

| 模型／构建 | UI draw | root build | prepaint/layout | paint（含 atlas） | atlas miss | platform draw |
| --- | --- | --- | --- | --- | --- | --- |
| 棋盘 Dev | 6.85 / 7.97 | 3.31 / 3.71 | 1.22 / 1.59 | 1.80 / 2.18 | 0.57 / 0.66 | 0.16 / 0.18 |
| 棋盘 Release | 4.25 / 4.52 | 1.59 / 1.77 | 0.80 / 0.89 | 1.44 / 1.57 | 0.59 / 0.62 | 0.13 / 0.15 |
| Mao Dev | 10.00 / 10.83 | 4.78 / 5.28 | 1.98 / 2.18 | 2.48 / 2.82 | 0.60 / 0.68 | 0.17 / 0.20 |
| Mao Release | 6.25 / 7.14 | 2.32 / 2.72 | 1.26 / 1.51 | 1.97 / 2.37 | 0.62 / 0.74 | 0.16 / 0.20 |

UI draw 包含 Editor／Kit element construction、GPUI prepaint/layout、paint 和帧收尾，
不能把 root build 全部归因于 GPUI 核心；其中也有 Editor 的 Render 实现和 Kit 控件构造。
这些 scope 嵌套，不能相加独立 P95。Mao Dev 第二轮平移／缩放 UI 峰值为 24.04／24.58 ms，
说明中位数外仍存在超过 16.7 ms 的帧。新图像 atlas API 调用通常约 0.6 ms，CPU platform
调用通常约 0.15 ms；这两项在当前 CPU 采样中不是最大项，但未测量其异步 GPU 执行时间。

另增加 `ZIPZI_BENCH_UI_ONLY=1` 控制，只 notify 当前 canvas，保持原有图片，**不请求新
模型渲染／回读**；每模型／构建采样 180 次。四份 trace 实测 request=0、上传 bytes=0，
每份有 170–178 个 UI draw，排除“没有实际重绘”的假控制。UI draw 中位数／P95：

| 控制组 | Dev | Release |
| --- | --- | --- |
| 棋盘，复用图片 | 5.43 / 6.45 ms | 3.30 / 3.62 ms |
| Mao，复用图片 | 8.85 / 9.48 ms | 5.03 / 5.53 ms |

控制组证明 UI 重绘开销可独立于 WGPU 回读／新图片上传复现。这是保留完整 Editor/Kit
面板的 canvas 重绘基线，不是空 GPUI 程序。后续优先细分 UI subtree 构建／布局、缓存失效
范围，再核查 Windows 帧调度；现有数据不能直接证明某个子树必然多余重建。

第二轮 Mao 的新图像 **CPU platform-return 事件**速率：Dev 平移 50.82、缩放 48.88、
透明度 38.63 次／秒；Release 分别 59.83、60.01、58.95。各场景 180 个输入中，Dev
22／26／0 个没有关联到新图片提交，Release 为 11／11／12；这些未关联输入保留在报告，
不能只展示成功样本。透明度输入到首张对应新图片的 CPU platform-return 中位数／P95：
Dev 33.01／39.52 ms，Release 15.75／25.78 ms。这些都是合成 GPUI dispatch 到 CPU
调用返回的测量，**不是 OS 输入到屏幕延迟，也不是实际屏幕 FPS**。

driver 目标输入频率为 120 Hz，但大多数输入间隔实际约 16 ms；Mao Dev 透明度为
28.68 ms 中位数，即使 UI-only Release 控制的间隔也约 15.84–15.94 ms。后续测试必须先
区分 driver timer／Windows 消息循环节奏与渲染吞吐，不能用目标 120 Hz 宣称达标／掉帧。
该 CPU 基线没有 D3D11 GPU timestamp、DWM 队列或 scanout 证据，不能据此排除
GPU／桌面合成瓶颈；后续系统呈现测量见下一节。

原始 12 次运行、复制二进制及 SHA-256、trace、完整逐项分位数、显示信息与脚本位于
本地 `target/windows-gpui/`；`summary.json` 为八次实际交互，`control-summary.json`
为四次静态图片控制。通用分析器为 `tools/benchmarks/windows-gpui-trace-report.py`，
拒绝未完成、丢事件、缺 workload 和失败运行，保留同一 scene 的重复 platform 调用。
3 项工具回归、10 项 Editor UI performance 回归及 Dev／Release 构建通过。
复现环境及计时边界见 `tools/benchmarks/README.md` 的 Windows GPUI 小节。

### 现有 UI benchmark 的 Windows 呈现帧率（2026-10-28）

按用户要求，使用现有 `tools/benchmarks/window-benchmark.sh` 对应的生产驱动
`crates/zipzi-editor/src/benchmark.rs` 运行 Release／Mao：pan、zoom、opacity 各
30 次预热＋300 次正式输入，目标 120 Hz。只以 Windows subprocess 替换脚本中 macOS
专用的启动／环境采集，不改负载。第一轮应用正常完成、trace 丢失 0；原 Metal 报告器
遇到 Windows CPU 图片没有 `published.texture` 时崩溃。已做最小兼容处理，输出
`painted_unconfirmed`，保留缺呈现时间戳的 INCOMPLETE 判定及空 FPS，而非伪造成功。
该报告器 16 项聚焦回归通过。

随后复跑相同完整轮次，并由本机已安装的 PresentMon 1.8.0（UE 安装附带的 console）
采集 DXGI／DWM 呈现事件。普通权限返回 access denied；用户明确允许 UAC 并手动确认后，
只提权运行采集器，Editor 仍以普通用户运行。Windows PowerShell 的脚本策略阻止首次启动，
改用现有 PowerShell 7，不修改执行策略。另一次没有启动目标的空采集未计入结果。

benchmark trace metadata 新增 opt-in QPC 校准：以两次 trace-clock 读数夹住
QueryPerformanceCounter，同时记录 frequency；本次 bracket 为 100 ns。PresentMon
使用 `-qpc_time`，由此精确映射到已有 trace 时间域，不估计应用启动耗时。采集得到同一
测试 PID、同一 swapchain、`Composed: Flip` 的 1,035 个 DXGI Present，全部在对应的
GPUI `platform_draw_begin/end` 区间内匹配；应用 trace 共 1,036 次 platform call，
其中一次没有对应的 PresentMon row，未补造该帧。PresentMon 正常退出，无丢事件警告；
应用正常完成 900 次正式输入，trace `lost_events=0`。

按原 UI 报告的时间窗口口径，排除启动、预热及阶段间等待，统计正式输入首／末 tick
之间的**系统报告显示时间戳**。这次输出为 3840×2160／60 Hz：

| 负载 | 窗口显示 FPS | 新预览首次显示 FPS | 显示帧间隔 P95 / 最大值 | 输入到新预览显示 P50 / P95 |
| --- | --- | --- | --- | --- |
| 平移 | 60.00 | 59.57 | 16.74 / 16.79 ms | 41.72 / 50.23 ms |
| 缩放 | 60.00 | 59.78 | 16.73 / 16.79 ms | 41.49 / 50.66 ms |
| 透明度更新 | 60.00 | 58.54 | 16.72 / 17.18 ms | 41.90 / 51.76 ms |

三个正式输入时间窗口分别有 282／281／288 次确认显示，区间内的 Present 丢弃均为 0。
整个目标进程采样包含 1,033 次显示、2 次丢弃；后两者在上述正式输入区间之外，仍保留
在原始 CSV。每项 300 个正式请求中，281／281／282 个被关联到首次显示，剩余
19／19／18 个保留为没有确认显示；不能把 300 个请求全部解释为独立显示帧。

窗口 FPS 使用所有已显示窗口帧；新预览 FPS 对不可变图片请求去重，仅计首次显示。
图片由 `compose_image` 关联到单视口 CPU 场景，再关联到该场景的实际 DXGI Present；
由 `QPCTime + msUntilDisplayed` 得到系统显示时间，`Dropped=1` 不参与显示 FPS。
帧率使用 `(N-1)/(last-first)`，沿用现有 UI 报告，不计算瞬时 FPS 的平均数。
该轮证明 Release 在当前 60 Hz 环境下窗口呈现约 60 FPS，但应用输入到新图像显示仍约
42 ms 中位数／50–52 ms P95。自动输入从 GPUI dispatch 开始；这不是硬件输入到光子延迟，
也不是 Dev 构建或 120 Hz 显示器的验收。没有新增 GPU 执行时长的结论。

本地原始数据及固定二进制位于 `target/ui-benchmark-windows-20261028/`。
有效 ETW 轮次在 `presentmon-measured/`：`presents.csv`、`trace.jsonl`、`process.json`
（PID／二进制 SHA-256）、`presentmon-joined.csv`、`display-requests.csv` 和
`display-summary.json`；`analyze.py` 保留校准、逐行匹配及分组计算。
原 Metal-only `summary.json` 仍明确 INCOMPLETE；补充的 `display-summary.json` 才是
上述 Windows 系统呈现结果，不以修改原报告的 GPU／呈现字段填补缺失覆盖。

### 全场景定位与首批修复（2026-10-28）

使用仓库现有 native UI driver 完成 68 个场景：Properties 45 项（Mesh 33、
Physics 12）、Parameter 8 项、Natural／Dense Timeline 各 7 项，以及 canvas
reference。全部运行的功能断言通过，trace 完整且无丢事件。新增的 8 项 Timeline
scroll／zoom／seek／key 探针存在迟到后零间隔连续投递的问题，其帧率排除；用户要求
不再重跑。其余 60 项保留原始结果，不能把功能断言通过解释为性能达标。
原始数据在 `target/ui-all-20261028/`，其中 3 项 Parameter detail 是独立的插桩诊断，
不与普通构建混为同一基准。Windows 仍未运行 macOS 专属的完整 Timeline 像素／持久化验收。

Release、Mao、3840×2160／60 Hz 的现有 PresentMon 结果中，Properties 透明度拖动
40.8 FPS、色相／SV 拖动 37.2／36.4 FPS，Parameter 未选中／全选 59.5／51.4 FPS，
Natural／Dense Timeline 数值拖动 60.0／23.9 FPS。以下针对性 CPU 诊断用于定位和
验证修复，**没有重新采集系统呈现 FPS**。

1. Dense 数值拖动的主要耗时在 UI 输入处理内的 `core.validate_motion`，不是增量投影
   （约 0.1 ms）或预览读回。完整校验原先对每个关键帧调用 `contains_base_identity`，
   反复扫描 Physics、Pose、motion 注册等集合。改为一次校验建立一份借用的身份集合，
   不引入跨文档修订缓存，不跳过曲线时间、数值、身份冲突和跨 clip 检查。
2. Properties Mesh 外观候选每次调用 `EditorView::redraw`，使整窗失效。Modeling 下
   成功的 Mesh 候选改用已有 `complete_preview_edit`，由新预览到达驱动画布刷新；
   inspector 自行通知，提交、取消和错误仍走现有路径。其余工作区／资源编辑保留原刷新。
3. Parameter 的全选对照确认 Windows 后备 overlay 绘制有明显成本。插桩运行中，
   保留全部选择而关闭绘制后，UI draw P95 从 14.63 降到 9.47 ms、窗口 FPS 从
   51.22 到 58.26；无选择为 7.96 ms、58.92 FPS。Windows 使用 GPUI 路径，
   macOS 使用专用 Metal overlay；当前只定位，尚未实现 Windows 的对应优化。

固定 Release 插桩二进制、相同输入驱动的局部 CPU 对比（单位 ms）：

| 测量 | 修改前 | 修改后 |
| --- | --- | --- |
| Dense Timeline 完整校验平均耗时 | 44.06 | 8.02 |
| Dense Timeline 输入处理 P95 | 48.82 | 10.16 |
| Properties 透明度 UI draw P95 | 18.77 | 14.74 |
| Properties 色相 UI draw P95 | 21.27 | 20.16 |
| Properties SV UI draw P95 | 20.52 | 11.80 |

Timeline 各 32 次输入，Properties 各 3 次预热＋30 次正式输入；只通过原 driver
分发事件，不强制逐输入绘帧。上表使用无 Cargo／rustc／link 干扰的短采样，
早期与编译重叠的探索数据另行保留，不作为主对比。原始 trace、脚本、完整分位数及
固定二进制 SHA-256 在 `target/ui-diagnosis-20261028/`；Timeline 主对比为
`before/` 与 `after-clean/`，Properties 为 `properties-before-clean/` 与
`properties-after/`，汇总为 `cpu-summary.json`。这些是线程 wall time，含调度影响，
不能换算为修复后的实际 FPS。

34 项聚焦回归通过：core 身份／跨 clip 校验 2 项、SDK motion editing 20 项、
Mesh Properties 9 项、外观预览保留整窗 1 项、draw-order 提交／取消 1 项、
benchmark 迟到调度 1 项。新增工作量回归同时要求每次输入替换预览且历史不变，
避免“不更新界面”造成假性能提升。相关 UI 测试辅助的全选快捷键已按平台选择
macOS `cmd-a` 或 Windows `ctrl-a`。

首批修复只缓解已确认的两处开销。色相拖动仍约 20 ms UI draw P95，Parameter
全选 overlay 仍需优化；Dense 校验也仍约 8 ms。P5 的 UI 性能不能据此整体结项。

### Parameter 后备 overlay 修复（2026-10-28）

继续定位全选 Parameter 拖动：Mao 的 150 个选中 Mesh 会产生 5,159 个顶点标记、
12,202 条网格边及 2,825 条边界线。耗时主要发生在显示这些变形结果的 GPUI overlay。
本次不修改 Runtime 参数求值、模型渲染或 wgpu readback。

- 标记原先逐个进入 GPUI bounds tree。连续的纯 Quad 标记改为使用 paint layer，
  保留先普通、后选中／悬停的顺序，并把混合 Path 的旋转中心十字线留在层外。
  GPUI 本来就能合并相邻 Quad；这里减少的是 CPU 遮挡排序，不是把数千次 GPU draw
  合并成一次。实际 Scene 回归确认重叠 Quad 的顺序和图元数量保留。
- 独立直线原先经过通用 `PathBuilder`／Lyon 描边细分及 `KeptPaths` 缓存。
  参数每次变形会产生新的大路径；按形状数量限制的通用缓存会保留大量旧姿态几何。
  改为直接生成每段直线的两个实心三角形，保留 butt cap、线宽、三角形方向、
  MSAA 复制边界和原有 4,096 条边的分块，绕开通用形状缓存。曲线和填充仍使用原路径。
- 修正 Windows `vertices` 诊断仍绘制 mesh edges 的开关问题。先前数据保留，
  但不能将旧 Windows `vertices` 结果解释为纯顶点消融。

使用同一个固定 Release 插桩二进制，通过两个 opt-in 开关选择旧／新路径，继续运行
现有生产 Parameter window driver。每进程 30 次预热＋180 次正式输入、目标 120 Hz；
两轮交换新旧顺序。下表关闭详细计时，只保留已有 frame trace：

| 轮次 | UI draw P50：旧 → 新 | UI draw P95：旧 → 新 | 进程峰值工作集：旧 → 新 |
| --- | --- | --- | --- |
| 1 | 11.31 → 9.71 ms | 15.34 → 11.72 ms | 1,045 → 561 MiB |
| 2（交换顺序） | 11.52 → 9.75 ms | 15.29 → 11.61 ms | 1,028 → 579 MiB |

进程峰值 commit 分别从 1,692／1,694 MiB 降到 1,168／1,182 MiB。
工作集／commit 是整个测试进程的启动到退出峰值，不是 GPU 内存或单一缓存的独立计量。
原始 `GetProcessMemoryInfo` 样本每 250 ms 保留一次。

单独的详细计时对照（每项 177 次实际绘制）中，各 scope 的中位数／P95，单位 ms：

| Scope | 旧路径 | 新路径 |
| --- | --- | --- |
| `overlay.gpui_mesh_edges` | 2.46 / 2.91 | 0.76 / 0.88 |
| `overlay.gpui_lines`（语义／控制线） | 0.76 / 1.06 | 0.17 / 0.23 |
| `overlay.gpui_markers` | 1.59 / 1.77 | 0.59 / 0.63 |

详细对照的 UI draw P95 为 15.50 → 11.40 ms；嵌套 scope 的独立分位数不可相加。
前期仅分层标记的 10 个探索进程也保留：标记函数变快，但详细轮整帧 P95 曾从
15.20 到 17.00 ms，未将这轮解释为整体改善；新增 mesh-edge scope 后才继续修复直线。
修正后的纯顶点对照确认没有 mesh-edge scope；仅边对照没有 marker scope。

10 个探索进程与 6 个主对照进程均正常完成，trace 无丢事件。主对照每项均验证实际
变形的 Mesh 数为 93、选择数 150、顶点／边数量一致；输入不是无效操作。
主对照的编译进程前后快照为空，采样时没有运行构建。10 项 Editor 聚焦回归和
3 项工具归因回归通过：包含新旧描边在分数原点／线宽下的覆盖与方向比较、退化输入、
完整保留数千标记、重叠排序，以及拖动预览／越界拖动／失焦取消。
macOS 原有 native overlay 像素验收已接入新路径，本机 Windows 没有执行该 Metal 测试。

可复现控制为 `ZIPZI_BENCH_UNLAYERED_MARKERS=1` 与
`ZIPZI_BENCH_TESSELLATED_EDGES=1`；两者同时开启为旧路径，默认均关闭。
Python launcher 对应 `--unlayered-markers --tessellated-edges`，其启动包装仍限 macOS；
Windows 本次仅用 subprocess 包装相同生产驱动，CPU 归因复用现有报告器 `diagnose`。
本地 `target/parameter-path-20261028/` 保留探索数据，`direct-edges/` 为主对照，
含固定二进制／SHA-256、原始 trace、audit、逐项分位数、内存样本和运行脚本。
本轮没有采集新的 PresentMon，以上改善只确认 CPU 时间和进程内存，不宣称新的显示 FPS。

## P6：Windows 全 GPU 预览第一版（2026-10-28）

开始前将当时工作区保存为 `7e42c511`。主预览及网格生成候选预览现改为：

```text
wgpu / D3D12 普通输出 → 一次 GPU CopyTextureRegion → 共享 RGBA8 纹理
                                                   ↓ ready fence
                                       GPUI / D3D11 surface 合成
                                                   ↓ consumed fence
                                       无引用且 GPU 完成后复用
```

- 按 GPUI adapter LUID 选择 D3D12 adapter，不改变 SDK 默认 adapter 选择。
- 共享纹理为独立 committed resource。wgpu 跟踪源纹理的 COPY_SRC 转换；桥接层
  单独管理共享目标的 COMMON/COPY_DEST 状态。普通提交不等待 GPU 完成，也不映射像素。
- GPUI core 新增 DirectX surface 和设备访问接口；本地纳入 crates.io 0.1.7
  `gpui-fast-windows`。常规与 Fast 路径均采样共享 SRV，并使用预乘 source-over、
  content mask、opacity。Fast 绘制后使状态缓存失效，后续文字/图元可正确恢复绑定。
- 独立 ready/consumed fence；每次实际绘制（包括 retained redraw）都更新消费时间线。
  槽位复用同时检查 Arc 独占及双向完成，不按帧序号直接覆盖。
- 每个 presenter 上限 8 个共享槽位 / 256 MiB；该预算不含模型渲染附件与尚未退休的
  普通输出纹理。主预览在池满时合并请求并重试；候选预览耗尽预算时返回明确错误。
- 尺寸变更回收可复用的旧尺寸资源。设备身份变化重建预览连接，旧设备 surface 被跳过。
  不支持共享资源/fence 的设备返回预览错误；未增加隐式回读 fallback。
- 关闭时仍在飞行的原生命令转到后台退休线程；保留 wgpu Device/Queue，确保原生对象
  先于后端/动态库释放。真实模型测试曾复现相反释放顺序的退出崩溃，现已加入回归。
- 旧 `WgpuImageReadback` 保留用于导出、测试和基线测量；Windows 正常展示不调用它。
  trace 新增 `shared_texture_submitted` / `paint_shared_surface`，二者都不是屏幕完成时间。

### 本机验证

RTX 5060 / Windows，以下检查通过：

- Editor + editor-render all-targets check，启用 WGPU/test-support。
- `windows_shared_texture`：3 项 GPU 集成测试。覆盖半透明颜色、元素 opacity、裁剪、
  后续 UI 图元、重复绘制、100 次尺寸切换、旧帧不可变、池耗尽/复用、实际模型/UV 方向、
  生产者销毁后继续合成，以及人为阻塞 GPU 时关闭 presenter 不阻塞调用线程。
  最终测试目标允许回读用于像素断言，生产交接路径没有像素回读。
- `headless_tests::ui_performance`：11 项通过；两个帧追踪工具测试文件共 22 项通过。
- Release Editor 构建通过，包含 FXC 预编译 surface shaders。
- Cargo domain 完整性检查及 vendor patch 反向应用检查通过。

### 原生窗口 Release 对照

使用此前保留的旧 Release 二进制（SHA-256 `d2842d8132dd651ca744d846322d93ae58984d5753ef7a9d59d13373c08370e5`）
与本次新二进制（`51b0c3160344c052f5402dabfbef1d8794d08c7cc58fa9b8359670a88e20edfa`）。
同一 Mao 模型、全选/全部 overlay、30 次预热 + 180 次正式参数输入、120 Hz 目标；
按旧→新→新→旧顺序运行，采样阶段未运行编译。每轮实际变化的 mesh 数均为 93；
四个进程正常完成、trace 无丢事件、无 render_error。

| 轮次 | UI draw P50 / P95（ms） | 输入到首次图片/纹理平台返回 P50 / P95（ms） | 全程预览回读次数 |
| --- | --- | --- | --- |
| 旧 1 | 9.79 / 11.87 | 30.93 / 38.26 | 208 |
| 新 1 | 9.05 / 10.64 | 30.58 / 36.68 | 0 |
| 新 2 | 9.61 / 11.10 | 30.35 / 43.14 | 0 |
| 旧 2 | 9.59 / 11.72 | 30.70 / 38.36 | 208 |

新版本两轮各提交 208 张共享纹理；测量窗口内图片 atlas 上传字节均为 0。
旧版本测量窗口的 atlas 上传约为 1047 / 1035 MiB，worker 回读 P50 为 1.71 / 1.70 ms。
比较同时包含 Vulkan→D3D12 后端切换，不能把所有差异单独归因于共享纹理。
这里确认移除回读/重新上传，UI draw P95 在两轮均降低；输入到平台返回的第二轮新版本
尾部更慢，不宣称端到端显示延迟或 FPS 改善。新版本 published 表示异步提交，旧版本
published 在回读完成之后；两种 publish-to-paint 指标的起点语义不同。

本地 `target/windows-shared-texture-20261028/` 保存固定新旧二进制、运行脚本、原始 trace、
模型变化 audit 和逐轮 JSON 结果。正常路径仍有一次 GPU 复制。直接渲染共享目标、
最新已完成帧选择、更细的 GPU 时间戳/PresentMon、多厂商/混合显卡及强制设备丢失验收，
留给后续阶段；本轮不将这些项目列为已完成。

### 用户要求的再次 benchmark（2026-10-28）

复用上述固定二进制（当前 Release SHA-256 与 `51b0c316…20edfa` 一致），按
旧→新→新→旧重跑 Parameter 全选/全部 overlay。每轮 30 次预热＋180 次正式输入，
目标 120 Hz；未运行编译。四轮均正常完成、零丢事件、无 render_error，实际变化的
Mesh 均为 93。原始记录位于 `target/windows-shared-texture-20261028/rerun-1/`。

| 轮次 | UI draw P50 / P95（ms） | 输入到首张对应纹理平台返回 P50 / P95（ms） | 回读次数 | 测量区间 atlas 上传 |
| --- | --- | --- | --- | --- |
| 旧 1 | 10.48 / 13.16 | 31.34 / 43.15 | 208 | 1059 MiB |
| 新 1 | 9.13 / 11.36 | 30.21 / 39.74 | 0 | 0 |
| 新 2 | 9.02 / 10.81 | 30.88 / 39.38 | 0 | 0 |
| 旧 2 | 9.60 / 11.63 | 30.90 / 39.72 | 208 | 1053 MiB |

每项 180 次正式输入中，未关联到对应新纹理平台返回的输入依次为 4、11、6、4；
保留这些未确认输入，不将有时间戳的子集解释为所有输入。新版本各提交 208 张共享纹理。
结果再次确认消除预览回读／atlas 上传及较低的 UI draw P95；输入到平台返回中位数
仍约 30–31 ms，第二轮尾部差距很小。没有新增 PresentMon，不能当作显示 FPS 或
屏幕端到端延迟结果，后端切换仍是对照中的共同变量。
