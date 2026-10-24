# UI draw 修复验证

2026-10-24。对照为修复前的 kasane-editor `b4e4abc` 与同一提交加本次修复，SDK 均为 kasane-2d `e27230a`。这是本次修复的 before / after 对照；历史 `44accd1` / `b86b13` 基线仍见原 benchmark 报告。问题定位见 [诊断报告](2026-10-24-ui-draw-diagnosis.md)。

Physics / Motion / Expression 的同一资源在 revision 更新、上下文和字段控件配置未变时复用控件；仍更新 PropertyTarget 和表单校验版本，清理旧输入草稿。对象、字段结构、上下文变化，或存在预览 / 打开的菜单时，继续使旧控件和回调失效。数字输入 Enter 明确退出文本编辑，保留随后拖动的行为。共享 `List::set_searchable` 仅在值变化时通知。

69 个定向测试全部通过，包含新增的 5 个回归：控件复用及 revision / 结构防护、旧草稿清理、关闭 / 打开 Select 的版本边界、静态属性保留绘制、setter 的通知次数。现有 Physics、Properties、Business / Animation authoring、输入协调和 compact popup 测试通过；根工作区格式检查、vendored 文件格式检查及 diff 空白检查通过。

## 原生 Release 对照

Physics 每项 40 个正式样本 + 5 个 warmup；播放阶段约 7.5 秒。两轮交换 before / after 运行顺序；使用保存的 Release 二进制，不加入诊断计数或 CPU sampler。表中为两轮 UI draw P95 的中位数，单位 ms。

| 负载 | 修复前 | 修复后 | 变化 |
|---|---:|---:|---:|
| Physics input weight text | 7.605 | 5.864 | -22.9% |
| Physics node mobility text | 6.884 | 6.100 | -11.4% |
| 播放 | 3.409 | 3.029 | -11.2% |

逐轮结果：

| 负载 | 轮次 | 修复前 P95 ms | 修复后 P95 ms |
|---|---:|---:|---:|
| Physics input weight text | 1 | 6.565 | 5.913 |
| Physics input weight text | 2 | 8.644 | 5.814 |
| Physics node mobility text | 1 | 6.956 | 6.055 |
| Physics node mobility text | 2 | 6.812 | 6.146 |
| 播放 | 1 | 3.399 | 3.036 |
| 播放 | 2 | 3.420 | 3.022 |

额外运行修复后的完整 Physics suite：12 个 case 均完成，包括文本提交、连续拖动、反射和 idle。全部 13 份 trace 通过现有 reporter，lost_events = 0；每次运行的 1 Hz 进程快照均未观察到 Cargo / rustc 等编译器。按照用户要求保留后台任务。

UI draw 是整窗 wall time。两轮结果是本机有限样本，不是稳定的普遍收益保证；例如 input weight 的修复前 P95 在两轮中有明显波动。窗口呈现与属性实际变化由原生脚本和 reporter 校验，不能把 draw 时间直接当作输入到显示延迟。

[机器可读结果](2026-10-24-ui-draw-fix.json) 保存二进制、资产、benchmark 源码和产品修复文件的 SHA-256、逐轮指标、trace 哈希与测试日志路径。本地原始数据、隔离配置、构建日志及修复补丁位于 `target/ui-draw-fix-20261024/`。
