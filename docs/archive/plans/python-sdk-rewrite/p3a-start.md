# P3A-0：启动清单与失败基线

日期：2026-10-25。状态：**历史启动切片；后续 P3A-0 已验收并进入 P3A-1。**
完整冻结结果见 [P3A-0](p3a-0.md)，恢复实现及验证见 [P3A-1](p3a-1.md)。
本记录对应 [P3A 方案](p3a-api-agent-contract.md) 的冻结与独立失败用例工作，
下列首轮记录不表示新 API、安装包或 agent 已验收；启动切片当时尚未改变生产 Python/Rust 实现。

## 本轮落地

- [清单与基线工具](../../../../tools/python/python2_contract.py)：`inventory`、`freeze`、`baseline`
  三个入口都有 `--help`，输出路径必须是新文件，不覆盖旧证据。
- [严格目标测试](../../../../crates/zipzi-python2/tests/test_p3a_contract.py)：使用实际 Local
  适配器和独立 Connected 测试宿主，恢复故障由现有宿主 fixture 丢弃响应触发。
  没有 `expectedFailure`、跳过或假后端答案来把缺口记作通过。
- [工具回归](../../../../tools/tests/test_python2_contract.py)：验证确定性、继承方法、领域入口、
  dataclass/普通实例字段、意外导出、缺失导出、注解错误、只增证据文件和结果记录。
  空测试、skip、expected failure、测试期间输入变化均不能通过。
- 当前公开表面清单：3,682 条路径，包括 1,223 条模块导出、
  757 条方法、41 条属性、62 条普通实例属性、1,464 条值字段、120 条枚举成员、
  15 条可达领域入口。包含重导出、辅助 codec 和意外泄漏，**这些数字不是唯一 API 数量或覆盖率**。
  `target_status=needs_review` 表示尚未逐项批准目标去向，不能拿扫描结果冒充最终公共目录。
- 启动验收：记录当时的工作区和源码身份、
  原生适配器/CPU 宿主/Editor 文件哈希，以及本轮失败和现有行为对照。

## 冻结身份与范围

启动提交为 `29024dded10f258756f87506aefaf1541558d8c7`，开始前 `git status --short` 为空。
启动时已检查工具/测试加入后的源码身份；历史 manifest 和哈希 JSON 产物已清理，
本页保留当时的验收范围与结论。

清单来自纯 Python 包静态成员/源码检查，没有实例化 Runtime、连接宿主或导入原生扩展。
已有能力范围包含当前 R4 Modeling、工程格式、异步采样、Buffer、ImageLease、PNG 和地址接口；
不能按早期 P3 文档列出的少数方法缩小后续重构范围。

原生适配器和 CPU 宿主由 `./tools/test-sdk.sh python2 PythonSdk.test_shared_recipe`
刷新后用于失败基线；共享 recipe、严格 Pyright 正例和 16 条既定类型反例通过。
Editor 仅记录文件身份，**没有执行本轮 Editor 验收**。本轮没有构建/安装正常 wheel 或 sdist，
没有重新采集 P0 性能，也没有运行 agent。文件存在和哈希相同不等于这些门禁通过。

## 实际失败基线

最终基线 `tests_run=11`，两项含 Local/Connected 子用例，共记录 13 条未通过结果。
测试前后源码与适配器/宿主身份保持不变。命令按设计返回非零，报告状态为 `failed`。

| 定向用例 | 重构前实际结果 | 目标与后续任务 |
| --- | --- | --- |
| mesh / parameter / image / binding 创建丢响应，4 项 | 恢复为 `WriteResult` | 完整调用投影后恢复为 `str`；P3A-1 |
| scene 单项读取丢响应 | 恢复为 `scene.Batch`，正常为 `scene.Snapshot` | 正常与恢复都为单项 Snapshot；P3A-1/2 |
| commit 丢响应 | 恢复已等待并返回 `Receipt` | 恢复返回 `Job[Receipt]`；P3A-1 |
| 捕获写入 OutcomeUnknown 后离开 Edit | 上下文没有再次抛出异常 | 未确认状态必须再次抛出可恢复异常；P3A-1 |
| `get_type_hints(OutcomeUnknown.recover)` | `RecoveryResult` 的 NameError | 可解析的 `object` 返回注解；P3A-1/5 |
| 创建 257 个同名网格后分页，两个后端 | 缺少 `meshes.list` | 完整、有界且来源一致的页面；P3A-4 |
| 默认预算下循环关闭 40 个 Session，两个后端 | 缺少 `Session.close` | 正确关闭并释放注册表槽；P3A-4 |
| 1,000 顶点 Geometry 默认 repr | 28,953 字符 | 至多 1,024 字符且保留完整字段；P3A-4 |

这些用例在修复成功后还会检查独立的名称、像素、候选 sequence、历史、重复恢复拒绝、
完整分页顺序和资源槽释放。当前在较早的失败断言处停止，**尚未执行到的断言不算通过**。
场景测试使用实际 `scene_snapshot` 故障名；最初试跑的错误拼写已更正，初轮输出仍保留在
`target/python2/p3a-start/red-baseline.json`，本记录采用更正后结果。

## 现有行为对照

以下 6 项原有测试通过，原始日志保存在证据 JSON；原测试没有改写或放宽预期：

- BlendShape、绑定拓扑与候选 CPU sweep。
- Mesh binding 候选/正式来源及 CPU sweep。
- Canonical Modeling records、Plan 与绑定拓扑。
- 图片替换、旧快照和 Undo/Redo 固定像素。
- SDK 引用、标签地址与 Undo 失效。
- Local 原生桥接不经过 Python JSON 编解码。

工具自身的 6 项回归通过。未运行昂贵的完整 SDK/Editor 套件。

## 可复现入口

从仓库根目录执行，输出文件已存在时更换新名称：

```sh
uv run --locked python tools/python/python2_contract.py inventory --output target/python2/p3a-inventory-new.json
uv run --locked python tools/python/python2_contract.py freeze --output target/python2/p3a-freeze-new.json
./tools/test-sdk.sh python2 PythonSdk.test_shared_recipe
uv run --locked python tools/python/python2_contract.py baseline --output target/python2/p3a-baseline-new.json
uv run --locked python tools/test_tools.py -p test_python2_contract.py -v
```

定向运行某项目标回归可在 `baseline` 后追加，例如
`P3AContract.test_scene_get_recovery_keeps_single_snapshot`。目标测试当前独立于
`test_api.py` 的既有验收，以便分别记录旧行为通过和新契约失败；后续恢复契约修复后
再纳入相应常规门禁。未改变正常测试脚本的默认范围。

## 启动切片当时的剩余工作

1. 将清单中的每个现有符号映射到明确目标路径、类型、行为或删除理由，区分重导出、
   codec 泄漏与必须保留的现有 R4 能力；不能批量把 `needs_review` 改成保留而省略审查。
2. 登记所有目标新增/改变方法的完整签名与结果类型，包括现有 R4 能力如何进入统一契约。
3. 冻结 G01–G08 的 agent 模型版本、提示、工具、调用/时间/token/输出预算和 fixture 种子，
   为 14 个任务环境 × 3 次运行提供可复核配置。
4. 按计划补齐 P0 重构前比较基线的适用产物身份与性能采样，不用本轮 Debug fixture
   替代既有 Release 性能证据。

上述工作其后已由 [P3A-0](p3a-0.md) 验证；本页保留启动验收结论。
P3A 范围和后续 D/C/G 门槛不变。
