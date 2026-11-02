[English](ANDROID.md) | **简体中文**

<a id="android-host"></a>

# 安卓被控端预览

原生安卓 App 支持通过 FarSail 电脑端查看和控制手机。这是本地预览实现，尚未公开发布手机版。复用现有协调服务、Ed25519 设备身份、逐次批准、iroh 加密传输、JPEG 画面及短期授权租约，无需修改协调服务数据库结构。

## 安装和连接

使用手机版前，需要用本轮源码升级协调服务：旧协调服务虽然识别安卓设备身份，但在线屏幕共享只允许 Windows，会用 HTTP 409 拒绝安卓开启共享。本次允许 Windows 和 Android 被控端，保留心跳代次、设备在线和批准检查，无需新增数据库迁移。本轮不会自动部署生产服务器。

在安卓手机安装本地构建的 APK。最低支持 Android 8，APK 包含 ARM64 和 x86_64 原生库，不包含旧的纯 32 位手机。

- 输入与电脑端相同的 HTTPS 服务地址，登录后添加这台手机。App 支持注册和邮箱验证。
- 仅查看时，点击“开启屏幕共享”，确认 Android 系统录屏授权。请求共享整个屏幕，让远程触摸坐标对应真实屏幕。
- 需要控制时，在系统设置开启 FarSail 辅助功能，再开启“允许已批准的电脑控制手机”。仅开启辅助功能不会授予远程连接。
- 在电脑端选择在线手机，申请查看或控制，再在手机批准本次请求。两端均显示连接核对码。
- 鼠标点击对应轻触，拖动在松开后对应有时限的滑动，滚轮用于滚动。右键和 Escape 对应返回。更新后的电脑工具栏提供返回、主屏幕和最近任务。可向当前聚焦的普通可编辑输入框发送 Unicode 文字。
- 电脑可同时打开不同手机或电脑的查看窗口。各窗口的画面、输入、重连和关闭状态独立。现有原生客户端上限为十六个传输会话；每台手机同一时间接受一个查看端。

通过 App 或通知中的“停止共享”停止采集，并立即使排队输入失效。Android 已经接受的手势可能在最长 1.5 秒的限定时间内完成。密码框、任意桌面快捷键、多点触控、声音和手机文件传输不在本次预览范围。

## 后台运行

切换应用、按主屏幕键或移除 Activity 任务后，显式启动的录屏前台服务继续运行。服务持有采集、传输状态轮询及 CPU 部分唤醒锁。Android 13 及以上不主动申请通知权限：未授权时，前台服务仍显示在系统任务管理器中，但通知栏不显示这条通知。可按需在系统设置开启通知。系统通知保持静默，没有声音或震动，停止共享后消失。Android 要求持续后台录屏使用前台服务通知；App 不隐藏该通知，也不通过开机广播重新开启采集。

部分厂商会限制后台活动。“后台运行设置”可打开应用设置，由用户允许后台运行。强行停止、收回录屏权限、进程结束以及系统锁屏行为可能停止共享。进程结束后不复用旧授权，需要重新打开 App 确认录屏。辅助功能断开会关闭控制连接，仅查看可继续。

平台要求见：[录屏](https://developer.android.com/media/grow/media-projection)、[前台服务类型](https://developer.android.com/develop/background-work/services/fgs/service-types)、[通知权限](https://developer.android.com/develop/ui/compose/notifications/notification-permission)、[辅助功能服务](https://developer.android.com/guide/topics/ui/accessibility/service)。

## 构建和验证

使用 Rust 1.93、JDK 17 或更新版本、Android SDK 35、NDK 27.2.12479018 和仓库内的 Gradle wrapper。按本机路径设置 SDK/JDK 环境变量：

```sh
export JAVA_HOME=/path/to/jdk
export ANDROID_HOME=/path/to/android-sdk
rustup target add aarch64-linux-android x86_64-linux-android
scripts/build-android.sh
```

输出为 `apps/android/app/build/outputs/apk/debug/app-debug.apk`，使用本机 Android 调试密钥签名。这是可安装的测试包，生产签名和公开发布需要另行处理。脚本为原生库配置 16 KB ELF 对齐，并运行 Android lint 与 JVM 测试。缺少任一原生 ABI 时 Gradle 拒绝打包。参见 [Android 页大小要求](https://developer.android.com/guide/practices/page-sizes)。

```sh
cargo test -p farsail-android
cargo clippy -p farsail-android --all-targets -- -D warnings
cd apps/android
./gradlew connectedDebugAndroidTest
```

需要完整模拟器媒体/输入探针时，启动 API 35 模拟器并构建两份 debug APK，再从仓库根目录执行：

```sh
cargo build -p farsail-android --example device_probe
(cd apps/android && ./gradlew :app:assembleDebugAndroidTest)
python3 scripts/test-android-emulator.py
```

脚本清除专用模拟器 App 的合成测试数据，可见地确认系统录屏授权，并检查完整 JPEG 解码及真实鼠标/文字输入。不要用于实体手机。

主机测试使用真实本地加密 QUIC 连接验证两台手机会话互相独立、画面 ACK、有序输入、仅查看拒绝控制、旧布局及已取消的采集开启操作。设备测试使用合成本地协调服务响应，以及真实 Keystore/JNI/录屏前台服务。可选的 `realPeer` 设备测试通过模拟器 UDP 转发连接主机探针，向自建测试输入框发送文字。以上不证明实体手机兼容性、生产账号协调或跨网中继行为。结果与缺口记录在[实现验证](verification/WI-ANDROID-001.zh-CN.md)。

## 代码边界

`apps/android` 包含原生 Kotlin 界面、Android Keystore AES-GCM 存储、录屏前台服务和辅助功能输入适配。凭据保留在由不可导出 Android Keystore 密钥保护的原生存储中，禁用备份。录屏服务不对外导出，辅助功能服务由系统绑定权限保护。

`crates/android` 提供 JNI 桥接及主机运行时，复用 FarSail 的客户端、传输和媒体 crate。在 Android 执行排队操作前，再检查采集代次、布局版本和有效会话租约。画面使用最新值槽、最多两帧在途、ACK 时限及流量预算。本机停止优先于迟到的服务端响应。经过认证的对端消息标识安卓导航能力；旧电脑端仍能接收基础画面和输入，只是不显示新增导航按钮。
