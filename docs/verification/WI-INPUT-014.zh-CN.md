[English](WI-INPUT-014.md) | **简体中文**

<a id="windows-preview-014--input-rejection-must-not-end-screen-viewing"></a>

# Windows 预览版 0.1.4 — 输入被拒绝不得结束屏幕查看

<a id="baseline-and-scope"></a>

## 基线与范围

- 基线 `cc95071`（已发布的 0.1.3），干净的受管理 desktop-ui-release 工作树；分支 `codex/input-recovery-014`。
- 用户反馈在普通桌面/应用中移动/点击会立即断开。截图显示 `input_failed`；它没有暴露原始 Windows 错误，也不能证明 UIPI。
- 已确认的代码路径：Move/Button/Wheel 先使用 SetCursorPos，将任意失败转换为 InputDenied，然后关闭整个会话。Caps Lock/Windows 等普通键盘按键也被排除，并产生同样具有误导性的终止原因。
- 写集：Windows 原生输入、桌面主机/查看器输入状态协议和 UI、针对性浏览器/原生测试、版本/发布文档及相关 CI。保留认证/租约/停止和受保护桌面边界。主检出的文件传输进行中工作未改动。
- 运行时：此工作树的 target/node_modules/.local，仅自有前台测试窗口，回环 Vite，临时 Windows 安装程序 CI。不捕获私人桌面，也不向无关应用注入输入。

<a id="implementation-and-verification"></a>

## 实现与验证

鼠标使用 SendInput 的绝对虚拟桌面像素中心坐标，替代 SetCursorPos。支持普通系统/导航按键；不支持的输入和过期几何信息会被忽略，而不是报告为权限失败。SendInput 记录实际插入/预期数量及 Windows 错误码，不声称错误码零可识别 UIPI。

输入状态消息有界、按序，并通过现有已认证 Control 通道发送。操作系统拒绝输入时暂停主机输入并尝试释放按住的按键，媒体任务继续运行。暂停时使查看器输入队列失效。明确的 `resume_control` 使用独立且有序的恢复标记；普通失焦/释放不能隐式恢复控制。在输入锁下，新输入仍受原控制授权、本地共享状态、当前会话和取消检查约束。撤销、媒体/网络终止状态和受保护桌面采集失败保留现有停止行为。两台 PC 均需要此客户端更新；无需修改服务器。

<a id="local-checks"></a>

### 本地检查

- `cargo test --locked -p farsail-windows --lib`：8 项通过，有意忽略 2 项交互测试。新增覆盖包括负虚拟原点/像素中心往返、常见系统按键及不支持输入在注入前被拒绝。
- `cargo test --locked -p farsail-windows real_input_into_own_foreground_window -- --ignored --test-threads=1`：在 Windows 上通过。测试在注入无害文本、组合键、移动/点击前，确认前台 HWND/PID 属于自身进程。无 GUI 工作线程在该测试窗口内移动光标并验证实际像素位置；没有无关应用收到输入。
- `cargo test --locked -p farsail-desktop --lib`：5 项通过。包括实际已认证回环 QUIC 传输暂停诊断、暂停后媒体及明确恢复，以及拒绝过期/超大/未知字段通知。模拟授权权威仅用于测试；这不是实体双机 Windows 失败复现。
- desktop/windows 全目标 Clippy `-D warnings`、Rust 格式、前端构建及两套浏览器回归均通过。查看器测试确认图像保留、暂停输入受抑制、明确恢复、终止帧/队列清理，以及不变的只读/重连规则。截图 `input-paused.png` 仅含模拟像素。
- 用户截图缺少原生 API 错误详情，因此其机器上的精确 Windows 失败原因仍未证实。本次改动修复已识别的鼠标路径，并防止单独的输入拒绝关闭视频；仍需在两台更新后的 PC 上实地验证。

<a id="release"></a>

### 发布

- 源码 `40edacb2e17b1ac1d2dc04baa3457135fcb831e2` 已推送到 main。
- [Windows 客户端 36660400761](https://github.com/wanghao9103/farsail/actions/runs/36660400761)、[安装程序 36660402240](https://github.com/wanghao9103/farsail/actions/runs/36660402240)、[传输 36660400809](https://github.com/wanghao9103/farsail/actions/runs/36660400809)、[后端 36660400832](https://github.com/wanghao9103/farsail/actions/runs/36660400832)：全部在该精确提交上成功。
- 安装程序 CI 包括锁定原生发布构建、真实安装/设置 IPC/重启/重装/卸载保留、Rust 测试/Clippy 及无调试探针检查。客户端 CI 包括原生窗口隔离及两套浏览器测试。
- [0.1.4 发布版本](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.4-40edacb) / [Windows x64 安装程序](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.4-40edacb/FarSail_0.1.4_x64-setup.exe)。

| 公开附件                       |    字节数 | SHA-256                                                            |
| ------------------------------ | --------: | ------------------------------------------------------------------ |
| FarSail_0.1.4_x64-setup.exe    | 7,849,691 | `7cbec092f01f7ee6f05d15fea1fcf4c163f4c5b0e41aeebdfb9b592e8186322b` |
| SHA256SUMS.txt                 |        95 | `be02c790b0b02eecf076101bc691f60838b463e98abd798f853272db77f18608` |
| release-metadata.json          |       651 | `7defe47b8379b3cb04c974366433e4431ba35536ce41f3a125d6abc55c6a6bf9` |
| installation-verification.json |       422 | `d19f4bb8ad4265f29492e3476389cf5032d6c708f6c41d617b4e85d4e5020a9c` |

全部四个公开文件均已匿名完整下载，并按字节数和 SHA-256 与已测试的 CI 原件匹配。首次下载公开安装程序遇到 TLS/超时中断；有界 HTTP/1.1 Range 续传完成现有部分文件，随后进行了完整哈希验证。TLS 验证始终启用。安装程序未签名；未修改生产安装/配置或公开服务器。

运行时已关闭：自有输入测试窗口已销毁，浏览器进程和 Vite 已停止，回环传输测试端点已关闭。忽略的构建/下载证据仍保留。后续工作应复用当前受管理检出，而不是带已暂停文件传输进行中工作的过时主检出。

参考：[SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)、[MOUSEINPUT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-mouseinput)、[SetCursorPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setcursorpos)。
