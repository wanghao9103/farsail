[English](WI-FIRST-FRAME-046.md) | **简体中文**

# WI-FIRST-FRAME-046：UAC 桌面暂停与 Android 静止首帧

日期：2026-11-10。源码基线：88263ed；候选版本：桌面 0.1.43、安卓 0.1.16 / code 17；发布结果另行补证。状态：部分验证。

## 问题与处理

- Windows UAC 截图确认安全桌面边界。采集线程检查 UOI_IO，离开输入桌面清除帧缓存、坐标和采集器；暂停最多 120 秒，持续检查取消与授权。回原桌面先释放旧输入，再重新采集。暂停不消耗显卡恢复时间，也不补充已有失败预算。普通桌面错误仍遵循原有限恢复；不改变管理员清单或系统 UAC，不点击系统授权。
- Android 唯一静止帧可能在连接前已超过 700 ms，原 sender 会永远等待下一回调。首次发送允许使用有效 capture/layout/session 中的最近缓存，并保留采集时间；首帧可用上次尺寸，后续仍检查协商尺寸与实时年龄。首帧前不因 profile 协商丢弃唯一缓存，停止及几何变化继续清理。
- MediaProjection 不再 acquire 后因 100 ms 限帧直接丢弃图像；改为有界延后读取最新 ImageReader 图像，尺寸变更与停止移除旧任务。
- 查看器五秒无首帧显示具体说明与人工重连入口；首帧到达后原连接正常展示，不自动循环重连。

- 显示器下拉框改为独立“切换屏幕”按钮，依次循环选择显示器；正常单屏禁用，不可用屏幕仍允许显式恢复。复用现有显示器门禁和新帧确认，切换中禁止重复请求，悬停查看当前屏幕信息。

## 验证

证据目录：`.local/capture-first-frame-20261110/`。

- Android 真 QUIC 静止首帧与旧 viewport 重连用例：2/2 通过（static-bootstrap.log），原采集时间保持，停止清缓存；没有依赖额外采集回调。
- Android Rust 全套回归 28 通过、2 忽略，Clippy 无 warning；见 android-final-rust.log / android-final-clippy.log。
- Gradle compileDebugKotlin、testDebugUnitTest、lintDebug 全部通过（android-build.log）。
- npm build 通过；ViewerDisplayGate 显示策略十项通过。
- capture_recovery.rs 独立 rustc --test 七项通过，含暂停保留失败预算和必须新帧；完整 Linux desktop 测试因缺 GTK/GIO 库不能执行。
- farsail-windows GNU 目标 cargo check --tests 通过；完整 desktop GNU 交叉检查因缺 MinGW C 编译器不能完成（desktop-windows-check.log）。
- Chromium 首帧慢的合成 IPC 交互通过（first-frame-ui.log）：提示、人工重连入口、不自动重连、同连接首帧后提示消失。运行入口 UI_FIRST_FRAME_ONLY=1 scripts/test-remote-viewer.cjs；本机临时 runner 仅替换 Playwright 包及现有 Chromium 的路径，不修改用例语义。

- Chromium 原有 metadata-only 首帧协商和四窗口独立 viewport 用例通过（viewport-ui.log）；保留首帧之前的尺寸协商能力。

- 原有 Chromium 完整查看器合成 IPC 套件通过（viewer-ui-run.log），覆盖多标签、输入释放、重连、显示策略及本地弹窗隔离；临时 runner 只指定本机浏览器路径。

- 按钮修改后针对性 Chromium 合成 IPC 检查通过：1 → 2 → 1 循环切换、无显示器下拉框、单屏禁用（screen-button-ui.log）。按钮前的完整查看器套件证据不能冒充按钮后的完整套件。

## 未覆盖

没有 adb 设备或 Windows 实体环境；OPPO/ColorOS 真机、UAC 授权/拒绝/120秒等待、锁屏与持键恢复、物理双机、跨网首帧耗时未验证。缓存首帧是最近一次采集结果，不代表当下新采集或最终呈现延迟。没有改更新通道、发版本、安装用户客户端或修改个人配置。
