# Kasane Editor — assembly experiment

最小 Rust monorepo，用于验证 **gpui-ce UI + kasane-2d + wgpu 29** 能否组成一个原生 2D editor。
目前面向 macOS；不是文本编辑器，也不是完整模型编辑器。

```text
crates/editor        GPUI 窗口、工具栏、场景列表、检查器、画布宿主
crates/editor-scene  Kasane DrawableFrame 与 wgpu 渲染，独立于 UI
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
cargo test --workspace
cargo fmt --all -- --check

# 可选：生成可双击打开的 macOS 应用
./scripts/build-app.sh
```

依赖使用 workspace path，方便同时修改三个仓库。`Cargo.lock` 锁定 registry 依赖；相邻仓库版本见 `DEPENDENCIES.md`。外部库仍使用相邻 checkout；组件实验页另保留了一份带许可的上游输入框示例。

## 验证内容

- AppKit 负责原生窗口和输入，GPUI 的 UI 渲染使用 **wgpu → Metal**。
- Kasane 复用 GPUI 暴露的同一个 device/queue，将四顶点菱形渲染到离屏纹理。
- GPUI 直接采样该纹理合成到画布，无 CPU readback；仅 GPU 测试进行像素回读。
- `+` / `−` 改变视图缩放，`Opacity ±` 改变 Kasane drawable 透明度，`Diamond` 切换可见性，`Reset` 恢复默认。
- 窗口尺寸和 Retina 比例决定实际纹理尺寸。
- GPU 测试检查中心颜色、透明背景、透明度、隐藏、缩放和 resize。

## Fork 改动与边界

`gpui_ce_platform/wgpu` 是新增的可选 feature；默认的原生 Metal 后端仍然保留。Fork 增加 CAMetalLayer 上的 wgpu renderer、macOS typed GPU context、custom texture Surface，并保留 CoreVideo surface 合成。

这是最小验证：没有模型导入、文件保存、撤销、时间轴、拖拽顶点或 GPU device-loss 自动恢复。AppKit wgpu 的 `render_to_image` 测试接口暂不支持。Linux / Windows / Web 未在此项目验证。

## 验证记录

2026-09-27，Apple M4 / macOS：`cargo test --workspace`、格式检查通过；原生窗口实测首帧、文字、纹理合成、缩放、透明度、隐藏和重置正常。默认非 wgpu 的 gpui-ce macOS 路径也通过 `cargo check`。

## 常用组件验证页

点击工具栏 **Components** 可测试按钮、输入框、复选框、滑块、下拉菜单、标签页、树列表、滚动裁剪和弹窗。Typography 页检查多语言字体和长文字截断；Compact / Full size 切换窗口尺寸。

实测结果与未覆盖范围见 [组件检查记录](docs/component-checks.md)。文本编辑内核取自 gpui-ce 的 Apache-2.0 示例，许可保留在 `crates/editor/src/vendor`。
