[English](WI-ANDROID-BACKGROUND-042.md) | **简体中文**

# 安卓锁屏与后台设置快捷入口

日期：2026-11-05。基于安卓 0.1.12 / code 13 的未发布改动；手机已安装的公开 0.1.12 尚不包含此引导。

## 行为

设置、共享设置及已绑定设备首页提供「锁屏与后台设置」。标准电池豁免开启后首页入口仍可使用，因为该状态不代表厂商后台权限已放行。页面显示手机实际型号 / 安卓版本，区分保持在线与画面、控制可用。

电池豁免使用原有安卓标准授权页面，返回后读取实际状态。应用后台设置打开 FarSail 的系统应用详情。OPPO / OPlus / OnePlus / realme 另显示带适用条件的 ColorOS 菜单说明和手机管家启动入口，仅查询两个厂商启动包，不硬编码私有 Activity。未安装或禁止打开时依次回退到应用详情、系统设置；所有入口都不可用时提示手动路径，不崩溃。跳转不会自动授权或修改用户已有偏好。

安卓 8–10 明确保留系统录屏确认，不提供安卓 11+ 才有的辅助功能截图方式。锁屏可能限制画面或中断录屏；设备在线和重新连接不能证明画面、控制仍可用。既有共享防自动熄屏仍受原有八小时上限约束，不绕过手动安全锁屏。安卓 11+ 引导到可选、用户明确授权的截图方式，保留保护页面及厂商限制说明。共享页也不再把新安卓的锁屏录屏撤权行为一概套用到安卓 10。

## 验证

证据：`.local/android-background-settings-20261105/`。debug / 测试 APK、JVM 与 Android lint 通过。首轮专用 API 35 模拟器四项后台 / 导航测试通过，含真实系统授权和应用详情跳转、按版本显示说明、返回导航、电池 / 控制 / 在线选择保持、模拟入口缺失 / 禁止时回退。最终源码在专用 API 29（安卓 10）及 API 35 模拟器各通过四项测试（`api29-final-tests.log`、`api35-accepted-tests.log`），JVM 九项、最终构建与 lint 通过。首轮 API 29 用例在未检查滚动容器存在前就尝试滚动，失败保存在 `api29-tests.log`；用例现等待设置内容就绪，仅在容器存在时滚动。该测试失败没有要求修改产品逻辑。

OPPO R17 是用户暂时回忆的型号，不是已测试设备。没有 R17 / ColorOS 或整夜实体保活通过结论。没有发布新安装包，也没有用此次 debug 包替换个人手机已装的 0.1.12。

依据：[安卓设置 Intent](https://developer.android.com/reference/android/provider/Settings)、[API 30 引入辅助功能截图](https://developer.android.com/reference/android/accessibilityservice/AccessibilityService)、[Doze 豁免与限制](https://developer.android.com/training/monitoring-device-state/doze-standby)。
