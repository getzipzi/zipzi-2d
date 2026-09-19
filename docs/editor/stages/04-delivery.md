# Stage 04 交付与验收记录

日期：2026-09-19。状态：macOS arm64 / Godot 4.7.2 下已验收。

## 交付

- 演示程序内的本地会话服务 `session_server.gd`，仅绑定 `127.0.0.1`，不保存或重载项目来同步编辑。
- 标准库实现的 Python SDK `python/kasane_sdk`，支持对象摘要、按 ID 查询、网格几何批量读取、规则网格创建、批量位置事务与 Undo/Redo。
- 核心按预期 revision 原子提交的入口。所有脚本位置修改仍使用 Stage 02 的验证、ChangeSet 和历史路径。
- 固定示例 `python/examples/edit_open_document.py`，以及独立 Python 进程连接真实 Godot 进程的集成测试。

## 验收

```sh
python3 tools/run_kasane_preview.py --render
```

外部会话测试覆盖：查询摘要/网格/Asset、两次位置命令的一次提交与一次 Undo、预览数组即时一致、Redo、旧 revision 拒绝、无效批次整体回滚、规则网格创建和断线取消未提交草稿。Godot 集成测试和实际 GPU 像素测试同时回归通过。连接方式见 [Python session guide](../PYTHON_SESSION.md)。

限制：Stage 04 的规则网格创建是独立结构操作；结构 Undo 留给未来阶段。一次只服务一个本机客户端，且未实现身份认证，不应暴露该端口。
