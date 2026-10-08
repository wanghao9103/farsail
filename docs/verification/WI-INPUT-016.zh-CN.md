[English](WI-INPUT-016.md) | **简体中文**

<a id="windows-preview-016--ordinary-remote-pointer"></a>

# Windows 预览版 0.1.6 — 普通远程指针

- 基线 `c72d76a`，干净的当前受管理检出；分支 `codex/viewer-cursor-016`。
- 用户澄清十字光标只是看起来不可交互，并非观察到控制失败。CSS 十字光标替换为普通箭头；控制暂停时显示禁止操作指针和解释性工具提示。
- 范围：指针 CSS、查看器工具提示/状态样式、发布版本和文档。
- 运行时：当前检出缓存，仅自有前台测试，回环 Vite/QUIC，临时 Windows CI。主检出进行中工作和生产配置不变。

<a id="evidence"></a>

## 证据

- `npm run build` 和 `git diff --check` 通过。现有查看器回归脚本的一个忽略的临时副本检查了实际计算后的光标样式：正常控制为 `default`，暂停控制为 `not-allowed`。现有鼠标/键盘、工具栏/配置、暂停/重试、只读和终止行为通过。此次外观改动未添加永久测试。
- 源码 `bdbfd17f9565a770b136a48e2feacc779056dd99`，[Windows 客户端 36675505835](https://github.com/wanghao9103/farsail/actions/runs/36675505835) 和[安装程序 36675506639](https://github.com/wanghao9103/farsail/actions/runs/36675506639)：成功。安装程序包括实际安装/设置 IPC/重启/重装/卸载保留和 Rust 回归。
- [0.1.6 发布版本](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.6-bdbfd17) / [Windows x64 安装程序](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.6-bdbfd17/FarSail_0.1.6_x64-setup.exe)。
- 全部四个公开附件均已匿名下载，并按哈希/大小与 CI 原件匹配。安装程序：7,869,856 字节，SHA256 `a55eb905d795e3332e99d756e28250976ead476d1fc3d3e9738c0a7413197160`。元数据和安装报告绑定到同一源码/哈希。安装程序仍未签名。
- 仅修改指针外观和工具提示；不声称修复了未被报告的功能性输入失败。测试 Vite/浏览器运行时已停止，未触及生产应用/配置/服务器。此次简单外观调整不创建新的知识笔记。
