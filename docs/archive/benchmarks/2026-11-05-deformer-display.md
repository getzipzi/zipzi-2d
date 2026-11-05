# 2026-11-05 Deformer 显示开关 benchmark

比较画布底部 regular deformer 显示按钮的开/关；全部使用当前 Metal 原生路径和同一个保存的 Release 二进制。
关闭只隐藏未选中的 Warp 外框与 Rotation 骨骼。模型变形求值继续运行，Mesh 选区、拓扑和已选 deformer 控件保留。

本次关闭显示后，无选中对象拖参数的 UI draw P95 降低约 0.26 ms（5.9%）；平移、缩放和全选 Mesh 拖参数的变化都在 1% 内。呈现延迟和新 preview FPS 没有一致改善，结论限于本机、此模型和这几个场景。

代表场景为平移/缩放、无选中对象拖参数、全选可见 Mesh 拖参数。每个场景 4 对新进程，顺序 ON/OFF、OFF/ON、ON/OFF、OFF/ON。
每个模式预热 30 次、正式输入 360 次，目标 120 Hz；24 个接纳进程，共 11,520 次正式输入。详细计时关闭。
Apple M4（10 核 GPU，16 GB），macOS 27.0.1；主屏逻辑 1920×1080 @120 Hz，镜像一个 60 Hz 显示器。
Mao：260 Mesh、128 参数，渲染请求尺寸全部固定为 941×1312 像素。全选场景保留 150 个可见未锁定 Mesh 的 5,159 个顶点、12,202 条边及 2,825 条边界线。

几何审计：ON 为 116 个未选中 Warp 外框、59 个 Rotation 骨骼；OFF 均为 0。两侧的 Mesh 选区、边线数和 authoring history 逐对一致。
应用显示设置本身没有增加模型求值次数或历史；两种参数负载在 ON/OFF 下均验证了实际 Mesh 位置变化。无选中对象且 OFF 时只剩 2 条原点十字线；测量区间内全部命中纹理缓存，未重新提交 overlay draw。

下面使用每个进程 P95 的四轮中位数，单位 ms；变化为 OFF/ON−1，负值表示关闭后耗时降低。

| 场景 | UI draw P95：ON | OFF | 变化 |
| --- | ---: | ---: | ---: |
| 平移 | 4.438 | 4.423 | -0.4% |
| 缩放 | 3.653 | 3.683 | +0.8% |
| 拖参数：无选中对象 | 4.371 | 4.111 | -5.9% |
| 拖参数：全选可见 Mesh | 5.544 | 5.596 | +0.9% |

| 场景 | dispatch → present P95：ON / OFF（ms） | 新 preview FPS：ON / OFF |
| --- | ---: | ---: |
| 平移 | 38.95 / 37.07 | 120.00 / 120.00 |
| 缩放 | 47.86 / 47.97 | 120.00 / 120.00 |
| 拖参数：无选中对象 | 33.58 / 35.32 | 118.16 / 111.82 |
| 拖参数：全选可见 Mesh | 40.03 / 37.32 | 112.68 / 115.86 |

| 场景 | UI draw P95 四轮范围：ON（ms） | OFF（ms） |
| --- | ---: | ---: |
| 平移 | 4.175–4.646 | 4.306–5.928 |
| 缩放 | 3.517–3.775 | 3.541–3.777 |
| 拖参数：无选中对象 | 4.276–4.472 | 3.992–4.275 |
| 拖参数：全选可见 Mesh | 5.230–5.717 | 5.411–5.828 |

UI draw 是整个窗口的 UI 线程布局/绘制墙钟时间。呈现边界从 GPUI 合成事件分发开始，到 compositor 时间戳结束，不包含 OS 输入投递和物理显示响应。
延迟仅统计具备相关边界的请求；合并、取消、无请求和未呈现结果保留在 outcomes，不能把缺失延迟当零。新 preview FPS 与普通窗口重复呈现分别统计。
每秒采样后台进程，接纳进程中未观察到编译器；6 个受编译影响而整对排除的进程保存在 attempts.json。其他后台负载与调度波动仍可能存在。
全部接纳 trace 通过 frame reporter：benchmark_done、唯一 end、lost_events=0、GPU 状态成功、测量输入与 workload 齐全。二进制和全部模型资源前后哈希一致。
二进制 SHA-256：`e8367520dfbd55bafda096432db3391069156e0b3d2c3f1a56d734c0cad3da01`。源 HEAD：`0ec23705ab9c8d59c88c2fc9d5a7f62885a6fde2`，含未提交修改；源 diff 和新增代码已归档。
原始记录、逐轮 P50/P95/P99、outcomes、实例数审计、启动配置和资源哈希：`/Users/liangbohao/web/zipzi-2d-warp-viewport/target/deformer-display-20261105-main`。
复现入口：tools/benchmarks/viewport-ui-benchmark.py --comparison deformers --scenes navigation parameter_unselected parameter_selected；其余本次参数见 run.json。

验证：Release 构建、7 项相关 Python 工具测试和 git diff --check 均通过。
