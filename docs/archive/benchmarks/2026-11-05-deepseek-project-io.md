# DeepSeek 导入与打开：77f1383b 对照 alpha4

本页为修复前基线；后续实现和同轮复测见[首次优化记录](2026-11-05-deepseek-project-io-optimization.md)。

2026-11-05，Apple M4，macOS 27.0.1 arm64，Rust 1.99.0，Release 默认 features。
基线代码为 `77f1383bf08f642333e793d3bfde66446916b17b`，对照 tag
`v0.1.0-alpha.4` 指向 `d88c70c18268342789fea56f109dff4a518d0c15`。
两边使用相同探针及同一版本 Purism Core `ad5127666ccffb34552d8e660852995858bee437`。
完整逐次数据、源码/输入/二进制 SHA-256 见 [JSON](2026-11-05-deepseek-project-io.json)。

打开工程的回退明确：工程层读取中位数增加 51.7%，Editor 到首次 CPU 预览增加
17.3%。model3 导入的工程层中位数只增加 1.7%；Editor 导入增加 7.9%，但范围明显重叠，
本次数据不足以证明一个同样稳定的导入回退。首次保存反而快了 53.9%。

## 结果

每格为 **中位数（最小–最大）**，单位秒。各版本 5 次独立进程，未剔除样本。

| 测量边界 | alpha4 | 77f1383b | 中位数变化 |
| --- | ---: | ---: | ---: |
| Project model3 导入 | 3.963（3.533–4.016） | 4.030（3.653–4.987） | +1.7% |
| Editor model3 导入到首次 CPU 预览 | 3.540（3.391–3.884） | 3.819（3.313–3.990） | +7.9% |
| Project 打开已保存工程 | 1.815（1.763–2.121） | 2.754（2.148–3.631） | +51.7% |
| Editor 打开到首次 CPU 预览 | 2.806（2.336–2.887） | 3.291（2.695–3.549） | +17.3% |
| Project 导入后首次保存 | 2.756（2.668–2.901） | 1.271（1.200–1.543） | −53.9% |

Editor 打开在五个配对中均较慢。worker 中位数为 2.751 → 3.256 秒；
文档发布为 0.274 → 0.333 毫秒，首次 CPU 预览为 30.563 → 34.676 毫秒。
因此本次打开差异集中在加载 worker，不在文档挂接或首次求值。

Editor 导入 worker 为 3.503 → 3.784 秒。每次导入包括 MOC3 解码、附件处理、
图集拆分、SDK 安装、纹理准备和对象索引；不能将这项总时间全部归为 MOC3 解析。
图集拆分在 alpha4 中已经存在。

## 测量方法和内容校验

- 输入为 `models/local/DeepSeek/大肥鱼.model3.json` 及其引用的 MOC3、5 张纹理、
  physics3 和 cdi3。受许可限制的素材及生成工程均未加入 Git。
- 先预热两边一次，再按 BA、AB、BA、AB、BA 顺序运行 5 组；A 为 alpha4，B 为
  `77f1383b`。每组、每版本分别启动 Project import/save、Project open、Editor import、
  Editor open 四个进程，共 40 个正式测量进程，另有 8 个预热进程。
- 构建在计时前完成，运行保存的二进制副本。每进程独立临时目录、cache 和 Runtime
  profile，清除 `ZIPZI_DEV_SESSION_DIR`、`ZIPZI_RUNTIME_OWNER_FD`。未清空 OS 文件缓存。
- 两边采用各自正式工程格式：alpha4 目录工程，`77f1383b` 单文件 ZIP。
  每组各自导入、保存，再在新进程打开该版本生成的工程。
- 每次 Project import/save 均在计时外重新打开并比较完整 document 内容，检查图片资源
  有效性。Editor 均确认非空预览。两边均为 715 Mesh、30 Mesh Binding、715 Image Asset，
  Editor 纹理共 703,790,836 字节 RGBA。
- 模型还含 822 Transform、648 Scene Binding、1,007 BlendShape Binding、509 Parameter。
  715 个资产去重后为 490 份 PNG，总计 41,335,967 字节；对照两边全部 PNG 的完整
  SHA-256 集合相同。alpha4 文档为 48,026,218 字节，候选文档 48,074,976 字节、
  归档 56,242,793 字节。
- 两版均报告原素材已有的 `UNRESOLVED_PART: $.Parts[130].Id: Part160 is absent from
  the model`。该显示信息提示保留在原始结果中；独立图片校验及保存重开通过。

Editor 总时间取同一次输入的 admission、worker、publication、first CPU preview
之和。它不含应用启动、窗口布局、GPU 上传、合成器或屏幕首帧，不能当作可见首帧时间。
机器存在普通桌面活动及另一个 checkout 的任务，未隔离 CPU；保留全部范围，不能把
小幅差异或五次样本解释为稳定的尾延迟。未测 Debug 构建或冷存储读取。

## 热点与后续优化方向

正式对照完成后，另用 `/usr/bin/sample` 对两版 Project import 和候选 Project open
分别采样 3 秒。采样包括计时外图片检查，也可能未覆盖进程结尾；不将采样比例转成
本次回退的毫秒归因。

1. 候选 open 栈中，codec 重建文档、`create_blend_binding` → `contains_id` →
   `binding_identity` → `build_binding_identities` 明显。两版 import 也都有这个热点。
   批量插入 1,007 个 BlendShape Binding 时反复重建身份索引，值得作为共同加载成本优化，
   但它不是已证明的新回退来源。
2. [archive/reader.rs](../../../crates/zipzi-project/src/archive/reader.rs) 的 `json()`
   对 project.json 做一次完整语法检查，`decode_project()` 又检查一次；归档层和 codec
   也分别解析头部。相较 alpha4，新增了扫描和 JSON 解压成本。后续可合并校验入口，
   同时保留重复键、版本、完整性及资源限额检查。
3. 同一读取器按 490 个 PNG 路径分组后，仍为每个资产引用调用
   `read_project_texture_bounded`，715 次解码后丢弃像素；Editor 准备纹理还会再次解码。
   alpha4 的资源诊断可按已验证 PNG hash 复用尺寸检查。
   后续可按内容复用已验证结果，同时逐项检查每个引用声明的尺寸和 hash，并维持共享内存预算。
4. 新归档还需要复制/哈希输入、验证 ZIP 索引、展开资源和写入私有工作集。
   本次粗粒度计时不能把上述成本与文档存储结构变化分别归因；尚未做逐项消融或修复对照。

打开变慢的同时，进程峰值 RSS 降低：Project open 中位数约 599.4 → 298.9 MiB，
Editor open 约 1,442.1 → 1,100.9 MiB。来自 `/usr/bin/time -l` 的整个进程峰值，
包含探针设置、检查和退出，不是某阶段的净分配；不能简单用内存换时间后宣称整体改进。

## PSD 补充检查

另对同目录 `大肥鱼.psd` 各运行一次 Editor PSD 导入，两版均失败于相同错误：
`RESOURCE_BUDGET_EXCEEDED: Exceeded memory limit: needed 38136704 bytes,
26679532 bytes left in budget`。
`import_psd_in_memory_authoring` 的解码预算封顶为 256 MiB。本次不调整该产品限额，
也不将失败退出时间计为成功 PSD 导入耗时。上述主表全部针对 model3/MOC3。

## 复现与产物

共用探针为 [project_io_benchmark.rs](../../../crates/zipzi-editor-core/examples/project_io_benchmark.rs)，
使用方法见 [benchmark guide](../../../tools/benchmarks/README.md#project-import-and-open)。
把同一文件复制到两个独立 checkout 后分别构建；生产逻辑未作修改。

本机保存：`target/deepseek-load/` 下的二进制、逐次 JSONL、time 输出、生成工程、
`metadata.json`、`samples.json`、`summary.json`、`artifact-verification.json` 及三份采样栈。
批处理脚本为 `target/python/scratch/deepseek_compare.py`。PSD 失败和最初探针把显示信息
提示当成图片错误的预跑记录单独保存，均不进入正式性能统计。

验证：两边 Release 探针构建成功；正式 48 个进程含预热全部成功，12 次保存重开一致；
490 份 PNG 完整 hash 集合相等；`cargo_domain.py --check`、探针 rustfmt 和 `git diff --check`
通过。未运行全 SDK/Editor 套件；本轮没有修改生产代码。
