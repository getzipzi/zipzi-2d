# Stage 04：外部 Python 编辑当前会话

从仓库根目录先启动 Godot 演示，再在另一终端运行示例：

```sh
python3 tools/run_kasane_preview.py --skip-build --demo
PYTHONPATH=python python3 python/examples/edit_open_document.py
```

首次运行请先执行 `python3 tools/run_kasane_preview.py` 构建扩展。演示监听 `127.0.0.1:43884`；可用 `KASANE_EDIT_PORT` 改端口。演示右侧 Document 不再自动动画，脚本提交后可直接看到网格变化。外部连接是本机开发接口，未提供身份认证，不要转发端口到其他主机。

SDK `KasaneClient` 提供 `summary()`、`get_mesh(id)`、`get_asset(id)`、`preview(id)`、`create_grid(...)`、`begin(revision)`、`stage_positions(...)`、`commit()`、`cancel()`、`transaction(revision)`、`undo()`、`redo()`。几何以整批数组传输；`get_mesh` 返回稳定 Vertex ID、基础位置、UV 和三角形，不逐点 RPC。规则网格由客户端提供或自动生成 Mesh UUID，给定现有纹理 Asset ID、行列数和模型宽高创建；目前创建是独立结构操作，不纳入位置事务的 Undo 历史。

协议是回环 TCP 上的逐行 JSON，一个会话只接受一个活动客户端。每条请求有 `method`，结果有 `ok`；失败附 `code`、`message`。Python SDK 将失败转为 `KasaneError(code, message)`。服务运行在 Godot 主线程，每帧读取请求；没有模型文件往返。事务命令先保存在连接层，`commit` 时以预期 revision 调用 Stage 02 的核心原子批量入口，整个批次只生成一个 Undo 步骤。错误或断线丢弃未提交命令；过期 revision 返回 `STALE_REVISION`。`revision` 经 JSON 数字传输，当前协议只保证 IEEE-754 精确整数范围内的值。

本阶段没有多客户端合并、长期草稿、脚本沙箱或远程网络服务。
