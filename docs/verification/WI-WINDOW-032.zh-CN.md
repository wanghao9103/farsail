[English](WI-WINDOW-032.md) | **简体中文**

# WINDOW-032 — 交叠窗口与鼠标释放

日期：2026-10-10。基线 `dab641d`，客户端候选 0.1.21。本记录针对候选源码，不表示已公开发布或用户游戏现场已验收。

## 现象与发现

用户从 Ubuntu 控制 Windows。同款游戏的两个窗口交叠，坐标准确、前窗可点击，但后窗露出的内容和标题栏均不能切到前面。尚未检查具体游戏及其输入消息。游戏鼠标捕获、激活行为和注入处理仍是候选，不能写成已证明的唯一根因。

Viewer 转发一张桌面图像上的坐标，没有按 Windows 前后台窗口筛选目标。被控端普通输入也不以前台完整性预检拦截，该只读见证用于注入失败后的恢复。原自建两窗口测试每次点击前先激活目标，因此漏测了靠点击激活后窗。

另一个释放缺口已在真实浏览器事件与延迟合成 IPC 下复现：图像外 pointerup 先排队定位 UP，随后出现 lostpointercapture 和 pointerleave；旧离场处理推进输入代次，废弃该 UP。最终 release-all 能安全清理按钮，却丢了拖放落点。修正只保留本次正常捕获释放引发的 leave，先送 UP，再有序 release-all。真实 blur、异常捕获丢失和普通 hover 离场仍取消旧队列；外面已按住的按钮不能因入场而创建远控手势。

## 检查的改动

- 合法的新按钮手势可请求正常激活实时命中的可见、启用顶层窗口。已持有的拖拽、移动和 UP 不请求激活；输入桌面与已知完整性检查保持保守。Windows 仍能拒绝，原 SendInput 行为与错误处理保留。生产输入不使用 AttachThreadInput、强制 Z 序、恢复窗口或提权。
- **更多操作 → 切换远端窗口（Alt+Tab）**沿现有授权串行队列发送 release-all、左 Alt DOWN、Tab DOWN/UP、Alt UP。仅查看、暂停、结束和旧代次输入不能使用；本地系统对快捷键的拦截不能替代这个显式远端操作。
- 原生回归用自建交叠窗口的露出命中点，禁止逐次预先激活目标，验证实际激活、目标收到客户区/非客户区 DOWN 与 UP、连续点击、捕获与释放。完整点击由同一 GUI-less worker 排入，GUI 泵消息；标题栏测试计数后跳过系统移动循环，客户区拖动保留真实 EDIT 处理。真实注入前核对 HWND/PID 和命中点，取消和清理也限制在自有前台窗口，只操作测试创建的窗口，不采集桌面像素或实际游戏。

## 验证

延迟 IPC 外拖释放回归在修前失败。修后的完整 Chromium 和 WebKit 合成 IPC 回归通过：延迟 DOWN 后边缘 x=1 的 UP、唯一且有序的 release-all、外部按住入场拒绝、真实 active capture 丢失和 blur 取消、严格五条远端 Alt+Tab 以及暂停时废弃整组旧快捷键。原画质、布局、重连、终止和 FPS 检查也通过。TypeScript、Vite 生产构建与格式检查通过，日志为 `.local/viewer-input-final-{chromium,webkit,build}.log`。

Linux portable Windows 模块 6 项策略检查、Rustfmt 和 all-target Clippy 通过。这不能证明 cfg(windows) 的 API 编译或原生行为；Windows 编译及真实自建交叠窗口结果由后续 CI 补证。

首轮提交 `36bf83c` 的 [Windows CI](https://github.com/wanghao9103/farsail/actions/runs/38025377398) 已通过生产代码/测试代码编译和 23 项普通 Windows 测试，但真实交叠检查 11.08 秒失败于目标点击、前台和捕获释放的联合等待，之后的 Clippy/原生查看器未执行。未带阶段与消息细分的失败日志不足以归因；补充测试专用诊断后继续调查，不跳过断言或逐次预激活目标。Ubuntu 完整 WebKit 三套 UI 与真实协调/文件集成已通过。

Windows 工作流将原生输入放在独立并行 job，避免等待整个桌面构建才发现原生问题。它只显式选择 `real_input_into_own_foreground_window`，使用 `--ignored --test-threads=1` 与超时，不启用另一个真实桌面采集测试。普通输入记录器单元不能调用真实窗口激活。

## 边界与关联

用户游戏的实际 Ubuntu→Windows 双机操作仍待验收。普通 EDIT 夹具或模拟捕获通过不能证明游戏接受注入；自建窗口与注入 worker 同属一个进程，也不能证明跨进程游戏满足 Windows 的前台政策。跳过 WS_EX_NOACTIVATE 仍无法保留所有应用自行返回 MA_NOACTIVATE 的策略，显式点击提前请求激活属于本次行为变化。不绕过游戏过滤、管理员窗口、安全桌面、模态禁用或 Windows 前台限制；原控制授权、共享/会话检查、几何和取消继续有效。

实现与证据入口：[原生输入](../../crates/windows/src/native.rs)、[输入环境](../../crates/windows/src/input_context.rs)、[查看器回归](../../scripts/test-remote-viewer.cjs)、[PR #2](https://github.com/wanghao9103/farsail/pull/2)。主源 API：[WM_MOUSEACTIVATE](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-mouseactivate)、[SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow)、[WindowFromPoint](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-windowfrompoint)。
