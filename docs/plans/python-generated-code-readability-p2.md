# Python 生成代码可读性：P2 记录

2026-10-30。生成入口现在将编译契约归一化为类型图：327 个完整或显式外部
Rust 类型身份，其中 311 个完整定义均有领域归属。完整定义冲突、未声明的
opaque、缺失引用、类型循环和未审核领域归属会中止生成；字段及 variant 的
原始顺序保持不变。类型、DTO、场景、资源、API 与 binary renderer 解析同一
图的引用；schema 的旧格式输出仍直接保留编译契约，待 P4 替换。为后续 replay
保留的展开适配器不参与日常生成。

Python API 模型负责生成类的公开名称、文档和辅助方法；catalog renderer 只
渲染整理好的 `DocEntry`。生成器不再读取磁盘上的旧生成 Python 文件，测试以
损坏和多余的旧文件验证这一点。领域依赖图保留确定性的强连通分量，供 P3
设计具体 import：目前有一个跨多个领域的分量，涉及 addressing、bindings、
commands、deletion、mesh、motion、observation、project、resources、sampling 和
scene。不能将这些领域简单地按当前分组直接相互导入；需按具体类型依赖安排
局部 codec 导入或进一步调整归属，并验证默认值及注解解析。

现阶段公开 API 和全部 74 个生成文件均保持字节一致；目标领域模块路径是
投影模型中的计划目标，实际模块和重复类型的合并留到 P3。

验证：`test_python2_codegen.py -k Python2Codegen`（28 项，另有禁用展开适配器
的独立用例）、`test_python2_codegen_graph.py`（8 项）、源归档输入检查、
`generate.py --check --no-cache` 和 `python2 --discovery` 均通过。
