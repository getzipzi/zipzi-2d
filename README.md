# Kasane Editor

最小 Rust monorepo，验证 **GPUI Kit + kasane-2d** 的原生 2D editor 组装。当前面向 macOS，不是完整模型编辑器。

```text
crates/editor                主 editor：Kit 工具栏、名称输入、属性栏与 Kasane 预览
crates/editor-scene          独立于 UI 的 Kasane 场景与 wgpu 渲染
crates/component-playground  Kit 组件示例、状态反馈与 Rust 用法
```

## 运行

需要 Rust stable、Xcode Command Line Tools，以及相邻的 Kasane checkout：

```text
web/
├── kasane-editor/
└── kesane-2d/       https://github.com/pakholeung37/kasane-2d
```

`kesane-2d` 是现有本地目录名。GPUI Kit 从官方仓库的固定 commit 获取，不再需要本地 gpui-ce 或 gpui-kit checkout。

```sh
cargo run --locked -p kasane-editor
cargo run --locked -p component-playground
cargo test --locked --workspace
cargo fmt --all -- --check

# 生成可双击运行的 macOS 应用
./scripts/build-app.sh
./scripts/build-app.sh playground
```

应用输出为 `target/Kasane Editor.app` 和 `target/Kasane Component Playground.app`。依赖版本见 [DEPENDENCIES.md](DEPENDENCIES.md)。

## Editor

使用 Kit 官方 Input、Checkbox、Slider、Button、Dialog、主题和窗口 Root。名称输入同步到图层列表；缩放、透明度、可见性更新 Kasane 预览。重置预览恢复这些显示属性并保留名称。

UI 使用官方 GPUI 的 macOS Metal 后端。Kasane 在后台线程使用独立 wgpu 29 设备绘制 768×768 预览，再回读为 BGRA 图像交给 GPUI。此路径包含 CPU 回读和纹理上传，**不是共享设备或零拷贝方案**。窗口缩放由 GPUI 缩放预览图像。

所有更改仅保留在当前窗口。尚无文件保存、模型导入、撤销、时间轴、顶点编辑或停靠布局。中文粘贴已测试，系统中文输入法组合输入与连续上屏仍待实测。

## 组件验证

Playground 提供 Button、Checkbox、Input、Slider、Tabs、Dialog、Typography 页面，展示真实 Kit 组件、当前状态和可复制的用法片段。页面切换保留输入状态，可切换浅/深主题。旧的手写 `ui` crate 和实验性双入口已移除，历史实现可从 Git 历史查看。

实现与限制见 [editor 说明](docs/kit-editor.md)，测试记录见 [组件检查记录](docs/component-checks.md)。
