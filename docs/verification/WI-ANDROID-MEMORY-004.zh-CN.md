[English](WI-ANDROID-MEMORY-004.md) | **简体中文**

# 安卓访问选择记忆 — 0.1.4

控制选择原先仅在原生运行内存里，录屏授权也不能跨进程复用。现在按认证服务器和绑定设备保存意图，在辅助功能真实连接后恢复已允许的控制；录屏结束后回到前台主页会请求新的可见系统授权。仍在运行的共享直接保留，不重放录屏 token，也不代授辅助功能权限。

App / 通知停止和取消录屏授权会清除共享意图，退出登录清除两者。控制代次防止迟到恢复覆盖新关闭；已消费录屏请求代次与服务器 / 设备范围防止旧授权重新启动已停止或异设备共享。设置可关闭前台共享恢复。

本地 Kotlin/APK/lint 和专用 API 35 验收通过：相同证书覆盖安装导致真实进程更替、不含凭据的私有偏好保存、原生控制恢复、后台不自动重开录屏 / 不弹授权、前台新的系统弹窗、取消后不循环、主动停止后再覆盖安装、旧恢复 / 授权拒绝、异设备拒绝和登出清除。第 1 阶段在真实共享运行时被有意终止，crash 标记是预期交接，不算 JUnit 通过；正常进程界面与状态由外部核验，独立意图保护测试正常通过。

只读状态 provider 仅在 debug 构建，要求 shell DUMP 权限及模拟器；正式构建不包含，发布前再次检查 manifest。独立尝试出现过 API 36 模拟器 System UI ANR、崩溃 / 强制停止后的辅助功能未绑定，未算作成功验收，也未通过产品改系统权限来绕过。实体厂商后台策略、强制停止后的服务恢复不在已验证范围内。

安卓 android-v0.1.4 / version code 5 已从源码 9375ffa616d76376e905b3d0bfa7b2ab474b30f0 发布。流水线 41433064652 在仅重试发布后成功；4 个不可变附件的 ID、摘要和时间戳保持一致。全部匿名完整下载与 API 摘要、签名元数据及固定更新通道一致。APK SHA-256：f76bac4220ff2bde074667dbca116718d288312d5aef47aa9aec973b7de0602b。原始签名证书、非调试 manifest、manifest / dex 无 debug provider、两种原生 ABI 的 LOAD 对齐和 APK 16 KB 对齐检查均通过。证据：`.local/android-public-0.1.4/final-verification.json`、`native-alignment.json`、`immutable-assets-retry.json`。

实体手机覆盖安装返回 Success，版本及已安装 APK 摘要一致，首次安装时间保留，没有清除应用数据。MainActivity 启动成功。手机仍锁屏，未新增厂商实机偏好恢复或真实账号 UI 验收。前版只在内存保存，首次升级后显式选择一次才建立持久化意图。

## 授权期间的选择持久化 — 0.1.8

此前控制选择直到辅助功能连接后才保存。首次开启跳转系统设置期间若进程消失，选择可能丢失；已成功保存的选择原本已能保留。现在开关与授权引导操作均先保存意图，再离开 App；共享选择也在请求系统录屏授权前保存。初始化同时发布已认证状态与就绪标记；成功录屏结果在 Activity 内等待该状态就绪，继续核对请求代次与身份。取消、停止和退出登录仍撤销对应选择。主页明确区分记住选择后的待恢复状态与实际录屏中。

专用 API 35 模拟器的选择驱动已通过：授权前保存控制、无权限时实际相同证书覆盖包更替进程、辅助功能连接后自动启用控制、真实成功录屏授权等待账号就绪、主动停止拒绝延后的授权。证据：`.local/android-choice-verification/attempt-1793679415645304676/verification.json`。早期尝试出现测试手势时序及模拟器 attach 失败，不计为产品验收。进程更替交接仍为预期 instrumentation 终止，由新进程状态独立验证。

0.1.8 的既有正常进程记忆回归也已通过：替换前真实共享、恢复控制、前台新授权、取消后不循环、主动停止后再次重启、迟到代次拒绝、设备范围与退出清理。证据：`.local/android-memory-verification/attempt-1793679626021029139/verification.json`。测试 fixture 现在等待辅助功能真实解绑再重新绑定，并将控件滚动到稳定视口；不修改生产系统权限。Debug 构建、Kotlin 单元测试与 lint 已通过。

被控端 NativeSmokeTest 返回 OK（4 tests），其中 API 36 专属安全锁屏项在 API 35 上跳过。真实 JNI/QUIC 手机控制端对合成电脑/手机的回归也通过，涵盖静止画面控制、旋转触点、文字、仅查看拒绝输入、后台关闭和待批准取消。证据：`.local/android-choice-native-regression.log` 与 `.local/android-controller-verification/attempt-1793679803317677371/verification.json`。测试等待设备列表刷新后再取控件，首次旧辅助功能节点异常属于测试同步问题，不算产品行为验收。

安卓 android-v0.1.8 / code 9 已从源码 79a522cd10e0649a9566f8a54e2ecb3e8c25854b 正式公开发布。流水线 41577963983 在固定通道缓存收敛后仅重试发布成功，四个版本附件的 ID、摘要与时间戳均未变化。匿名下载、源码来源、更新签名、固定通道、原安装证书、非调试 manifest、无 debug 探针及双 ABI / ZIP 16 KB 对齐均通过。APK SHA-256：afea5fa1d34d9700dcbb355d03beb790ec667c671775bf91369f4b56526fb3b9。证据：`.local/android-public-0.1.8/final-verification.json`、`native-alignment.json`、`immutable-assets-retry.json` 与 `local-regressions.json`。

正式 APK 校验完成时，实体手机已未连接 USB。本次未对实体手机执行安装或清除数据，厂商系统上的恢复效果仍未验收。已验证交付包为 `.local/deliverables/FarSail_Android_0.1.8_universal.apk`。此前未保存的选择，升级后需明确开启一次；记住意图不替代新的系统录屏确认。

### 重新连接 USB 后的实体安装

用户在发布后重新接回实体手机。对已验证的 0.1.8 / code 9 APK 执行覆盖安装返回 Success，已安装 base APK 的 SHA-256 与公开制品一致，首次安装时间未变。未卸载或清除应用数据。MainActivity 启动返回 Status: ok，进程保持存活。系统仍显示锁屏，因此本次安装检查不计为真实厂商偏好恢复或界面交互验收。证据：`.local/android-public-0.1.8/physical-installation-018.json` 与已更新的 `final-verification.json`。
