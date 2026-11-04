# Zipzi 原生工程 `.zipzi` 设计方案

状态：P0–P3 实现已落地，P4 本地工程、故障、macOS 指定应用事件/原生对话框及 Finder 默认入口冷启动已验证；Finder 无窗口/鼠标双击和 Windows 原生交互待验收。日期：2026-11-04。调研基线：`f59db0f0`。

## 1. 建议决策

将 `.zipzi` 定义为跨平台、可独立携带的单文件原生工程，底层采用标准 ZIP 容器，
内部保存 JSON 文档和工程资源。Editor 与 Rust/Python SDK 使用同一条读写路径。
程序打开工程后，使用私有工作目录读取资源；工作目录由程序管理，不属于交付物。

用户操作的对象始终是 `角色.zipzi`。复制该文件即可带走已经纳入工程的有效资源；
打开不依赖原始 PSD、导入目录或上一次运行留下的缓存。模型和动画继续属于同一工程，
不按工作区增加扩展名。运行时模型、CMO3/CAN3 等继续通过导出功能产生。

当前尚未正式发布，按仓库约定直接切换正式保存格式，不开发历史目录工程迁移器，
不长期维护两套同名 `.zipzi` 保存后端。内部 codec 测试和诊断工具可以读写裸 JSON，
但裸 JSON 和目录不是用户侧工程入口。

本提案选择单文件，是为了统一 macOS 与 Windows 的工程操作方式。仅正式命名现有目录
仍是成本更低的选择，但无法同时完成 Windows 文件关联和单文件交付。本次实施范围
应按“原生工程容器与生命周期改造”估算，不能按一次后缀重命名估算。

## 2. 当前实现与改造依据

| 当前行为 | 实现入口 | 对方案的影响 |
| --- | --- | --- |
| 保存建议名为 `名称.zipzi`，实际写目录 | [Editor 保存对话框](../../crates/zipzi-editor/src/project.rs)、[manifest 路径解析](../../crates/zipzi-project/src/store/manifest.rs) | 用户路径需要从目录/manifest 切换为真实文件 |
| JSON 标识为 `zipzi-directory-project`，版本 21 | [codec](../../crates/zipzi-project/src/codec/encode.rs)、[crate 说明](../../crates/zipzi-project/README.md) | 复用文档字段和校验，区分容器版本与文档版本 |
| 先写内容寻址 PNG，再原子发布 manifest；校验旧 manifest SHA-256 | [保存实现](../../crates/zipzi-project/src/store/manifest.rs) | 新格式必须保留发布边界与冲突检测 |
| 项目锁是目录内永久保留的 `.zipzi.lock` | [文件系统原语](../../crates/zipzi-project/src/filesystem.rs) | 文件原子替换后，写锁仍需拥有稳定身份 |
| `DocumentSession::root()` 从 manifest 父目录推导 | [DocumentSession](../../crates/zipzi-project/src/store/session.rs) | 工程路径与资源根目录必须分离 |
| 保存会重定位 undo/redo 资源，历史可能引用旧目录或外部文件 | [SDK 保存](../../crates/zipzi-sdk/src/project_io.rs)、[历史资源重定位](../../crates/zipzi-sdk/src/assets.rs) | 不能在另存为或清理缓存时使历史资源失效 |
| AuthoringSnapshot 和 ObservationInput 等保留资源路径 | [SDK 快照](../../crates/zipzi-sdk/src/session.rs)、[观察输入](../../crates/zipzi-observe/src/lib.rs) | 目录清理需要显式资源租约，不能只看编辑器标签是否关闭 |
| 导出快照按资源根目录、导入路径保护源文件 | [transfer](../../crates/zipzi-project/src/transfer.rs)、[导出计划](../../crates/zipzi-project/src/package/plan.rs) | 分离缓存后仍须显式保护原始 `.zipzi` 文件 |
| 第二个 Editor 进程只发送 reopen，当前启动逻辑没有工程参数分发 | [启动](../../crates/zipzi-editor/src/lib.rs)、[IPC](../../crates/zipzi-editor/src/ipc.rs) | 文件关联必须同时补齐冷启动、常驻进程和启动中队列 |
| macOS plist 未声明工程类型；Windows 发布物是便携 ZIP | [发布元数据](../../tools/packaging/release.py)、[Windows 打包](../../tools/packaging/package_windows.py) | 关联注册需要独立的平台接入工作 |

## 3. 方案取舍与首版边界

| 形态 | 优点 | 成本/限制 | 决策 |
| --- | --- | --- | --- |
| 继续使用 `.zipzi` 目录，macOS 注册 package | 最接近现有实现，资源无需重复写入 | Windows 仍是目录；分享需另打包；平台体验不同 | 不作为目标格式 |
| 一个 `.zipzi` 描述文件加相邻 `assets/` | 文件关联直接；保存较轻 | 移动描述文件容易遗漏资源；工程边界分散 | 不采用 |
| 标准 ZIP 单文件 | 跨平台一致；可用通用工具检查；复用 JSON/PNG | 首版完整保存需要重写整个容器；需要临时磁盘空间 | 采用 |
| SQLite 或自定义二进制容器 | 有机会实现页级更新或定制随机访问 | 增加格式与存储实现成本；收益尚未测量 | 暂不采用 |

首版包含：完整工程读写、保存/另存为、资源完整性、冲突保护、资源生命周期、
取消与失败语义、Editor 文件入口、系统关联和跨平台验收。

自动保存、崩溃后恢复未保存编辑、持久化撤销历史、增量 ZIP、跨进程共享缓存、
云同步冲突合并、Finder/Explorer 缩略图插件均为后续独立事项。
原子保存保护的是上一次成功保存的工程，不等同于恢复尚未保存的编辑。

## 4. 容器协议

### 4.1 文件结构

```text
角色.zipzi                 # 普通文件，标准 ZIP；无自定义前导字节
├── format.json
├── project.json
└── assets/
    ├── <64 位小写 SHA-256>.png
    └── ...
```

`format.json` 首版结构：

```json
{
  "format": "zipzi-project-package",
  "container_version": 1,
  "document": "project.json",
  "document_sha256": "<project.json 解压后原始字节的 SHA-256>",
  "required_features": []
}
```

- `format.json` 和 `project.json` 各有且仅有一个，路径固定；不能通过字段指定任意入口。
- JSON 使用 UTF-8。容器版本描述打包规则，文档版本描述 authoring 数据结构，分别检查。
- `project.json` 复用现有文档结构，将标识改为 `zipzi-project`，文档版本拟递增至 22。
  实施时若主线已增加版本，使用当时的下一个版本；不重编号已存在的文档版本。
- 有效图片内容的 `source` 只能是 `assets/<完整 SHA-256>.png`，同时保留现有尺寸与
  SHA-256 字段；相同编码字节只存一次。使用完整 hash，去掉目录格式的短前缀冲突处理。
- 现有 motion、expression、physics、pose、opaque 数据和 package attachment
  继续按当前 codec 保存。实施前清点所有资源类型，确认没有遗漏的文件依赖；
  不能把所有需要携带的内容简单等同于画布上可见的图片。
- 所有当前文档持有的有效资源均纳入工程，包括暂未被 Mesh 使用的 asset；
  仅存在于 undo/redo 的资源留在工作集，不进入保存文件。
- 原始 PSD/CMO3 等导入文件不自动作为附件加入。来源路径可以是诊断信息，
  但打开和渲染工程不能依赖它，也不能据此自动访问外部文件。
- 不写入窗口布局、选择、缓存、机器绝对路径、锁文件、临时文件或 SDK 撤销栈。

### 4.2 ZIP 约束

使用 Stored 和 Deflate 两种压缩方法；写入端对 JSON 使用普通 Deflate，对已经压缩的
PNG 使用 Stored。固定条目顺序与时间戳，降低无关差异；不把跨压缩库版本的字节一致性
作为 authoring 内容相等的条件。支持 ZIP64，运行时资源限额独立于 ZIP64 能表示的大小。

首版只接受协议列出的文件；可容忍一个空的 `assets/` 目录条目。不保留未知 ZIP 条目，
而是在打开阶段报告不支持，避免保存后静默丢失数据。未来增加附件类型或预览图时，
使用明确的容器版本/required feature 规则；未知必要能力拒绝打开。

禁止重复路径、大小写碰撞、绝对路径、`..`、反斜杠、盘符、NUL、符号链接、特殊文件、
加密条目、分卷文件和不支持的压缩方法。校验本地头与中央目录的一致性、数据区边界与
重叠、CRC 和实际读取长度。只把识别出的资源流写到程序构造的固定目标路径，
不调用“把整个 ZIP 按成员名解压到磁盘”的便利接口。

### 4.3 识别、版本与资源诊断

打开时以 ZIP 结构、`format.json` 和文档标识共同识别工程；后缀是用户入口约定，
不能代替内容校验。通用 ZIP 改名为 `.zipzi` 应得到“不是有效的 Zipzi 工程”。
未知容器版本、未知文档版本或必要能力给出独立错误，避免统一提示文件损坏。

manifest 缺失、重复、hash 错误或文档结构非法时，整个打开失败，原 Session 不变。
图片缺失、hash/CRC 错误、尺寸非法等沿用当前资源诊断思路：允许打开可修复的文档，
不发布错误图片到工作集。严重的容器结构或限额错误仍拒绝整个打开。

`content: None` 等已有缺失资源表示可被保存，并在诊断中保留；有有效内容声明却找不到
正确字节的资源会阻止保存，直到用户替换或明确移除该内容。不能在保存时自动把损坏资源
变成缺失资源。自包含保证覆盖全部有效资源，不承诺补回导入前已经缺失的素材。

## 5. 内部存储状态与资源生命周期

将当前 `manifest.parent()` 隐含承担的职责拆开，拟引入下列内部概念：

| 概念 | 含义与所有者 |
| --- | --- |
| `project_path: Option<PathBuf>` | 用户选择的 `.zipzi` 文件；未保存草稿为 None |
| `ProjectFingerprint` | 保存基线：完整归档字节的 SHA-256 和长度；与 authoring Version 分离 |
| `ProjectWorkingSet` | 资源工作集：私有绝对目录、内容寻址资源、磁盘预算与清理所有权 |
| `ProjectResourceLease` | 对工作集的强引用，供 Session、History、快照及后台任务持有 |
| `ProjectIoLimits` | 容器、解压、文档和磁盘占用限制；由宿主配置、项目层执行 |

这些是职责边界，最终 Rust 类型名可以调整。`zipzi-core::Document` 不持有缓存目录或
OS 文件句柄；容器、资源与发布逻辑集中在 `zipzi-project`，SDK 管理历史与快照的租约。
现有 `package/` 表示运行时导出，原生工程容器使用独立 `archive/` 模块，避免混淆。

```text
用户 .zipzi 文件 ──复制/校验/展开──► 私有 ProjectWorkingSet
                                          │
                         Session / History / Snapshot / Job 租约
                                          │
保存候选文档 + 当前资源 ──► 同目录新归档 ──原子发布──► 用户 .zipzi 文件
```

首版使用每个工作集独立的私有目录，不做跨进程去重缓存。Editor 由平台缓存目录提供
工作空间；Local SDK 可配置目录，默认使用自己拥有的临时目录。位置选择在宿主层，
`zipzi-project` 不依赖 `zipzi-editor-core`。测试注入临时路径。

工作集内有效文件按内容 hash 不可变存放，只能添加新内容，不能原地覆盖旧资源。
保存和另存为保持同一工作集，避免因用户文件换位置而重定位所有资源。
首版在最后一个租约释放时整体删除工作集，不做会误删历史资源的会话内逐文件 GC；
持续替换大图片会增加磁盘使用，须纳入工作集预算并给出可操作的超额提示。

资源租约必须覆盖：SDK undo/redo、AuthoringSnapshot、编辑候选、冻结的导出输入、
ObservationInput、异步纹理准备和 Runtime Job。已经捕获全部资源字节的对象可以提前
释放目录租约，但仅有 `PathBuf` 的对象不能释放。关闭标签、detach 客户端和结束 Session
不是同一个生命周期事件，清理不能绑定到 UI 标签关闭。

保存前，把当前文档与 undo/redo 仍依赖的外部磁盘资源准备为稳定的工作集绑定；
已有内存图片继续保留其资源租约。准备历史绑定与诊断在发布前完成，失败时不修改活动
文档或历史。已知 SHA-256 必须验证；无历史 hash 时不能声称验证了过去的内容，
保留现有 `history_warnings` 语义。打开归档得到的历史资源天然来自其私有工作集。

后台快照继续保持自身的资源视图。另存为 A → B 后，即使删除 A，当前 Session 的
undo/redo 仍应有效；关闭 Session 后，仍在执行的归档来源快照/任务也应继续有效。
首版不扩展“裸外部文件描述符”在尚未捕获字节前的现有快照保证。

## 6. 打开流程

1. 校验宿主路径，规范化工程身份，拒绝目录和最终路径上的符号链接；检查同一宿主
   是否已打开该文件。复制文件得到的新路径可作为另一个工程打开，即便文档 UUID 相同。
2. 打开源文件，以有界缓冲复制到新的私有候选目录，并计算归档 fingerprint。
   使用同一个文件句柄读取源；Zipzi 写入端只做原子替换，避免读取过程中混合两个版本。
   打开只读位置不需要在源目录创建锁文件。
3. 在创建 ZIP 索引前检查文件长度、中央目录长度和条目数量上限；验证结构，
   有界读取 `format.json` 与 `project.json`，检查版本、文档 hash、结构和资源声明。
4. 从私有归档副本流式提取所需的有效资源，校验实际输出长度、CRC、SHA-256 和图片
   头信息，再发布到工作集。图片解码继续使用现有 PNG 与观察预算。
5. 全部候选状态准备完成后，原子替换 Session 的 Document、工作集、工程路径和基线；
   成功打开才重置历史/增加相应 generation，失败或取消保留原 Session。
6. 删除不再需要的私有归档副本，保留工作集。最近工程和 UI 只记录用户 `.zipzi` 路径。

源文件在复制期间出现可检测的原地修改时应失败重试；任意不遵守协议的外部写入者
无法获得与 Zipzi 协作写入相同的一致性保证。打开完成后源文件被外部替换，
不影响当前私有工作集，下一次保存通过 fingerprint 检测冲突。

首版选择先复制再展开，便于获得独立且可重复校验的输入，代价是打开期间临时占用
“一个归档副本 + 展开资源”。后续只有在测量显示必要时，才考虑省略副本或懒读取。

## 7. 保存、冲突与失败语义

### 7.1 保存事务

1. 通过现有 Runtime/SDK 保存调度冻结文档版本及资源租约，沿用同一 Session 的编辑互斥。
   首版不新增边保存边继续修改同一 Session 的版本合并机制，其他 Session 可继续工作。
2. 解析目标文件身份，取得该路径的协作写锁。确认新建目标不存在，或现有目标的
   完整归档 fingerprint 等于打开/上次保存/明确替换时捕获的基线。
3. 预先准备当前资源、历史资源绑定、编码后的候选文档和保存后状态；为必要磁盘空间
   做预算检查。当前文档仅包含相对的内容寻址路径。
4. 在目标父目录下创建唯一、排他的新 staging 文件，流式写入完整 ZIP，并完成中央目录。
   staging 与目标必须位于同一文件系统。PNG 直接复制，不因每次保存重新编码。
5. 完成文件同步、必要结构检查和 fingerprint 计算。ZIP writer 可能回写头部，因此
   hash 必须针对 finalize 后的实际文件，不能直接使用一次顺序写入回调累计的 hash。
6. 再次检查目标基线及取消状态，然后发布：新建必须使用原子 no-replace 操作；
   覆盖使用经平台测试的原子替换操作。不得先删除旧文件再重命名新文件。
7. 发布成功即安装已准备的工程路径、保存基线和历史绑定，随后同步父目录并汇总警告。
   发布后的状态切换不应再依赖磁盘分配或可失败的资源读取。

现有 `FileSystem::write_new(&[u8])` 不适合容纳整个归档，需要增加可注入故障的流式
staging writer、sync 和 publish 接口，并显式区分 no-replace 与 replace。
不能只把 `encode_project` 的结果外面包一个 ZIP 后继续沿用 manifest 写入接口。

### 7.2 结果与取消

| 发生位置 | 目标文件 | 内存状态与返回 |
| --- | --- | --- |
| 编码、复制、校验、空间不足、同步或发布前取消 | 旧文件保持原样；新目标仍不存在 | 保留 dirty、工程路径、基线和历史；返回失败/取消 |
| 原子发布失败，包括 Windows 文件占用 | 旧文件保持原样 | 失败，可重试或另存为 |
| 发布成功，父目录同步失败/平台不支持 | 新文件已经发布 | 正常保存回执，`durable=false` 并带警告；不得伪装成未保存 |
| 发布成功后才收到取消 | 新文件已经发布 | 返回已发布结果；不能报告“已取消且未保存” |

继续区分 `published` 与 `durable`。Windows 当前缺少目录同步保证的事实不会因为
换为 ZIP 消失，不新增“断电后必定保留”的承诺。进程崩溃前尚未发布的 staging 不作为
正式工程或自动恢复候选；正式目标只可能是上一个版本或已经完成发布的新版本。

### 7.3 写锁与外部修改

使用目标旁稳定的 `.<文件名>.lock` 作为协作锁，例如 `.角色.zipzi.lock`。
它不是工程依赖，也不随工程分享；采用现有 Unix 文件锁/Windows 独占句柄原则，
持锁期间及正常释放后都不删除锁文件，避免不同进程锁住不同 inode。
锁名由规范化后的目标路径导出，并测试大小写别名、Unicode、不同父目录与路径别名。

同一文件在两个独立进程中打开后，A 保存成功，B 保存必须返回 `PROJECT_CONFLICT`。
锁保护 Zipzi 协作写入；hash 检测协作外的文件变化。普通文件系统没有跨平台的
“按旧内容 hash 比较并替换”操作，最终校验与替换之间对任意外部写入者仍存在竞态，
不能宣称强 CAS 或跨机器云盘事务。

创建新文件必须使用原子 no-replace 以保护竞争中新出现的目标。现有目标被删除也视为
冲突，不自动重建；用户可明确另存为。最终符号链接与硬链接工程首版不作为可写入口，
给出明确路径错误，避免替换语义与工程身份不一致；能力在两平台逐项验收。

fingerprint 校验整个文件，不能只比对内部 JSON，否则外部只替换图片可能逃过冲突检查。
UI 捕获覆盖目标基线、SDK save-replacing 与 worker 最终验证必须共用项目层 API。

## 8. 限额、性能与清理

`ProjectIoLimits` 首版建议默认值如下，属于待 P0 真实样本核验的产品限额，
不是从现有工程测得的规模。宿主可以显式调节，并同时受自身剩余预算约束。

| 项目 | 首版候选默认值 |
| --- | --- |
| 归档输入大小 | 16 GiB |
| 中央目录字节 / 条目数 | 64 MiB / 100,000 |
| `format.json` 解压大小 | 64 KiB |
| `project.json` 解压大小 | 256 MiB |
| 单次打开累计展开字节 | 16 GiB |
| 流式复制缓冲 | 1 MiB |
| 单 PNG 编码/解码空间 | 沿用现有 PNG 限额及宿主更严格限制 |
| 工作集与并发 Job 磁盘预算 | 宿主统一配置并预留；不能每个任务各自假设磁盘无限 |

上述上限都要检查 ZIP 元数据和真实读出字节，所有长度运算检查溢出。
内存限制必须包含索引、JSON 输入、解析中间态、Document 和并发任务，不把
“ZIP 流式复制”误写成“整个打开流程固定内存”。现有 JSON codec 的完整解码开销需要
单独测量并纳入 admission；图片不能在保存时为打包目的批量解码成 RGBA。

保存复杂度为 O(当前工程资源字节 + 文档字节)，还包含目标冲突 hash 和新文件 hash 的
顺序读取成本。磁盘峰值包括旧目标、完整 staging、新增工作集资源；打开还包含源副本。
可用空间预检查只是早期提示，所有写入仍须正确处理 ENOSPC 和部分写入。

文件复制与 hash 每 1 MiB 检查取消；JSON 编解码、ZIP finalize 等不可中断阶段单独记录
耗时，不承诺所有阶段都能瞬间取消。Editor 显示“准备资源 / 写入工程 / 完成保存”进度；
有已知字节总量时报告真实进度，后台执行，不在 UI 线程做归档或 hash。

工作集父目录只清理由本功能创建、具有有效元数据且确认无人持有的子目录。
用 owner lock 区分活跃与遗留，不能只凭 PID 或创建时间判断；清理路径必须仍在配置根目录下。
同目录 staging 仅清理能确认属于自身的遗留文件，不扫描删除其他应用临时文件。
无法清理作为诊断处理，不撤销已经完成的保存。

## 9. Editor 与系统入口

### 9.1 文件菜单

- 打开选择文件，显示类型“Zipzi 工程 (*.zipzi)”；不再把目录和内部 JSON 当正式工程。
  核实 GPUI 的过滤能力，若 API 缺失则补平台封装；内容解析始终是最终判定。
- 保存建议 `名称.zipzi`。UI 对无后缀名称追加一次 `.zipzi`，接受 `.ZIPZI` 的大小写变体；
  输入其他后缀时提供明确纠正，不静默生成另一个文件名。SDK 对显式路径不偷偷追加后缀。
- 文件类型规范化必须发生在覆盖确认之前，确保用户确认的文件正是最终写入目标。
- 标题显示去掉末尾一个 `.zipzi` 的名称，最近工程记录归档路径；默认另存为位置为归档
  所在目录。保存状态显示 `角色.zipzi`，不泄露内部缓存路径。
- 外部修改冲突提供“重新打开 / 另存为 / 取消”；重新打开涉及 dirty 文档时走现有确认流程。
  不提供绕过当前目标基线的无条件覆盖。

### 9.2 macOS / Windows

macOS 在正式应用 plist 注册 `dev.zipzi.project`、扩展名 `zipzi`，类型继承
`public.zip-archive`，声明 Editor 角色与工程图标。它是普通 ZIP 文件，不声明为目录 package。
开发包装与发布包装的生成逻辑需一致，同时避免开发应用抢占正式应用的默认关联。

Windows 便携发布形式可以保留。增加按用户注册/卸载文件关联的入口，使用
`HKCU\Software\Classes` 下的 ProgID（建议 `Zipzi.Project`）、图标和正确引用的打开命令，
例如 `"C:\...\zipzi-editor.exe" -- "%1"`。注册可用于“打开方式”，默认应用选择遵循系统规则；
不依赖用户修改系统默认关联保护字段。移动便携应用后应允许重新注册。

## 10. 启动、IPC 与同工程去重

在现有 endpoint claim 之前解析 `args_os()`，支持 `--` 后的一个或多个工程路径。
路径与未知启动选项分别处理，中文、空格、前导 `-` 的文件名均须覆盖。

把菜单打开、启动参数和 macOS open-document 事件送入同一批量打开队列：

1. 冷启动先缓冲请求，Runtime 与 DesktopHost 就绪后顺序调度。
2. 已有进程时，启动进程经现有私有 IPC 发送有界的 OpenProjects 请求和路径列表，
   不再只发 reopen；驻留的无窗口 host 创建窗口后处理请求。
3. 一个文件已在同一 Runtime 打开时激活已有文档，避免重复 Session；一批请求中失败的
   文件单独报告，其他有效请求继续。请求不能被 UI 初始化或单次弹窗覆盖掉。
4. 队列容量、路径数量与载荷长度设限，复用现有 endpoint 身份校验。接收确认表示已入队，
   打开成功仍由实际 Job 结果决定。

不要实现第二套全局单实例服务；复用既有宿主所有权与本地 IPC。Windows 命令行关联、
macOS 文件事件和现有 `--no-window`/开发启动参数需要共同验收。

## 11. SDK、Runtime 与其他消费端

| 边界 | 拟变更 |
| --- | --- |
| Project 层 | 增加 archive reader/writer、working set 与 lease；分离 `project_path()` / `asset_root()`；替换 manifest 专属基线 |
| Rust SDK | `open_project` / `save_project` 接受归档路径；`SaveReceipt.manifest` 改为 `path`；save-replacing 使用明确的归档 fingerprint 类型 |
| Contract | 新旧字段语义一次切换，更新注释、序列化形状及受影响版本；不留下值实际是归档却仍叫 manifest 的公开字段 |
| Runtime | 工程 registry/reservation 使用归档身份；路径与快照租约随 Job 生命周期流转；预算、取消与 publication 状态保持一致 |
| Python | 从 Rust 契约重新生成 schema/types/docs，更新 save Job 回执映射和 Local/Connected 测试，禁止手改生成文件 |
| Observe / Render | 需要磁盘资源的冻结输入持有租约；现有 root 参数可以内部保留，但不能指向用户归档的父目录 |
| Export / Transfer | 单独记录原始工程文件与导入源的 protected paths，不能因为 source root 换成缓存而允许导出覆盖工程 |
| Fixtures / Benchmarks / Tools | 原生工程样本与路径断言切换为归档；低层 JSON codec 样本继续独立存在；更新模型构建与性能脚本 |

Authoring `Version`、Runtime Session 身份、文档 UUID、磁盘 fingerprint 是不同概念。
另存为保留 authoring 文档身份与历史，磁盘基线变为新归档；不能按文档 UUID 合并两份文件，
也不能把保存引起的容器字节差异当成用户编辑。

首次实施应盘点全部 `manifest` / `root` / `asset_root` / `project_manifest` 消费者。
实验 CBOR codec 可以继续用于性能实验，但不增加正式文件后缀或第二种默认容器内容。
旧目录路径应明确报“不支持的目录工程”，不得将已有同名目录删除后改写为文件。

## 12. 实施顺序与验收门槛

各阶段是依赖顺序；每阶段完成后补记录实际改动、验证命令与结果。

### P0：存储原型和成本确认

- 用现有 codec 及真实小/中/大型样本做 ZIP64、Stored PNG、Deflate JSON 的原型。
  检查现有 lockfile 的 ZIP 库能否满足流式写入、限额、结构校验与授权要求，再声明直接依赖。
- 记录目录工程基线与新格式的打开、普通保存、另存为、峰值 RSS、读写字节和临时磁盘峰值。
  至少覆盖无图片、许多小图片、大纹理、重网格/关键形、已缺失资源五类数据。
- 用两平台原型确认 no-replace、原子替换、占用文件、Unicode 和文件锁行为。
- 核验平台文件打开回调、GPUI 过滤与 IPC 扩展入口。

退出条件：资源逐字节一致，解码后 authoring 内容一致；无整包内存缓冲或批量 RGBA 解码；
记录所有样本的可复现指标，给出明确保存耗时预算和限额。当前没有这些测量结果，
不承诺保存速度；性能不满足产品预算时在此阶段重新评估存储形态。

### P1：Project 层容器与发布

实现格式、严格读取、有界复制、工作集、fingerprint、流式 staging 和发布原语。
保留错误码区分，覆盖损坏输入与发布故障注入。读取失败不修改原 Session，
发布失败不改变目标；post-publish 警告准确报告。

起始验证：`cargo test --locked -p zipzi-project --test project_tests`，以及该 crate
新增 archive/working-set 模块和 `filesystem::tests` 的聚焦测试。

### P2：SDK / Runtime / Python 生命周期

接入 Session、History、Snapshot、Observation 与 Job 租约，调整 receipt 和基线契约，
恢复资源与源文件覆盖保护，更新生成物与受影响样本。

起始验证：`cargo test --locked -p zipzi-sdk --test project_io`；Runtime 的工程所有权、
save Job、取消与预算测试按实际模块过滤；`cargo test --locked -p zipzi-contract`；
`uv run --locked python tools/test_tools.py -p test_python_codegen.py`。
Python 路径和回执改动完成后运行 `./tools/test-sdk.sh python` 与 `python-connected` 模式。

### P3：Editor 文件体验与平台关联

统一打开/保存对话框、标题、最近工程、覆盖确认与状态显示；增加启动参数、打开队列和
IPC 转发；接入 macOS 文档类型与 Windows 用户级注册。

起始验证：Editor `fast` / `ui` 中对应工程 I/O、菜单、host 生命周期的聚焦测试；
`uv run --locked python tools/test_tools.py -p test_release.py` 与
`uv run --locked python tools/test_tools.py -p test_package_windows.py`，再执行新增关联测试。

### P4：真实工程和故障验收

| 场景 | 必须满足的结果 |
| --- | --- |
| 保存后移动文件，删除原始素材和旧缓存，再在新进程打开 | 有效资源、模型和动画完整可用 |
| 打开 → 编辑 → 保存 → 关闭 → 打开 | authoring 内容与保存结果一致，最近工程指向归档 |
| 保存/另存为后 undo/redo，包括图片替换、删除 | 各历史版本资源正确；删除旧归档不破坏当前历史 |
| Session 关闭或重新打开其他工程，旧快照/观察 Job 仍运行 | 任务所需工作集不被提前清理；最后租约释放后回收 |
| 两进程打开同一工程，顺序保存 | 第二个过时保存冲突；两个不同工程不会互相误锁 |
| 外部只修改归档图片或删除/替换文件 | 全文件基线发现冲突；缓存不会伪装成磁盘基线 |
| 写入、sync、finalize、rename、父目录 sync 注入失败 | 每个阶段符合第 7 节发布语义 |
| 竞争中新建目标、同名目录、符号链接、硬链接、文件占用 | 不覆盖未经确认的新对象，不误删目录，报告明确错误 |
| 截断 ZIP、重复条目、超限、路径逃逸、未知版本/能力 | 有界失败；旧 Session 与源文件不变 |
| 缺失/损坏单个图片 | 可修复诊断准确；不能保存出冒充完整的工程 |
| macOS / Windows，关闭、运行中、无窗口、启动中各状态双击 | 文件正确进入同一打开流程；同文件激活已有文档 |
| 中文/空格路径、多文件打开、覆盖取消与任务取消 | 路径不丢失，不误打开临时文件，不把已发布结果报作取消 |
| 导出目标与原始 `.zipzi` 文件冲突 | 源工程受到保护，缓存抽象不绕过保护 |

最终以两平台实际构建和原生交互验收为准；单个平台的逻辑测试通过不能替代另一平台的
文件关联、原子替换或占用句柄验证。保存同一工程的测试应同时核对资源字节和完整
authoring 数据，不能只验证“能解开 ZIP”或“显示了成功提示”。

## 13. 本次设计交付与下一步

原生归档、生命周期、契约及 Editor 平台入口已按 P0 → P3 实现。
P4 已加入真实中大型工程、跨进程移动/冲突、快照观察、故障及预算验证，详见下方记录。
macOS 指定应用的 LaunchServices 和原生保存对话框已追加实机验证。
Finder 默认入口冷启动已补测；无窗口场景尚有失败，鼠标双击和 Windows 原生交互仍待验收。
按用户要求在此收尾，保留未完成项，不宣称跨平台退出条件全部满足。


## 14. 实施记录

### P0 原型：`1547069e`

已测量五类合成样本和 24 Mesh 的真实 cat 工程，详见
[可复现基准记录](../archive/benchmarks/2026-11-04-native-project-format-p0.md)。
该记录是原型成本测量，并未满足真实中大型样本、产品保存耗时预算以及 Windows
文件原语的完整 P0 验收；这些仍列入最终验收，不以 macOS 结果代替。

### P1 Project 层（2026-11-04）

- 增加正式容器 v1 / 文档 v22 读写 API；生产读取拒绝 P0 实验标记。
- 在 ZIP 索引分配前检查中央目录字节与条目数，检查 ZIP64、local/central 头、descriptor、
  CRC/长度、路径、条目类型、数据区边界及重叠。未知能力与版本保留独立状态码。
- 打开先复制、hash，再从私有副本读取；缺失/损坏图片产生诊断，错误文件不会进入工作集。
- 工作集由资源租约持有，包含共享磁盘预算、owner 元数据和锁；仅回收确认无人持有的
  自有遗留目录/staging。保存直接复制 PNG，输出限额贯穿 ZIP finalize。
- 流式 staging 同目录发布；新建使用 no-replace，替换必须有完整 fingerprint；
  发布前失败保持目标，发布后取消/目录同步警告保持 published 语义。
- Session、History、SDK 等用户入口在 P2 统一切换；本阶段没有将旧目录工程当作新归档打开。

验证：Project lib（含 archive、working set、filesystem）及 `project_tests`；
`cargo check --locked -p zipzi-project --features archive-prototype --examples`；
`uv run --locked python tools/workspace/cargo_domain.py --check`。
故障测试包含 write/finalize/sync/publish、no-replace 竞争、基线重检和发布后取消。
Windows 原生文件占用、系统关联和真实交互尚待对应平台验收。

### P2 SDK / Runtime / Python（2026-11-04）

- Session 正式入口统一使用归档；工程路径、完整 fingerprint、私有资源根分离。
- 保存前流式绑定当前及全部历史磁盘资源，保持同一工作集；历史缺少旧 hash 时保留警告。
- History、AuthoringSnapshot、Edit 候选与 ObservationInput 持有工作集租约；导出保护显式包含原归档。
- Runtime 共享磁盘预算并允许宿主配置归档存储；取消贯穿资源复制与归档读写，发布后结果保持成功。
- SaveReceipt/PsdImportReceipt 使用 `path`；明确替换使用完整 fingerprint。协议升级为 v3，Python 从契约生成。
- 目录/裸 JSON 用户入口已移除；P0/CBOR 目录能力仅保留在显式实验 feature。

验证：Project lib 48 项、project_tests 51 项；SDK integration 的 project_io 21 项（含候选/快照最后租约清理、删除外部素材与旧归档后的历史）；Runtime host 5 项及 runtime 30 项；Contract；Python 111 项行为、16 项 recovery、8 项 mesh 与正反静态类型；安装后 Connected 21 项；Python codegen 64 项；实验 examples 编译。
Windows 原生交互仍需对应平台最终验收。

### P3 Editor 文件入口与平台关联（2026-11-04）

- 文件对话框限制为 `.zipzi` 普通文件，打开支持多选；补充 GPUI macOS/Windows 原生扩展名过滤接口。
- 保存名称先规范后缀，补写后缀导致目标变化时再次确认最终目标；同路径另存为沿用原基线，不能绕过外部修改冲突。
- 标题、状态栏和最近工程统一使用归档路径；冲突提供重新打开、另存为和取消。重新打开通过 SDK 事务执行，失败保留原 Session。
- 在 endpoint claim 前用 `args_os()` 解析多路径和 `--`；启动、系统文件事件、菜单与 IPC 共用有界队列（128 路径 / 1 MiB）。队列按顺序打开，失败继续，同工程激活已有文档。
- Editor 工作集使用平台缓存目录及共享 Runtime 磁盘预算；无窗口 host 仍能接收请求并创建窗口。
- 发布与开发 plist 复用工程类型声明，开发关联优先级为 None；Windows 便携包携带当前用户注册/卸载脚本，保留系统默认应用选择。
- 更新 Editor/acceptance 测试与样本的原生 receipt 字段及归档路径断言。

验证：Editor `--lib project` 21 项、`headless_tests::runtime_host` 24 项、`headless_tests::recent_files` 2 项、启动参数 2 项；Editor core workspace 10 项（含重新打开失败/成功）；Runtime IPC 队列限额和请求重放 1 项；release 6 项、Windows 包 2 项、workspace entrypoints 5 项。`cargo check --locked -p zipzi-editor` 通过。
macOS 原生构建通过；系统双击与 Windows 文件关联交互仍列入 P4 实机验收，未以逻辑测试替代。

### P4 本地工程和故障验收（2026-11-04）

- 真实 Mao（260 Mesh、7 Motion）和大肥鱼（715 Mesh）执行三轮打开、保存、另存为，
  每轮核对完整 authoring 内容与所有有效 PNG 字节。release 普通保存分别为 182–185 ms
  和 1055–1067 ms。方法、内存与容量限制见[验收记录](../archive/benchmarks/2026-11-04-native-project-format-p4.md)。
- 独立子进程验证归档移动、原素材删除、旧工作集回收和两进程过时保存冲突。
- 修复锁定资产无法绑定存储路径的问题；保留锁与 authoring 内容，缺失资产不会被错误补全。
- 补齐保存阶段和真实 payload 字节进度，覆盖 finalize/publish 失败及发布后目录同步警告。
- 共享内存 admission 改为按中央目录、解压 JSON 及单 PNG workspace 预留，
  Job 完成前持续持有；高压缩率输入拒绝后保留旧 Session 并回收候选资源。
- Observe 冻结输入跨 Session 重置/关闭仍可渲染，最终租约释放后清理。
- 样本、生成脚本与压力工具统一为 `.zipzi` 归档；底层 codec/P0 实验保留诊断用途。

已通过 Project lib 51 项、Project integration 51 项、SDK project_io 23 项、Runtime 31 项、Editor core runtime 22 项、
Editor project UI 21 项、Observe GPU 租约测试及样本校验。阶段内 Editor core 完整 integration
617 项通过；Python Local/Connected 行为、recovery、mesh、静态类型与安装后 Connected 门禁
按验收记录命令执行；独立 Python 代码生成工具检查 64 项通过。原生 Windows 和系统双击状态矩阵仍待实机验收。

### P4 macOS 原生事件与保存对话框（2026-11-04）

- 新增隔离的实际 Editor/LaunchServices 验收脚本，覆盖冷启动、ready 前追加、多文件、
  无窗口、运行中重复打开及第二进程 IPC，核对 Session 身份与归档资源。
- 真实面板暴露并修复静默追加其他后缀、规范化后漏掉覆盖确认的问题；
  在原生 replacement 检查前补齐缺失后缀，保留显式大小写/双后缀及其他后缀纠正提示。
- Save As 三种名称均比较完整 authoring 和 PNG 字节；覆盖取消保持原文件和 Session。
- 原生构建、脚本全部场景及 release/dev bundle 工具各 6 项通过。

详见[macOS 实机记录](../archive/benchmarks/2026-11-04-native-project-format-macos.md)。
指定应用打开不证明默认文件关联，Finder 双击与 Windows 原生验收仍待完成。

### 收尾：Finder 默认入口部分验收（2026-11-04）

- 发布文档类型元数据下，核对系统默认应用后通过 Finder 选中文件执行“打开”；
  cold、多文件、ready 前追加、重复激活、第二进程 IPC、资源诊断及原文件字节检查通过。
- resident 场景等待 ready Session 超时，尚未定位；工具只开放 `--finder-default --case cold`，
  不把失败场景计入通过。实际鼠标双击、安装体验与 Windows 仍未验收。
- 清理全部临时验收 bundle 和进程，确认恢复原默认应用；原开发 Editor 未停止。
- 本轮最终工具改动经 Python/Swift 静态检查；交互结果、失败及复现命令见
  [macOS 实机记录补充](../archive/benchmarks/2026-11-04-native-project-format-macos.md)。

P4 跨平台退出条件未满足；本次按用户要求收尾，未将整个 plan 标为完成。
