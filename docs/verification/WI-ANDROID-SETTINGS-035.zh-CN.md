[English](WI-ANDROID-SETTINGS-035.md) | **简体中文**

# 安卓共享设置 — 0.1.9

首页直接显示“我的设备”，只负责设备发现、控制与仅查看。本机被连接能力位于“设置 → 共享这台手机”；共享子页统一管理录屏、同账号控制、防止熄屏、前台恢复和辅助功能授权。新的连接请求在首页提供处理入口，到共享子页批准或拒绝。辅助功能授权完成或恢复共享时进入该子页，并保留返回设置的路径。

MainActivity 与 DeviceListActivity 复用 DeviceListContent；离开页面会使异步设备加载回调失效。MobileUi 将系统栏、刘海和键盘边距应用于裁剪后的滚动视口，避免内容留白随滚动消失后文字压入状态栏。最低 API 仍为 26；导航栏主题属性位于版本化 API 27 资源。没有新增远控权限或保存录屏令牌。

专用 API 35 模拟器通过：首页与设置职责分离、设置路径、请求处理入口、表单隔离、辅助功能引导和共享失败原因显示。初次 System UI ANR 另存现场；一项旧被控测试在移除任务后仍去首页寻找停止按钮，改为设置路径后，真实 MediaProjection、后台 / 任务移除保活、停止和通知 / 唤醒锁清理通过。早期失败运行保留。

真实进程替换通过控制记忆、新录屏授权、取消、主动停止及旧代次 / 异设备拒绝。真实 JNI/QUIC 控制端回归通过合成电脑 / 手机对端、静止帧超过六秒后的输入、触摸 / 拖动 / 滚轮 / 文字、仅查看拒绝、旋转输入、后台关闭及待批准取消。Kotlin 单元测试与 lint 通过。本地证据位于 `.local/android-navigation-verification/`、`.local/android-memory-verification/attempt-1793693916725949497/` 和 `.local/android-controller-verification/attempt-1793693993046810019/`。

0.1.9 / code 10 本地安装候选沿用已安装证书；Rust 原生源码未变，打包复用已核验的公开 0.1.8 APK 中的双架构原生库。它是本地签名候选，没有新增公开发行或发布更新通道。签名 release 构建、Kotlin 单测与 release lint 通过；证书连续性、非调试 manifest、manifest/dex 无调试探针、双架构 ELF 及 ZIP 16 KB 对齐通过。连接的实体手机覆盖安装成功，版本 0.1.9 / code 10 和已安装 APK 一致，首次安装时间保留，没有卸载或清除应用数据；MainActivity 启动成功且进程存活。检查时仍锁屏，因此没有验收手机界面或真实账号恢复。APK SHA-256：`08287984a46393565fd975953d3838dc5c4093e6407f98be6fb6a9b911fe00a0`。本地证据：`.local/android-install-navigation/apk-verification.json`、`installation.json` 和 `native-provenance.json`。

真实厂商策略、生产账号跨网、API 26 运行和本轮最终页面结构的大字体专项尚未验收。此前模拟器大字体下的导航键像素配色差异，不能仅凭窗口 appearance 标志就宣称已解决。
