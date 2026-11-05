[English](WI-ANDROID-PRESENCE-037.md) | **简体中文**

# 设备在线与录屏生命周期分离

日期：2026-11-04。基线：main 31b9d29。本地候选：`0.1.9-online-fix`，code 10。未修改公开更新通道。

## 实现机制

实体 PJE110 / Android 16 的应用进程仍存活，但采集服务和唤醒锁均不存在，手机已锁屏，用户报告离线。系统最近一次睡眠原因为 power_button，不能据此推断每次所述自动锁屏的唯一原因。源码确认，周期在线心跳只存在于采集服务，录屏结束也会停止续期设备在线状态。

新增独立、不导出的设备连接前台服务，每 15 秒维护已认证端点。原生 presence 会清理过期共享能力，未独立授权时保持采集与自动批准关闭。服务有状态通知、持久化关闭入口和按身份范围恢复的 sticky 路径；每次尝试重新核对原生账号 / 设备，范围不同或凭据被拒绝即停止。退出登录会清除待恢复范围。CPU 唤醒锁以 90 秒上界续期，停止即释放，不唤醒屏幕或解锁。

网络设备连接服务声明 `connectedDevice`、`FOREGROUND_SERVICE_CONNECTED_DEVICE` 和 `CHANGE_NETWORK_STATE`。采集继续使用独立的录屏前台服务，仍需新的系统授权。依据：[Android 前台服务类型](https://developer.android.com/develop/background-work/services/fgs/service-types#connected-device)。

## 验证结果

- Rust Android 16 项、media 7 项通过，包含未恢复采集 / 自动批准的在线续期与无设备凭据拒绝；JVM 7 项以及 debug/release 构建、lint 通过。
- API 36 在线服务 4 项用例通过：真实熄屏 70 秒，超过合成服务的 60 秒在线租约，心跳仍持续；主动关闭停止续期并释放锁；停止录屏保持在线、退出登录停止；按已保存范围重建服务且拒绝异身份；HTTP 403 停止重试。部分断言属于同一个用例。
- 真实安全锁屏用例通过：系统撤销录屏、停止采集、清除共享能力并释放屏幕锁，独立心跳仍继续。既有控制 / 前台恢复 / 授权导航回归 4 项通过；合计执行的 9 项设备用例全部通过，无跳过。
- 保留初轮夹具失败。启动夹具等待 Activity 真正恢复；锁屏夹具先关闭并等待旧的 dead 辅助功能绑定移除，再重新开启，保留真实服务与权限检查。
- 证据：`.local/android-idle-offline-20261104/rust-tests-final.log`、`.local/android-idle-offline-20261104/connection-tests-final.log`、`.local/android-idle-offline-20261104/secure-lock-test-passed.log`、`.local/android-idle-offline-20261104/access-regression-passed.log`。签名、release manifest、不导出的服务、无 debug 探针、双 ABI ELF / ZIP 16 KB 对齐见 `.local/android-idle-offline-20261104/apk-verification.json`。最终安装与实体观察分别记录在 `.local/android-idle-offline-20261104/installation.json` 和 `.local/android-idle-offline-20261104/verification.json`。

## 适用边界

模拟器使用合成协调服务。Sticky 恢复测试覆盖已保存范围的重建路径，不代表所有厂商杀进程策略通过。强行停止、凭据不可用、断网或厂商限制仍可能阻止在线；设备在线不代表锁屏可查看。原有八小时屏幕保护上界未改变。拔线整夜和 Android 8–15 各版本不在本次验证范围内。

## 实体复核更正

首版通过模拟器用例后，实体仍复现锁屏掉线，不能按模拟器结果宣称整机通过。进程和连接前台服务仍活跃，未被冻结；辅助功能仍开启，系统录屏已结束。设备接着电源、标准 Doze 控制器仍 ACTIVE，但连接唤醒锁被系统释放并列入 Restored Wake Locks。该区不属于当前生效唤醒锁；首轮实体采样仅按 tag 匹配得出的持锁布尔值不可信。

第二版 `0.1.9-online-fix2` 增加标准电池优化豁免入口，首页提示一次性设置，设置页展示系统真实豁免状态；不会自动申请、自动同意或循环弹窗。实体用户已在系统页面允许，未通过 shell 修改白名单。连接服务增加无账号信息的标准服务诊断，分别报告最近成功心跳、定时回调时间和工作阶段，避免将进程存活当成在线证据。API 36 系统授权页打开、未同意及返回后状态保持用例通过；第二版构建、lint 和包校验通过。

实体二次观察先取样，再停止应用 Binder 采样至少 130 秒后复核，以免诊断调用干扰休眠。最终结果见 `.local/android-idle-offline-20261104/verification.json`。用户要求普通安装并拒绝 ADB 激活方案；安全锁屏后的免确认远控仍未实现，本工作不能作为该目标完成的证据。桌面源码补充 Android 未共享 / 中断后的准确提示，未发布桌面安装包。

静置复核失败：停止应用 Binder 采样约 147 秒后，实体仍 Dozing、keyguard showing=true，服务 running=true 且 batteryExempt=true，但 heartbeatAgeMs=95984，当前生效唤醒锁列表没有连接锁。Java 的 cpuLeaseHeld=true 不能证明系统仍兑现该锁；不将系统电池豁免当作 OPlus 后台兼容性通过。实体长期在线仍未解决，尚未覆盖厂商后台活动 / 自启动限制与拔线整夜。

## 后续采集后端补充

上述限制描述当时的系统录屏后端与验证状态。后续新增支持普通安装的 Android 11+ 可选辅助功能截图，见[辅助功能截图共享](WI-ANDROID-A11Y-038.zh-CN.md)。模拟器结果不替换本篇实体在线失败记录。现已通过普通设置选择厂商完全允许后台行为，实体长时间闲置及锁屏恢复仍待验证。
