[English](WI-ANDROID-CONTROLLER-005.md) | **简体中文**

# 安卓控制端 — 0.1.7

手机 App 新增独立的我的设备入口和前台远程查看器，可控制 Windows 电脑及安卓手机。控制端需要登录和绑定本机，不需要本机录屏或辅助功能权限。复用对方批准、认证设备身份、短期租约及查看 / 控制权限，不引入协调服务改动。

查看器支持等比直接点击 / 拖动、滚动模式、右键、文字与常用键盘操作、本地画面旋转及已声明显示器选择。安卓对端显示返回、主屏幕和最近任务。显示旋转和触控逆映射复用同一等比几何。画面只保留一个最新值槽，解码图像有界，输入有界且按序，拒绝旧布局或旧输入代次。后台、退出、取消及登出先同步使主动连接失效，再异步清理协调服务；仅查看拒绝所有控制包。

被控与控制分别拥有会话和采集权限，复用认证端点。被控订阅忽略主动连接，停止本机采集不再拆掉主动查看器；身份变化会取消。手机控制端拥有一个前台查看器，原桌面多窗口行为保持。

12 项 Rust 测试通过，包含两类对端能力的真实认证 QUIC 和合成协调服务流程、ACK、输入顺序、输入暂停、旧几何 / 代次拒绝、仅查看选屏、请求取消和独立本机停止。控制端夹具只记录合成输入，不注入宿主系统。Kotlin 几何测试覆盖全部旋转、等比留白及拖动夹紧。API 35 专用模拟器界面通过真实 JNI、认证 QUIC 与合成电脑 / 手机对端验收：实际点击 / 拖动、滚动、文字、安卓导航、旋转触控、仅查看拒绝、后台关闭及待批准取消。2 项 Kotlin 几何测试、debug APK / lint 及四项原生被控回归成功（API 36 安全锁屏项在 API 35 跳过）。证据：`.local/android-controller-verification/verification.json`、`.local/android-controller-host-regression.log`、`.local/android-viewer-rust-tests.log`。正式打包和公开校验已完成；本记录不声称实体 Windows / 手机双端或跨 NAT 已验收。

最终 0.1.6 界面仅为电脑对端显示电脑快捷键；安卓提供文字、回车、退格、全选和返回。滚动模式明确显示开启文字并高亮。专用模拟器回归已包含安卓菜单排除、全选按键送达及滚动选中状态断言并通过。另一次启动尝试被已捕获的 System UI ANR 挡住，该失败证据保留，不计入通过。

静止画面回归：此前按图像年龄判断控制，把五秒没有新帧错误当作不可控制。真实 QUIC 回归先复现失败，再将授权条件改为有效租约连接、输入代次和当前布局，去掉像素年龄判断后通过。手机夹具现在只发送一帧，界面必须在静置六秒后仍可控制，并断言只有一次 ACK。源码测试与日志：`.local/android-controller-static-initial.log`、`.local/android-controller-017-rust.log`、`.local/android-controller-017-ui.log`。不可变 0.1.6 候选在安装包发布前取消并标为已替代，没有重新指向版本标签。

静止画面的最终界面回归已通过：手机对端只发一帧（一次 ACK），超过六秒后仍能完成导航与旋转触控；12 项原生及完整 Clippy 通过。证据：`.local/android-controller-017-ui.log`、`.local/android-controller-017-rust.log`、`.local/android-controller-017-clippy.log`。

android-v0.1.7 / code 8 已从源码 23823342de199c30f0ecad9ce8391a13d6a03db3 发布。流水线 41569036476 的构建、测试、签名及发布最终成功。首次发布通道比较失败，本地首次固定地址读取还出现一次 TLS EOF；保持正常 TLS 重试后，固定通道与签名版本清单逐字节一致，仅重试失败的发布任务。4 个版本附件的 ID、摘要、大小和创建 / 更新时间保持一致。匿名完整下载、API 摘要、Ed25519 元数据、原始 APK 证书、非调试 manifest、控制端 Activity、manifest / dex 无 debug provider、双 ABI ELF LOAD 及 APK 16 KB 对齐全部通过。APK SHA-256：d1f51353326d77a0c217c160af13b7687430624681209cd7b11a2b865020c7d3。证据：`.local/android-public-0.1.7/final-verification.json`、`native-alignment.json`、`immutable-assets-retry.json`。

实体手机从 0.1.4 / code 5 覆盖升级为 0.1.7 / code 8 返回 Success。包管理器版本、已安装 base.apk 哈希匹配公开正式包，首次安装时间保留，没有卸载或清除数据，MainActivity 启动成功。手机仍锁屏，未新增实体 Windows / 安卓设备跨网控制验收。0.1.6 候选的安装包工作流取消，原源码标签保留，版本附件数量为零。
