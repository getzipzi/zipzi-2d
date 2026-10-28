# Windows 原生开发支持实施方案

状态：待实施。更新日期：2026-10-28。

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
| P0 工具与最小 CI | 待实施 | 待记录 |
| P1 平台与文件语义 | 待实施 | 待记录 |
| P2 Runtime IPC | 待实施 | 待记录 |
| P3 Python 开发与分发 | 待实施 | 待记录 |
| P4 基础设施收口 | 待实施 | 待记录 |
| P5 GPUI/WGPU 与桌面验收 | 待实施 | 待记录 |

完成证据应包含变更范围、实际命令、操作系统/工具链、结果和未覆盖项。源码调查、
macOS 交叉编译或 CI 配置存在都不能单独证明 Windows 运行行为。
