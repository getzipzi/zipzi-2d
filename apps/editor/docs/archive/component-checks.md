# 组件检查记录

2026-09-27，Apple M4 / macOS / Retina 2×。当前两个应用均使用官方 GPUI Kit，旧的手写组件检查记录由 Git 历史保留。

## 已验证的 Kit editor 行为

- 名称 Input 居中，英文输入和中文粘贴可同步到左侧图层列表。
- Checkbox 点击切换可见性；从 Input 按 Tab，再按 Space 可切换 Checkbox。
- Button 缩放与重置预览，Slider 点击轨道更新透明度。
- 浅/深主题显示正常，约 800×480 与 1100×740 内容尺寸保持控件可见。
- 关于 Dialog 打开，Escape 关闭。
- 原生 Metal 合成测试覆盖 RGBA/BGRA、透明度、裁剪、叠放、隐藏、旧帧保留和尺寸变化。用户已确认原生 Metal editor 运行正常。

## Playground

使用同一 Kit 依赖，包含 Button、Checkbox、Input、Slider、Tabs、Dialog、Typography 七页，保留各页状态。原来的手写下拉菜单、树列表和自定义组件 API 文档已移除；新增示例应直接组合官方 Kit 组件。

## 验证分工

后续 UI 交互与视觉验收由用户手动完成，不使用 computer-use 自动测试。自动检查保留编译、格式和 GPU 像素回归测试。

## 仍需验证

- 中文粘贴不是 IME 测试。请在 Input 页使用系统中文输入法连续输入，核对预编辑、候选窗、选词上屏和删除，不应据粘贴结果宣称重复上屏问题已解决。
- Slider 的 CUA accessibility setValue 没有更新状态，轨道点击正常；方向键及屏幕阅读器未全面验证。
- 输入撤销、长文本滚动、全部控件的焦点恢复、Linux / Windows 尚未完整验证。
