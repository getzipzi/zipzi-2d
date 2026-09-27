# Kasane Editor — assembly experiment

最小 Rust monorepo，用于验证 **gpui-ce UI + kasane-2d + wgpu 29** 能否组成一个原生 2D editor。
目前面向 macOS；不是文本编辑器，也不是完整模型编辑器。

```text
crates/editor        GPUI 窗口、工具栏、场景列表、检查器、画布宿主
crates/editor-scene  Kasane DrawableFrame 与 wgpu 渲染，独立于 UI
crates/ui            可复用的 GPUI 组件；src/components 中每个组件独立成文件
crates/component-playground  独立运行的组件文档与交互验证程序
```

## 运行

需要 Rust stable、Xcode Command Line Tools，以及相邻 checkout（目录名 `kesane-2d` 是现有本地目录名）：

```text
web/
├── kasane-editor/
├── kesane-2d/       https://github.com/pakholeung37/kasane-2d
└── gpui-ce/         https://github.com/pakholeung37/gpui-ce
                    branch: kasane/wgpu-macos
```

```sh
cargo run -p kasane-editor
cargo run -p component-playground
cargo test --workspace
cargo fmt --all -- --check

# 可选：生成可双击打开的 macOS 应用
./scripts/build-app.sh
./scripts/build-app.sh playground
```

依赖使用 workspace path，方便同时修改三个仓库。`Cargo.lock` 锁定 registry 依赖；相邻仓库版本见 `DEPENDENCIES.md`。外部库和输入组件均直接使用相邻 checkout，没有复制输入框实现。

## 验证内容

- AppKit 负责原生窗口和输入，GPUI 的 UI 渲染使用 **wgpu → Metal**。
- Kasane 复用 GPUI 暴露的同一个 device/queue，将四顶点菱形渲染到离屏纹理。
- GPUI 直接采样该纹理合成到画布，无 CPU readback；仅 GPU 测试进行像素回读。
- `+` / `−` 改变视图缩放，`Opacity ±` 改变 Kasane drawable 透明度，`Diamond` 切换可见性，`Reset` 恢复默认。
- 窗口尺寸和 Retina 比例决定实际纹理尺寸。
- GPU 测试检查中心颜色、透明背景、透明度、隐藏、缩放和 resize。

这是最小验证：没有模型导入、文件保存、撤销、时间轴、拖拽顶点或 GPU device-loss 自动恢复。AppKit wgpu 的 `render_to_image` 测试接口暂不支持。Linux / Windows / Web 未在此项目验证。

## 验证记录

2026-09-27，Apple M4 / macOS：`cargo test --workspace`、格式检查通过；原生窗口实测首帧、文字、纹理合成、缩放、透明度、隐藏和重置正常。默认非 wgpu 的 gpui-ce macOS 路径也通过 `cargo check`。

## 常用组件验证页

运行 `cargo run -p component-playground` 打开组件文档。左侧按 Button、Checkbox、Text field、Slider、Tabs 分页；每页都有实时示例、状态展示、可复制的 Rust 用法、API 和交互说明。Patterns 页保留下拉菜单、树列表与弹窗的组合实验，Typography 页检查字体回退与截断。Compact / Full size 切换窗口尺寸。文档页面位于 `crates/component-playground/src/playground/pages`，editor 仅依赖 `ui`。

新增组件时，在 `crates/ui/src/components/` 增加独立模块并从 `mod.rs` 导出；在 playground 的 `pages/` 增加对应文档页及侧栏入口，展示实际组件、状态、用法和 API。editor 通过 `ui` 的公开入口使用组件。

实测结果与未覆盖范围见 [组件检查记录](docs/component-checks.md)。输入框直接使用 `gpui_ce_elements::editable_text`，与上游 `editable_text` 案例一致。
