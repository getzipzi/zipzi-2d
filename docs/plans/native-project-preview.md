# 原生工程静态预览

日期：2026-11-04。实现基线：`cc2f32ba`。状态：实现已接入；原生交互验收部分待补。

目标是在 Finder / Explorer 中显示 `.zipzi` 的模型静态预览，不启动 Editor。
原生工程格式的剩余 Windows / Finder 验收继续记录在原 plan，不因本工作自动完成。

## 分阶段交付

1. 直接修改容器 v1：增加可选 `preview.png` 与 SHA-256、尺寸声明，不增加 required feature。
   按用户要求不递增版本，不实现旧格式兼容或迁移。
   提供独立、有限额的预览读取库，不解码工程 JSON、展开素材或访问外部资源。
2. GPU 宿主保存时，从冻结的文档与资源生成默认姿态的全模型预览。
   输出最大 1024×1024，不含选择、网格和编辑器 UI。无 GPU、超出预算或生成失败
   不影响工程保存；每次保存重新生成或省略，不复用旧图片。
3. macOS Quick Look Thumbnail / Preview 扩展，随应用构建和打包。
4. Windows Thumbnail Provider / Preview Handler，随便携分发注册、卸载。

缩略图与预览仅是保存时的静态图，不提供动画播放或模型交互。
预览不是 authoring 数据，不改变文档版本、历史或 dirty 状态。
读取器对 ZIP 索引、编码字节、PNG 尺寸和解码内存设上限，缺失或损坏时让系统回退。
系统预览不读取完整工程来验证 authoring 内容，其结果不代表整个工程通过校验。

## 验证

- 容器保持 v1、有/无预览、旧预览不复用、损坏/超限/取消和原子发布语义。
- 默认姿态渲染的输出尺寸、实际像素和资源生命周期。
- macOS 扩展编译、bundle 元数据、签名与原生预览。
- Windows 原生构建、COM 注册/卸载和 Explorer 交互须对应平台实测。
- 每个实现阶段完成后记录结果并提交；未执行的实机门禁不得记为通过。

## 本地结果（2026-11-04）

- `zipzi-project-preview` 独立读取器与 C ABI、v1 可选条目、SDK/Runtime 保存已接入。
  CPU 宿主省略预览；GPU 宿主额外预留 256 MiB，资源预算不足、渲染失败或空模型
  均可省略。条目数、format/expanded/archive 字节或 staging 磁盘预算不足时亦回退。
- 读取器 4 项测试通过；archive 26 项测试通过，包含损坏预览不阻碍编辑、预览不复用、
  限额省略、取消和原子发布。Runtime host 在 CPU / GPU 两种配置下各 6 项通过，
  GPU 保存实际生成图像，128 MiB 总预算下仍能保存无预览工程。
- GPU 默认姿态预览测试通过（1024×1024、非透明像素、保持实时预览参数与文档版本、取消）。
  `cargo check --locked -p zipzi-editor` 通过。
- macOS 两个扩展本机编译、签名和发现通过。Quick Look 系统 XPC 宿主实际返回测试图的
  256×128 缩略图并核对像素；原生 Provider 对正常、缺失、损坏图的直接测试通过。
  测试用 bundle/扩展已注销并移除，默认打开应用恢复 Archive Utility。
  完整 debug app 构建和分发产物校验通过；外层签名后 `codesign --verify --deep --strict`
  通过，扩展仍保留 `com.apple.security.app-sandbox` 权限。
- Windows Thumbnail Provider / Preview Handler、按用户注册/卸载、便携包与 CI 构建已接入；
  添加原生 COM 像素、窗口缩放/卸载及异常输入 smoke gate，**本机未执行 Windows 构建**。
  Windows 打包工具 2 项测试通过。
- 剩余：Finder 手工空格预览/缩放、Windows 原生构建和 COM 注册/卸载及 Explorer 交互，
  Developer ID 签名/公证。不得把直接 Provider 测试等同于完整原生交互验收。

构建/注册入口见 [native/project-preview/README.md](../../native/project-preview/README.md)。
