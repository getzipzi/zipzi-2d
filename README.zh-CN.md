# Zipzi 2D

[English](README.md) · 简体中文 · [日本語](README.ja.md)

**与 AI 一起创作的开源 2D 建模与动画工具。**

你负责创意，AI agent 协助建模。Zipzi 将可视化桌面编辑器与 Python API 连接起来，让你和 agent 查看、编辑同一个模型，共享撤销历史，接着彼此的工作继续创作。

Zipzi 2D 是 Live2D Cubism Editor 的开源替代选择，使用 Rust 和 GPUI 构建。名字来自粤语「摺紙」（*zip3 zi2*）。

[**下载 Alpha**](https://github.com/getzipzi/zipzi-2d/releases) · [Python SDK](crates/zipzi-python2/README.md) · [反馈问题](https://github.com/getzipzi/zipzi-2d/issues) · [参与贡献](#参与贡献)

![Zipzi 2D 编辑器的建模工作区](docs/editor-screenshot.png)

> **早期开发阶段：**目前尚无稳定版本，API 和文件格式可能变更。编辑器支持 macOS；Windows 版本仍是开发原型，桌面功能验证正在进行中。

## 你的编辑器，你的 AI agent，同一个模型。

向 AI 编程 agent 描述需求。它通过 Python SDK 检查模型、查看渲染图，并进行可在编辑器中检查和撤销的修改。你和 agent 操作同一份文档，共享撤销历史。

使用你自己的、能够运行 Python 的 agent，并向它提供 [SDK 指南](crates/zipzi-python2/client/python/zipzi/docs/guide.md)。

### 第一段代码：让 AI 为每个网格起一个有意义的名字

**你描述任务，AI agent 编写并运行代码。**下面的示例由 AI 编程 agent 生成，并已连接运行中的编辑器验证。它们展示 agent 如何通过 Python SDK 工作，你不需要手写这些脚本，只需说明需求，在编辑器中检查结果。

告诉 agent：**「看看每个网格，给它起个有意义的名字。」**

安装 [Python 客户端](crates/zipzi-python2/README.md#installation)（需要 CPython 3.14），启动支持 GPU 观察功能的兼容编辑器，并打开一个模型。能够看图的编程 agent 可以完成以下两步。

**1. Agent 观察每个网格。**它生成的脚本为每个网格导出一张图片，在带纹理的模型上高亮该网格的线框，同时生成 JSON 命名清单。`observe` 提供图像依据，agent 负责理解语义。

```python
import json
from pathlib import Path
from uuid import uuid4

import zipzi
from zipzi import inspection, observation, spatial

output = Path("mesh-review-" + uuid4().hex[:8]).resolve()
output.mkdir()

with zipzi.connect() as runtime:
    sessions = runtime.sessions.list()
    if len(sessions) != 1:
        raise RuntimeError("Open exactly one model for this example.")
    session = runtime.sessions.attach(sessions[0].id)

    # 遍历所有分页：一次 list() 调用可能无法返回全部网格。
    meshes, cursor, source = [], None, None
    while True:
        page = session.meshes.list(cursor=cursor)
        if source is None:
            source = page.source
        if page.source != source:
            raise RuntimeError("Model changed; restart the inspection.")
        meshes.extend(page.items)
        cursor = page.next_cursor
        if cursor is None:
            break

    rows = []
    for index, mesh in enumerate(meshes):
        # 用当前网格的边界确定取景范围，包括隐藏的几何。
        # {} 使用默认参数值求值，而非编辑器中的当前姿态。
        query = spatial.Bounds(
            targets=(spatial.MeshTarget(value=mesh.id),), include_hidden=True,
        )
        bounds = session.modeling.spatial(
            ({},), (query,), expected=source,
        ).wait().value.frames[0].results[0].value.model_bounds
        row = dict(id=mesh.id, old_name=mesh.name, new_name=mesh.name, image=None)
        if bounds is not None:
            x0, y0, x1, y1 = bounds
            margin = max(x1 - x0, y1 - y0, 1.0) * 0.15
            view = observation.View(
                width=768, height=768,
                roi=(x0 - margin, y0 - margin, x1 + margin, y1 + margin),
                topology=observation.Overlay(
                    targets=(observation.MeshTarget(value=mesh.id),),
                    wireframe=True, include_hidden=True,
                ),
            )
            # 所有观察都对应清单的同一数据版本。如果模型发生变化，
            # SDK 会拒绝过期读取；应重新运行，避免混用不同版本。
            images = session.modeling.observe(({},), view, expected=source).wait()
            report = inspection.save(images, output / f"mesh-{index:04d}")
            row["image"] = str(report.images[0])
        rows.append(row)

    # SessionId 是类型化句柄；保存字段，供第二段脚本重建。
    worksheet = output / "names.json"
    worksheet.write_text(json.dumps(
        dict(session_id=dict(runtime=session.id.runtime, serial=session.id.serial),
             meshes=rows), indent=2, ensure_ascii=False,
    ), encoding="utf-8")
    print(worksheet)
```

让 agent **逐张打开导出的 PNG**，结合周围画面识别高亮网格，只修改 `names.json` 中的 `new_name`，保留 ID 和 `old_name`。叠加层仅选择要显示的诊断几何，并不会隔离对应的图层：周围纹理仍然可见，隐藏或被遮挡的内容可能无法辨认。对于不确定或没有图片的条目，保留原名，或先观察其他姿态再命名。

**2. Agent 一次性应用语义名称。**你检查建议名称后，agent 编写并运行下面的脚本，传入 `names.json` 的路径。你无需复制代码或执行终端命令。观察与应用之间，请保持同一个模型打开且不作修改。

```python
import json
import sys
from pathlib import Path

import zipzi

review = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
rows = review["meshes"]
if len({row["id"] for row in rows}) != len(rows):
    raise ValueError("Duplicate mesh IDs in the worksheet.")
if any(not isinstance(row["new_name"], str) or not row["new_name"].strip()
       for row in rows):
    raise ValueError("Every mesh needs a nonempty name.")

with zipzi.connect() as runtime:
    # 重建类型化句柄；编辑器重启后，需要重新观察。
    session = runtime.sessions.attach(zipzi.SessionId(**review["session_id"]))
    # 全部重命名一起提交，形成一次撤销操作。网格不存在或原名改变时，
    # 整个编辑会中止，不会留下只改了一部分名称的模型。
    with session.edit("AI: semantic mesh names") as edit:
        for row in rows:
            current = edit.meshes.get(row["id"]).value
            if current.name != row["old_name"]:
                raise RuntimeError("Mesh changed; inspect the model again.")
            if row["new_name"] != row["old_name"]:
                edit.meshes.rename(
                    row["id"], row["new_name"], expected_name=row["old_name"],
                )
    print("Review the names in the editor. Undo to revert, or save to keep them.")
```

脚本不会自动保存项目。名称检查可以发现网格被重命名或删除，但无法检测所有几何或纹理变化；模型有变化时，应重新观察。更多工作流见[能力指南](crates/zipzi-python2/CAPABILITIES.md)。

## 也适合亲手创作

使用网格、参数、变形器和 BlendShapes 建模，再到动画工作区制作动画。自动生成网格，或手动调整细节。Mesh Edit 支持在当前变形后的模型上编辑，也可切换到原图视图；几何变化时，会重新采样已存储的形态。

Python 脚本也可通过本地 Runtime 独立运行，在编辑器之外完成自动化。建模、动画编辑、预览和检查功能见 [Python SDK](crates/zipzi-python2/README.md)。

## 试用 Zipzi 2D

- **使用编辑器：**从 [GitHub Releases](https://github.com/getzipzi/zipzi-2d/releases) 下载 Alpha 版本，查看发行说明中的平台构建和运行要求。
- **用 Python 自动化：**参考[安装与连接指南](crates/zipzi-python2/README.md)。SDK 目前需要 CPython 3.14。
- **构建自己的工具：**通过 [Rust SDK](crates/zipzi-sdk/README.md) 以编程方式编辑模型。

### 当前能力

| 领域 | 支持内容 |
| --- | --- |
| 建模 | 网格、参数、关键形态、部件、变形器、BlendShapes 和 Glue |
| 动画 | 动作、表情、物理与姿态编辑；动画预览 |
| 项目输入输出 | PSD 导入，以及 Model3/MOC3、Cubism 工程和运行时配套文件等相关格式；支持范围和信息丢失规则因格式及导入／导出方向而异 |
| 自动化 | Python 会话、批量编辑、共享撤销／重做、项目操作与模型检查 |
| 平台 | macOS；Windows 开发原型 |

详情见 [SDK 能力指南](crates/zipzi-python2/CAPABILITIES.md)。GPU 检查需要兼容的宿主和可用 GPU。支持某种格式并不意味着所有模型都能无损往返转换。

## 参与贡献

试用 Alpha 版本，并[告诉我们遇到的问题](https://github.com/getzipzi/zipzi-2d/issues)。报告问题时，请提供构建版本、平台和复现步骤；仅分享你有权提供的模型素材。

贡献代码可从[仓库结构](MONOREPO.md)、[编辑器指南](crates/zipzi-editor/README.md)或 [Python SDK 指南](crates/zipzi-python2/README.md)开始。更多参考资料见[文档索引](docs/README.md)。开发命令和针对性测试说明如下。

## 开发

需要 Rust 1.95 或更新版本、uv 0.10.7 或更新版本，以及 CPython 3.14。macOS 编辑器需要 Xcode Command Line Tools 和支持 Metal 的 GPU。[Windows 开发原型](crates/zipzi-editor/README.md)需要 MSVC、Windows SDK 和可用 GPU。在仓库根目录运行：

```sh
uv sync --locked
python3 tools/workspace/cargo_domain.py --check

# 针对性 SDK 开发
cargo test --locked -p zipzi-sdk --test integration physics_editing::

# 原生编辑器（macOS）
cargo run --locked -p zipzi-editor
./tools/test-editor.sh fast timeline::

# 仓库工具测试；无需构建原生应用
uv run --locked python tools/test_tools.py
```

对于 Python SDK，`./tools/test-sdk.sh python2` 会更新原生适配器，并检查 Local／Connected 行为和静态类型。`python2-connected` 会安装纯 Python wheel，验证真实子进程宿主发现和会话绑定；添加 `--editor-host /absolute/path/to/zipzi-editor` 可指定当前桌面宿主。根目录的 `pyproject.toml` 和 `uv.lock` 管理开发工具。

## 测试

<details>
<summary>针对性测试命令与覆盖范围</summary>

先选择覆盖改动的最小测试范围，再按需扩大。在仓库根目录运行命令；使用 `./tools/test-sdk.sh --help`、`./tools/test-editor.sh --help` 或 `uv run --locked python tools/test_tools.py --help` 查看可用模式。

| 改动范围 | 起始命令 |
| --- | --- |
| 单个 Rust crate／模块 | `cargo test --locked -p <crate> --lib <filter>`; 涉及集成行为时，添加对应集成测试目标 |
| Runtime 所有权／任务／事件 | `./tools/test-sdk.sh runtime` |
| 本地协议／已安装的 Python 客户端 | `./tools/test-sdk.sh python2-connected` |
| SDK 编辑行为 | `./tools/test-sdk.sh fast <filter>` |
| 编辑器自适应网格生成 | `./tools/test-editor.sh fast adaptive_mesh::`; `./tools/test-editor.sh fast mesh_generation::` |
| SDK 基于 alpha 的网格生成 | `./tools/test-sdk.sh mesh` |
| wgpu 后端／本地 GPU 一致性 | `cargo test --locked -p zipzi-render-wgpu --tests`; [宿主选择](crates/zipzi-render-wgpu/README.md#application-routes-on-macos) |
| 编辑器协调逻辑 | `./tools/test-editor.sh fast <filter>` |
| 编辑器 UI（macOS／Metal） | `./tools/test-editor.sh ui <filter>` |
| Runtime Python SDK（`zipzi-python2`）、Local／Connected 与类型 | `./tools/test-sdk.sh python2` ([范围](crates/zipzi-python2/README.md)) |
| 共享编辑契约／独立 Python 生成 | `cargo test --locked -p zipzi-contract`; `uv run --locked python tools/test_tools.py -p test_python2_codegen.py` |
| Runtime GPU 观察／检查（需要可用 GPU） | `./tools/test-sdk.sh python2 --observation` |
| Python 完整调用恢复、Edit 状态与 Job 超时 | `./tools/test-sdk.sh python2 --recovery` （可选 `P3ARecovery.test_name`） |
| Python 集合分页、Session 分离／关闭与离线目录 | `./tools/test-sdk.sh python2 --discovery` |
| 新客户端安装、宿主发现与会话绑定 | `./tools/test-sdk.sh python2-connected` （可选 `--editor-host /absolute/path/to/zipzi-editor`） |
| 仓库工具 | `uv run --locked python tools/test_tools.py -p test_<tool>.py` |

过滤条件和文件名均为占位示例；省略它们即可运行所选测试范围。各 crate 的集成测试目标见其 README 和 `tests/`，或用 `cargo test --locked -p <crate> --test <target> -- --list` 列出用例。Editor shell 测试入口会清除 `ZIPZI_DEV_SESSION_DIR`；直接运行 Editor Cargo 测试时，请手动清除。

[完整覆盖范围、前置条件与 CI](tools/README.md#test-coverage)说明回归、压力、原生验收及打包 wheel 测试。两个领域的 shell 脚本在未指定模式时，默认执行开销较大的 **full** 测试。直接运行 `cargo test` 仅选择 `zipzi-core`、`zipzi-project` 和 `zipzi-sdk`。工具测试位于 `tools/tests/`，crate 测试位于各 crate 内，共享数据位于 [`fixtures/`](fixtures/README.md)。

项目自有的 crate 共用根 Cargo workspace 和锁文件。本地模型放在 `models/local/`，参见[模型清单](models/README.md)。源码归档、Python wheel 和 macOS 应用包分别有对应的[分发检查](tools/README.md#local-distribution-artifacts)。

</details>

## 许可证与第三方权利

SDK 组件使用 [MIT](LICENSE-MIT) 许可证；编辑器组件使用 [GPL-3.0-only](LICENSE-GPL-3.0)。[LICENSING](LICENSING.md) 说明适用范围和共享文件来源；[第三方声明](THIRD_PARTY_NOTICES.md)保留依赖项和基准测试的例外说明。

Live2D、Cubism、Live2D Cubism SDK、Cubism Core、Cubism Native Framework 及相关示例数据由 Live2D Inc. 和／或各自权利人拥有或授权。本项目独立开发，与 Live2D Inc. 无隶属关系，亦未获其授权、认可或赞助。「Live2D」和「Cubism」名称仅用于说明互操作性。

本仓库不分发专有 Cubism Core 二进制文件、Cubism SDK 包或 Live2D 示例模型素材。获取、构建、链接、发布或分发使用 Live2D 材料的软件时，用户须遵守所有适用条款，包括 [Live2D 专有软件许可协议](https://www.live2d.com/eula/live2d-proprietary-software-license-agreement_en.html)、[Live2D 开放软件许可协议](https://www.live2d.com/eula/live2d-open-software-license-agreement_en.html)和适用的[示例数据条款](https://www.live2d.com/eula/live2d-sample-model-terms_en.html)。本仓库的 MIT 许可证不授予任何第三方软件、模型、美术素材、商标或其他材料的权利。
