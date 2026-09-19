# MOC3 v5 写出映射（M1 静态与参数增量）

状态：已实现静态网格、普通参数与 Mesh 位置 Keyform 数据链路，**M1 未验收**。正式变形器、Part、绘制属性与遮罩、通用资源包发布器、GPU 对照仍待实施。架构及编辑契约见 [核心重构说明](../M1-CORE-REFACTOR.md)。

## 正式代码与复现

- `modules/kasane-core/include/kasane/evaluation.hpp`：`evaluate_frame` 输出统一 DrawableFrame；`moc3.hpp` 的 `encode_moc3` 输出文件字节/资源描述/纹理槽。求值属于核心，不调用编码器、不加载纹理、不操作文件。
- `modules/kasane-core/src/moc3.cpp`：独立 `kasane_moc3` 库目标，依赖 `kasane_core`，不依赖 Godot 或 Core ABI。
- `modules/kasane-core/src/moc3_sections.inc`：完整 v5 的 152 个 section 的索引、显式元素宽度、count 索引。依据 PurismCore 的 `moc3.h`，保留来源说明。
- `modules/kasane-core/tests/moc3_tests.cpp`：同一构造程序分别链接 PurismCore 和官方 Core。新文件未借用或修改任何输入 MOC3。

从仓库根目录运行：

```sh
python3 tools/validate_m1_core.py
```

官方 SDK 默认位于 `third_party/CubismSdkForNative-5-r.5`，也可用 `--sdk /absolute/path` 指定。支持当前 macOS/Linux SDK 常规静态库目录；缺 SDK 时返回非零且报告 `not_run`，不跳过官方 Core 验证。

产物位于 `target/kasane/m1-core/`：两个 Core 的数据比较、构建和 CTest 日志、`field-layout.json`（每个 section 的实际 offset/count/size）、`report.json`、带两张程序构造 PNG 的 `package/`。报告记录工作区状态、Git/子模块 revision、平台、Core ABI 版本、采样、SHA-256、数值 expected/actual 和误差。`check_line` 对应测试源码中的断言位置；`object` 包含采样和对象 ID。

入口成功只表示静态和参数增量的数据检查通过。报告中完整 M1 和 GPU 项保持 `not_run`。纹理为无外部素材依赖的非对称 8×8 RGBA 用例。此入口发布的是验证用资源包，尚不是接受任意素材的产品导出接口。只在整个增量检查成功后替换上次报告与资源包；失败保留上次产物和本次失败证据。

## 坐标与身份

Document 位置单位为原画像素，X 向右、Y 向下；画布原点从左上角计量。`pixels_per_unit` 必须有限且大于零，默认 1；原点默认 `(0,0)`。

```text
runtime.x = (source.x - origin.x) / pixels_per_unit
runtime.y = (origin.y - source.y) / pixels_per_unit
runtime.uv = (source.u, 1 - source.v)
```

源 UV 的 `(0,0)` 为图片左上角，运行 UV 的 `(0,0)` 为左下角。转换 Y 方向时交换每个三角形的第二、第三个索引。MOC3 canvas flag 写 1，声明位置与索引已经按运行 Y 方向处理，避免 Core 再次翻转。canvas.origin_y 写 `height - source.origin.y`，其他画布数值保持像素单位。两个 Core 对实际返回结果进行检查。

Mesh 的 `runtime_id` 独立于内部 UUID 和显示名称；创建时为空则取内部 UUID。重命名不修改运行 ID。第一增量仅编码 1–63 字节的可打印 ASCII ID，剩余字节补零；超长、控制字符或未验证编码明确失败，不截断。Document 内拒绝重复的 Mesh 运行 ID。未来支持其他对象类型时需要扩展各 ID 命名空间的规则。

`gd-kasane` 原型快照更新为 format_version 4，保存原点、单位、运行 ID、参数、绑定和位置 Keyform。原型版本 1/2/3 没有提供迁移，现明确拒绝；这不构成 M2 工程持久化验收。旧原型 Rotation/Warp 仍在原接口中保留，但当前编码器拒绝任何 deformer 或父子关系，绝不烘焙为静态姿势后宣称支持。

## 布局、容量与默认值

头部 64 字节，magic `MOC3`，版本字节 5，endian 字节 0；随后 160 个小端 uint32 offset。offset 表之后预留零填充的 loader scratch，count_info 从 1984 开始。section 起始统一 64 字节对齐（同时满足 Purism 的 8 字节对齐）；152–159 为保留 offset，写零。不能 dump 原生结构体或 revive 后的运行内存。

count_info 为 64 个小端非负 int32。offset 在磁盘上是 uint32，但本实现限制在 Core 接受的 `INT32_MAX` 以内。位置/UV 为 IEEE float32，三角形索引 uint16；单 Mesh 最多 65536 个顶点。顶点稳定 ID 经过稠密索引映射后才写出。绘制顺序第一增量取 Mesh 遍历顺序，并限制数量保证整数能精确表示为 float32。容量与 ID 错误包含对象或字段，不静默截断。

下表中的索引为零基 section 索引；完整映射见 `.inc`，实际偏移由验收入口输出。

| 数据 | section | count 与默认值 |
|---|---|---|
| count_info / canvas | 0 / 1 | 固定 256 / 24 字节；canvas 末尾 3 字节零填充 |
| Mesh loader 指针槽 | 29–32 | 每 Mesh 各 8 字节零值，留给 Core revive；与本机指针大小无关 |
| Mesh 运行 ID | 33 | 每 Mesh 64 字节 |
| 绑定、形态起点/数量 | 34–36 | 无参数时绑定 0；有参数时按 binding_order 分配；指向全部组合形态 |
| 可见/启用、Part/变形父级 | 37–40 | 1 / 1 / -1 / -1 |
| 纹理槽、标志 | 41–42 | 按 asset_order 稠密分配；4（双面、normal） |
| 顶点、UV、索引窗口 | 43–46 | UV offset/count 以 float 为单位；索引以 uint16 为单位 |
| 遮罩窗口 | 47–48 | offset=0，length=0 |
| Mesh Keyform | 68–70 | opacity=1；draw_order=Mesh 索引；position offset 以 float 为单位 |
| 位置池 | 71 | 每顶点两个 float，count 是 float 总数 |
| 绑定 | 73–74 | 绑定 0 为零轴；后续按轴顺序引用 key_table_idx |
| UV / 索引池 | 78–79 | 每顶点两个 float / 三角形三个 uint16 |
| 根绘制组 | 81–85 | 一个组，覆盖所有 Mesh；min=0，max=N-1 |
| 绘制项 | 86–88 | type=0（Mesh）、idx=Mesh 索引、self_group=-1 |
| Mesh 颜色起点 | 107 | 每 Mesh 对应连续的形态颜色范围 |
| multiply / screen RGB | 108–113 | 分别 `(1,1,1)` / `(0,0,0)` |
| v5 Mesh Keyform 颜色 offset | 141–142 | 分别引用对应颜色池 |
| 其余动态段 | 见 `.inc` | 对应 count=0，section offset 指向当前对齐游标，长度为零 |

测试会根据固定 Purism revision 的宏定义核对 schema，避免新增/调整格式字段后导出器继续静默使用旧映射。每个非空字段的实际字节数必须与 schema/count 完全匹配；只有 loader 指针槽允许编码器自动补零。

## 已覆盖的行为与待补项

静态用例包含两个纹理槽、不对称四边形与三角形、非连续顶点 ID、非零原点和非 1 的单位换算。两个 Core 检查画布、ID、拓扑、UV、纹理索引、位置、绘制/渲染顺序、透明度、颜色、可见性与空遮罩。

编辑回归覆盖重命名不改变 MOC3、修改顶点改变导出结果且另一 Mesh 不变、替换静态拓扑后再次导出。失败回归覆盖不可表示 ID、重复运行 ID、未提交事务、未支持的父子/变形数据、无效画布，以及 Core 对截断头部、未知版本和越界 offset 的拒绝。

普通参数、完整组合位置形态和共享 Purism 插值已实现。参数字段为 section 49–57、102–104、114–116；key_table_idx 为 72，key_table 为 75–76，keys 为 77。Key table 按 parameter_order 分组，Binding 通过索引表保留自己的轴顺序；形态轴 0 最快变化。参数键值枚举为各绑定关键值的去重并集。decimal_places 控制共享关键值搜索的吸附阈值。

每对象实际 key_len 等于组合数；单关键值轴时可能另附不可达的重复形态，满足 Core 的最大 gather span 校验，但不伪造缺失组合。新的构造和编辑用例及剩余范围见核心重构说明。
