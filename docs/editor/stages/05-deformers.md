# Stage 05：原生变形器

实施范围：Rotation + 固定规格 Warp。复用独立 C++ Document、进程内 GDScript、Godot UndoRedo Action；不提取 Purism/Cubism 的复杂求值实现。

## 坐标与求值约定

- 模型为 Y-up；Rotation 的正角为逆时针，单位度，围绕给定中心。
- Warp 的 origin 是 rest 矩形左下角，size 正值，行列数各 1–16 个单元；控制点从下到上逐行、每行从左到右。规格创建后固定，控制点可整批更新。
- Warp 使用分片双线性插值；区域外沿最近边界单元外推，不夹紧位置。允许折叠，不自动修正形态。
- 所有源顶点、中心、rest 矩形和控制点使用同一未变形模型坐标图。每一级的输入是子级输出。先最近变形父节点，再逐级祖先；例如 Mesh → Warp → Rotation。
- 不提供独立局部 TRS 或绑定时保持世界姿态；改父节点立即按新链重新求值。
- 变形父节点只能为变形器；组织父节点可以为 Mesh 或变形器，完全不参与求值。两种图独立验证，禁止环，最多 16 条父边。
- 源顶点与求值输出分离。Document 保存父链接，变更时沿链接筛选受影响网格，仅上传这些网格；不引入通用调度器。

## 数据与文件

新增 Rotation/Warp 数据对象、脚本句柄、批量控制点编辑、父绑定和求值查询。数据直接写入，可选 Action 复用已有源快照与原生 UndoRedo。

保存写 JSON v2，包含变形器、变形父链接和组织父链接；兼容读取无变形器的 v1。候选项目完整验证后再替换当前文档。

## 验证范围

核心测试覆盖数学端点、中点、区域外外推、嵌套顺序、循环和深度；Godot 测试核对相同输入的求值、源数据隔离、无关网格不上传、Action 撤销重做、保存重开、坏文件保留当前文档；实际 GPU 检查旋转和 Warp 结果。

## 脚本 API

`KasaneDocumentBridge`：

- `create_rotation(id, name, center, angle)` / `create_warp(id, name, origin, size, columns, rows)`。
- `get_deformer(id)` / `get_deformer_snapshot(id)`。
- `set_rotation(id, center, angle)` / `set_warp_points(id, points)`。
- `set_deform_parent(child, parent)` / `set_organization_parent(child, parent)`，空 parent 解除绑定。
- `evaluate_mesh(id)` 返回求值位置，不覆盖 `KasaneMeshData.positions`。

`KasaneDeformerData`：`angle_degrees`、`center`、`control_points` 属性；结构化错误接口 `update_rotation`、`update_control_points`、`bind_to`。共享句柄类的属性按 `snapshot().kind` 使用；类型不匹配写入明确失败。读取数组是副本，需写回。重开后的旧句柄失效。

修改均可直接进行，不强制 Action。需要撤销时使用 Stage 04 的 `actions.perform` 或 Godot 原生属性 Action。源数据快照自动包含变形器和两种父链接，不另建历史系统。

## 项目格式 v2

保留 v1 的 `format`、`document.id/canvas/assets/meshes`。新增必需数组：

- `document.deformers`：每项含 `id/name/kind`。Rotation 含 `center` 数字对和 `angle_degrees`；Warp 含 `origin/size` 数字对、整数 `columns/rows` 和完整 `control_points` 数字对数组。
- `document.deformation_links`、`document.organization_links`：每项为 `child/parent` 对象 ID。同一数组禁止重复 child；不保存空父链接。

先创建所有对象，再加载两类链接。未知类型、非法规格、缺失控制点、缺失父节点、循环、超深链及不可有限求值的项目均拒绝，不替换当前文档。读取 v1 时没有变形器；之后保存升级为 v2。纹理仍引用资源路径，未引入打包格式。

低层直接写入有限但极大的值可能导致求值超出 float32；返回 `preview_ok=false` 与 `EVALUATION_OVERFLOW`，保持上次有效渲染数据，不伪造成功上传。`capture` 会检查求值与预览一致性，拒绝把旧画面标注为当前 revision。源数据仍可被脚本修正或显式撤销。

## 样例与实际验收

2026-09-19，macOS arm64 / Apple M4 / Godot 4.7.2，Compatibility OpenGL renderer。

```sh
python3 tools/run_kasane_preview.py --render
/Applications/Godot_mono.app/Contents/MacOS/Godot --path demos/kasane-preview -- --edit-script=res://scripts/stage05_deformers.gd
```

- 脚本：`demos/kasane-preview/scripts/stage05_deformers.gd`。25 个源顶点、32 个三角形，2×2 单元 Warp（9 个控制点），20 度 Rotation。
- 可重开项目：`demos/kasane-preview/samples/stage-05.kasane.json`，测试比较其求值结果与脚本生成结果一致。
- CTest：2/2 suites 通过，新增核心数学、固定拓扑校验、父图循环/深度和数值溢出检查。
- Godot：原有 158 项集成检查、34 项脚本检查通过；新增 56 项变形器检查通过。
- 真实 GPU：原有渲染测试通过；新增 Rotation 四象限方向、Warp→Rotation 位移、旧几何消失、保存重开像素一致性、无效求值截图拒绝测试通过。
- Action：创建与绑定整体撤销重做、控制点与角度同一步历史、Undo 回保存点验证通过。
- 资源：固定拓扑变形保留 RID，无关网格上传计数不变；最终测试没有对象或 GPU 资源泄漏报告。

日志位于 `target/kasane/` 的 `core-tests.log`、`deformer-tests.log` 和 `deformer-render.log`。截图位于演示 `artifacts/stage05-rotation.png`、`stage05-warp-rotation.png`、`stage05-reopened.png`、`stage05-demo.png`；这些是可重建的本地产物。

Stage 06 参数与 Keyform 尚未实现。本阶段不提供反求、任意变形器类型、动态 Warp 拓扑或完整 Cubism 兼容；Action 快照恢复仍重建全部预览，正常直接变形更新按受影响网格上传。
