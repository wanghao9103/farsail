[English](WI-CAPTURE-RECOVERY-043.md) | **简体中文**

# Windows 采集中断与会话范围恢复

日期：2026-11-05。在桌面 0.1.36 之后实现，已随桌面 0.1.37 发布，验收见后文。

## 现场观察

用户反馈 Windows 正常远程时突然结束，没有锁屏或切换用户。控制端日志在结束前 45 ms 仍收到媒体，最终原因是 `session_ended`；被控端截图中共享 / 值守选择仍开启、文件仍可用、屏幕共享已停止。这能确认屏幕运行能力中断，不能确认触发它的 Windows 驱动错误。截图里 Ubuntu 本机「远程连接已关闭」表示该控制端的入站共享状态，本身不禁止向 Windows 发起控制。

## 机制与改动

原实现的采集线程任一错误都会结束生产，随后关闭整台设备的屏幕能力；生产通道先被释放时，发送器还可能抢先以通用原因关闭连接，覆盖原本应送出的 `capture_failed`。这是一条经源码确认的「单会话采集失败 → 重连按钮被禁用」路径；现场具体触发原因仍未知。

现在将 DXGI 访问失效、设备移除、设备重置单独分类，区别于访问拒绝等持续错误。仅这些重置状态最多重建三次，间隔 150 / 300 / 450 ms；构造成功或单帧成功不能重置次数，实际健康帧跨越五秒后才补充预算。重置立即丢弃缓存和坐标、释放已按输入，并在继续前检查活跃会话及共享权限。重建后的显示器必须匹配此前选中的名称和桌面矩形，布局变化不能静默切到另一屏；新画面到达后才启用新的坐标代次。采集布局缺失期间，键盘和鼠标输入均等待。按键清理失败则以 `input_failed` 结束，不自动重试采集。

持续失败会在释放媒体通道前发布 `capture_failed`，只结束当前会话，保留用户全局共享 / 值守选择和文件能力；不会自动无限新建会话。初始没有交互显示器仍安全拒绝，主动关闭、授权到期和输入失败保留原有检查。新批准的连接可重新尝试采集，没有绕过授权、锁屏或 Windows 桌面安全检查。

有限的重置及关闭诊断现保留采集错误，设备提示也区分共享未开启和已中断，不再一律暗示用户未允许。微软说明模式 / 桌面 / 全屏内容变化可能使 DXGI duplication 失效，需要重建：[AcquireNextFrame](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgioutputduplication-acquirenextframe)。

## 验证和边界

证据：`.local/viewer-compact-20261105/`。

- 桌面 Rust 55 项通过，一个无关交互项忽略。新增有限重试预算、实际帧补充预算，以及真实认证 QUIC / 媒体通道清理后对端仍保留 `capture_failed`、持续采集失败不自动再开会话的用例。
- 查看器策略 17 项通过；TypeScript / Vite、Windows 采集模块跨目标检查、桌面 all-target Clippy 和格式通过。
- Linux 原生冒烟拒绝 23 项越权命令，窗口尺寸、全屏及新连接入口恢复已有主窗口通过（`native-final/report.json`）。保留初轮环境和旧文字定位失败。
- 应用内浏览器验证参考式查看器改动，1280 / 440 / 自然 571px 布局、标签、设备列表、关闭、全屏、工具栏展开 / 收起及当前会话 Alt+Tab，见[视觉对照](../../design-qa.zh-CN.md)。

没有在实体 Windows 注入或复现 GPU 重置，也未取得其被控端日志，不能确认这次现场的准确原因，或宣称真实多屏驱动恢复、游戏、跨网及整夜运行通过。初次实现未更改公开包 / 更新通道或个人设备安装，后续发布另见下文。现装旧版可在被控端点击「重试远程连接」重新初始化共享。

## 发布验收

桌面 [0.1.37](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.37) 已作为正式版本发布并设为 GitHub Latest，源码 `9e0ee37b8dafcbf9c7e621506ad7f32aa0be6922`，[发布流程](https://github.com/wanghao9103/farsail/actions/runs/41980249187)成功。Windows x64 与 Ubuntu 两个原生架构的包检查均通过，含 Windows runner 上的安装 / 重启 / 重装 / 卸载，以及原生 IPC、窗口尺寸保留和越权命令拒绝。

12 个公开附件独立下载并匹配 API SHA-256，三个平台签名、源码元信息、11 条校验和、原生托盘证据及逐字节重建通过；普通固定更新地址返回相同的 0.1.37 清单。初轮仅最后的通道比对失败；后续普通地址已相同，仅重试失败发布步骤，12 个版本附件的 ID、摘要、大小及时间均保持不变。未捕获初轮失败任务收到的响应正文，不推断其准确差异内容。一次慢速 Windows 重复下载改用同公开地址的独立匿名 HTTP/1.1 完整传输并核对摘要，原中断日志保留。

证据：`.local/client-public-0.1.37/delivery-verification.json`、`verification.json`、`download-provenance.json`、`windows-http1-provenance.json`、`package-attempt1-status.json`、`immutable-assets-final.json`。Windows 安装包 SHA-256 为 `5b0e0adad38246653b0fd519dde361ff35494b322c8f2bde1e7b3d890641ae34`。控制端与 Windows 被控端均需更新才能覆盖界面及采集修复；没有发布安卓或协调服务。本次仍不代表原实体 GPU 故障或整夜运行验收通过。

## 普通确认框与全屏覆盖补测（0.1.42）

2026-11-06：用户补充普通软件、游戏或广告的确认框会使远控结束。此前三次重建预算不能覆盖较长的模式转换；现在同一次恢复最多持续 15 秒，退避从 150 ms 增至最多一秒，并每 25 ms 检查取消。只有重建后实际取得新帧，随后健康采集满五秒，才补充预算；成功构造或反复单帧不能延长失败期限。恢复时仍丢弃旧画面、坐标和输入，维持严格显示器身份、授权、会话及清理检查。

`DuplicateOutput1` 返回的 `DXGI_ERROR_UNSUPPORTED`、`DXGI_ERROR_MODE_CHANGE_IN_PROGRESS`、`DXGI_ERROR_NOT_CURRENTLY_AVAILABLE` 和选中输出暂时消失现在进入上述有界恢复；这不是把所有错误统一重试。访问拒绝、授权终止和输入清理失败仍按原边界结束。持续采集失败只给当前会话提供显式“重新连接”，重新请求身份验证及批准，重复点击去重；没有自动无限新建会话。

修复提交 `df6779b6d4faa9daaf02e805ee3d519011dd7fb6` 与 `aca78c9e6c179639d7ad495e3308e45bba630ffc` 均已包含于现网源码 `7ff536801d6e2bf2b426fab80ec9b6c9d805f9bc`。本地 main 已快进合并至该源码，未将过时的 0.1.40 候选发布覆盖较新的正式版。另一份固定 RGB 的测试实验已保存在本地补丁 / stash，未覆盖现网以窗口实际 GDI 颜色为参照的夹具。

精确源 [Windows CI42133382209](https://github.com/wanghao9103/farsail/actions/runs/42133382209) 完整成功。真实 DXGI 读取自建窗口像素，五阶段分别为确认前、普通确认框可见、无边框全屏覆盖可见、覆盖关闭、确认框关闭；均命中本进程窗口，并与本窗口 GDI 参照逐通道相差不超过 24。参照值随阶段在 105 与 255 之间切换，帧数为 2 / 2 / 2 / 1 / 1。原先把主题色灰框当成纯黑的失败记录保留；最终夹具用实际主题颜色与阶段差异验证，未放宽画面归属检查。

五阶段实际 reset 次数均为零。因此这是普通确认框及无边框覆盖期间真实采集继续的证据，不是实体用户 GPU 重置、独占全屏、分辨率切换或原现场 HRESULT 的复现。恢复期限与取消 / 新帧门禁另有纯状态回归。原始任务日志 49,839 字节，SHA-256 `1317e6e28ebf66cad3bc4d8f71c3c018e48281da98fd14ab6028f43a17f61e63`；见 `.local/capture-fullscreen-20261105/live-main-audit/native-capture-audit.json`。后来重复运行在全部测试步骤后取消，只作为补充，不替代上述完整成功运行。

桌面 [0.1.42](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.42) 已正式发布，GitHub Latest 与普通固定更新入口一致，[发布流程42133536137](https://github.com/wanghao9103/farsail/actions/runs/42133536137)成功。只读公开审计通过 12 个附件、三个更新签名、11 条校验和、精确源码 CI、实际成功原生任务来源及重建；12 个版本附件身份和字节保持不变。Windows 安装包 8,842,288 字节，SHA-256 `76ccbd856f0c0c73fb9e518a7a465de4861e98f82713b1ec252e4c2797c28b2f`。证据 `.local/client-public-0.1.42-capture-audit/delivery-verification.json`。本次审计没有重发版本、修改通道或替用户安装；控制端与 Windows 被控端均需更新才覆盖完整修复。
