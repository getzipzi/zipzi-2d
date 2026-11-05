# DeepSeek 导入与打开：首次优化复测

后续的 viewport GPU 首帧及窗口呈现实测见[原生 Metal 首帧记录](2026-11-05-deepseek-viewport-first-frame.md)。

2026-11-05，Apple M4 / macOS 27.0.1 arm64，Rust 1.99.0，Release 默认 features，
Purism Core 启用。改动基于 `77f1383b`，对照同轮运行的原版二进制及
`v0.1.0-alpha.4`（`d88c70c1`）。源码差异、二进制和输入指纹、逐次数据见
[JSON 记录](2026-11-05-deepseek-project-io-optimization.json)；修复前记录保留在
[原始比较](2026-11-05-deepseek-project-io.md)。

Editor 打开到首次 CPU 预览的中位数降低 **24.0%**，导入降低 **14.7%**；
这两项分别比同轮 alpha4 低 12.6% 和 13.6%。原生工程重存后整个归档的 SHA-256
与原版一致，未改变工程内容或图片。

## 改动

- [blendshapes.rs](../../../crates/zipzi-core/src/document/blendshapes.rs)：新增/替换
  BlendShape Binding 保留轴和采样点身份索引。它们只引用 key table 中的采样点，不定义
  新的轴或采样点；仍执行完整的结构/求值失效和事件发布。DeepSeek 的 1,007 个此类
  Binding 不再各自触发同一身份索引的重建。实际新增/修改 key table、普通轴等继续失效。
- [decode.rs](../../../crates/zipzi-project/src/codec/decode.rs) 和
  [reader.rs](../../../crates/zipzi-project/src/archive/reader.rs)：归档复用 codec 的
  语法与头部校验。project.json 从两次语法检查、两次头部解析减少为各一次；仍先校验
  精确格式/版本再分配完整 wire 数据。format.json 独立校验，重复键、NUL、hash、ZIP
  admission 和共享内存预算检查保留。
- 同一私有 PNG 成功解码后，仅对声明尺寸完全相同的引用复用结果。不同尺寸的引用仍
  单独进行有界校验并产生资源诊断；失败结果不缓存。有效 DeepSeek 工程的打开校验由
  715 次 PNG 解码减少为 490 次，取消检查保留，未批量保留 RGBA。

## 同轮结果

单位秒，每格为 **中位数（最小–最大）**，每版本 5 个独立进程。

| 边界 | alpha4 | 原版 77f1383b | 优化后 | 相比原版 |
| --- | ---: | ---: | ---: | ---: |
| Project model3 导入 | 3.258（3.217–3.662） | 3.404（3.270–3.781） | 3.036（2.801–3.185） | −10.8% |
| Project 打开 | 1.882（1.806–2.070） | 2.210（2.145–3.057） | 1.637（1.486–1.754） | −25.9% |
| Editor 导入到首次 CPU 预览 | 3.549（3.406–3.573） | 3.594（3.490–3.721） | 3.065（2.973–3.435） | −14.7% |
| Editor 打开到首次 CPU 预览 | 2.404（2.344–3.018） | 2.764（2.713–2.954） | 2.101（2.036–2.995） | −24.0% |

保留全部样本，包括优化版打开的 2.995 秒样本。收益是五次中位数的比较，不承诺每次
运行都更快，也不把几个样本外推为稳定的尾延迟或产品 SLA。

## 方法和内容验证

先预热三个版本各一次，再轮换顺序运行五组，共 60 个正式测量进程和 12 个预热进程。
所有版本使用相同的 `project_io_benchmark` 源码、编译器及 Purism Core 版本。
原版与优化版打开同一个保留的 `.zipzi` 文件；alpha4 打开其目录工程。导入均读取
`models/local/DeepSeek/大肥鱼.model3.json` 及相同附件，输入 SHA-256 与原始测量核对。

每个进程使用私有临时/cache/Runtime 目录，清除开发会话和 owner fd。计时前完成编译，
运行保存的二进制副本。未清空 OS 缓存、未隔离 CPU/I/O，机器有普通桌面活动。
本次重新测量原版，避免用上一轮不同系统负载下的绝对时间计算优化幅度。

Editor 总时间为同一进程 admission、worker、publication、首次 CPU preview 之和，
不含应用启动、窗口布局、GPU 上传或屏幕呈现。Project 导入阶段结束后进行的资源检查
不计入导入时间。本轮 Project import 不附带保存，保存往返另作独立正确性验证。

- 所有正式和预热进程成功；每个 Editor 场景均为 715 Mesh、715 Texture，
  RGBA 共 703,790,836 字节。
- 优化版重新导入、保存、打开，完整 document 内容一致；490 份唯一 PNG 的完整
  SHA-256 集合与原版相同。
- 优化版打开原版工程并重存，所有 ZIP 条目逐字节相同，整个文件 SHA-256 也一致：
  `708b8098820da9383cdd43babc71b75f3f0054137d10a7bd062f0977920e4c1e`。
- 原素材的 `UNRESOLVED_PART / Part160` 提示仍保留，未屏蔽资源问题。

这是三项优化的合并结果，没有分别消融，不能把毫秒收益分配给某一项。
PSD 的原有 256 MiB 解码预算问题不在本次改动范围。

## 回归验证

```sh
cargo test --locked -p zipzi-core --test blendshape_glue_tests
cargo test --locked -p zipzi-core --test keyform_tests
cargo test --locked -p zipzi-project --lib archive:: -- --test-threads=1
cargo test --locked -p zipzi-project --test project_tests
```

分别通过 18、1、31、53 项。新增测试验证身份索引共享、旧快照保持、后续新增 key table
更新索引、采样点重名拒绝；归档测试验证根/嵌套重复键、版本/格式预检、NUL、有效引用
前后出现错误尺寸的共享 PNG，以及超限尺寸不能复用已有验证。

首次默认并行 archive 测试为 30 通过、1 失败：未修改的
`abandoned_staging_cleanup_requires_metadata_and_released_owner_lock` 在
`!stale_path.exists()` 断言失败。该项单独重跑通过，完整 archive 串行运行 31 项通过；
未确定该次失败的原因，未修改 publication.rs 或放宽断言。该记录不省略。

完整原始产物位于 `target/deepseek-load-optimized/`，批处理脚本为
`target/python/scratch/deepseek_optimized_compare.py`。原始 `target/deepseek-load/`
测量和二进制保留。未运行完整 SDK/Editor 套件或可见首帧测试。
