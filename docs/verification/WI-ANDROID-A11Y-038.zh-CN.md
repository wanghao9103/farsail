[English](WI-ANDROID-A11Y-038.md) | **简体中文**

# 辅助功能截图共享

日期：2026-11-04。基线：`b427d00`。普通安装候选：`0.1.9-a11y-preview2`，code 10，未发布公开通道。

## 机制与选择

Android 11 以上可由用户选择 `AccessibilityService.takeScreenshot`，服务元数据声明 `canTakeScreenshot`。默认仍为系统录屏，用户阅读截图、耗电和保护页面说明后明确启用，再按需在系统设置授权。目标采集间隔 350 毫秒，包含采集、复制和编码耗时，系统限流时自动降频；实际截图成功后才发布共享能力，使用独立的 `connectedDevice` 前台任务和可停止通知，不创建 `mediaProjection` 会话。依据：[辅助功能截图 API](<https://developer.android.com/reference/android/accessibilityservice/AccessibilityService#takeScreenshot(int,%20java.util.concurrent.Executor,%20android.accessibilityservice.AccessibilityService.TakeScreenshotCallback)>)。

保存的方式与共享选择按服务器 / 设备隔离。仅开启辅助功能不开始共享或自动批准；直接控制仍须独立选择。退出、主动停止和身份变化不得从旧意图恢复；权限被撤销时停止采集并即时撤销本地能力，权限恢复后回到前台可重试。恢复截图不弹系统录屏对话框。

硬件截图复制到软件 RGBA，受 2560 像素边长上界约束，复用原生全分辨率 JPEG 4:4:4 编码；显示布局和采集代次在发布时原子核对，迟到帧不能改变新会话几何或向新身份发布。截图缓冲始终关闭，旧回调和停止后的帧被拒绝。既有系统录屏路径保留。

熄屏唤醒由普通窗口 API 完成，入口不导出，启动时重新检查当前截图共享、辅助功能和原生控制会话的采集 / 布局代次。首次操作只唤醒，不重放坐标；窗口自动退出，不请求解除密码锁。仅取得查看权限不能触发该路径。

`onInterrupt` 表示中断辅助功能反馈，不等同于撤销权限。现在只清理手势，不暂停仍有效的控制或截图；实际权限关闭及服务解绑仍撤销能力。依据：[辅助功能生命周期](https://developer.android.com/reference/android/accessibilityservice/AccessibilityService#onInterrupt())。

## 验证证据

- 原始 API 探针使用专用 API 36 模拟器与合成纯色 Activity，未创建录屏会话：正常、熄屏安全锁屏、唤醒但仍锁屏及再次解锁均取得截图；`FLAG_SECURE` 页面为黑色。探针不是远程输入或实体手机兼容性验收。
- 原生测试覆盖旧代次帧无法修改新采集布局或发布；构建、共享生命周期及回归用例结果记录在 `.local/android-lock-followup-20261104/verification.json`，探针原始输出在 `.local/android-lock-followup-20261104/accessibility-probe-test-final.log`。
- 第一轮探针的解锁夹具和独立 debug 服务 lint 失败保留。后续使用已声明的控制服务和可见合成 Activity；只清除该用例创建的测试凭据，不触及实体锁屏、账号或权限设置。

- Rust：Android 17 项与媒体 7 项、Clippy 和格式检查通过；JVM 7 项通过。debug/release 构建、lint、原签名证书、双 ABI 符号与 16 KB 对齐已检查；正式 APK 不含调试探针。
- API 36 回归通过：原始截图及保护页面探针；截图生命周期覆盖旋转、锁屏、真实撤权、前台恢复、主动停止及身份范围；旧录屏及控制状态。最新反馈中断与控制回归共 4 项通过。
- 独立认证 QUIC 控制者在专用模拟器内解码原生画面，唤醒熄屏设备且安全锁保持；夹具在本地解开自己创建的测试凭据后，文字和点击到达自有页面。这不覆盖主机 UDP/NAT：API 36 转发失败，改为来宾内探针；用就绪握手替换了不稳定的固定等待与焦点夹具。
- 实体 PJE110 用户在 preview2 之前确认前台有画面且可操作。preview2 覆盖安装保留数据；15:25 实际截图后端发布 1080×2376 画面，采集 23 毫秒、原生编码 51 毫秒、间隔 350 毫秒。这是处理耗时样本，不是端到端延迟测量。
- 实体厂商耗电管理在标准电池优化已豁免时仍选择智能优化后台运行。通过普通系统界面改为完全允许后台行为并确认；辅助功能系统页说明启用服务会独立授予自启动权限。闲置观察前已恢复临时 USB 保持亮屏为原值关闭，未改密码或熄屏超时。15:26 实体 USB 设备消失，最后心跳为 capture=false，原因未确定。长时间闲置及实体锁屏恢复待验证。

## 边界

可截图不等于可绕过设备密码或保护页面。厂商后台停止和网络限制仍可能中断；此前实体省电豁免后的在线失败记录不因新增后端而自动转为通过。Android 8–10 继续系统录屏。实体厂商兼容性、长时间拔线、锁屏远程输入及生产发布分别记录，未验证前不作通过承诺。
