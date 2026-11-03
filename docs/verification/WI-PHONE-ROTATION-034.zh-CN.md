[English](WI-PHONE-ROTATION-034.md) | **简体中文**

# 手机显示旋转 — 0.1.34

常驻手机栏新增旋转画面，每次顺时针旋转 90 度；更多操作可还原方向。只旋转电脑图像，手机本身和权限不变。旋转属于独立查看器，仅查看也可使用。无需更新手机或协调服务。

交换图像边界并逆变换归一化坐标，让四个方向的点击、拖动和滚轮轴与显示一致。旋转前释放已按输入，等比和铺满均可使用；小窗口保留四个按钮。

TypeScript/Vite 构建及完整 Chromium 查看器回归通过：四角截图像素、逆变换点击与拖动、滚轮方向、360×640 和 1200×800 留白拒绝、旋转铺满与还原方向、既有全屏、权限和输入取消。仅合成 IPC，未新增实体手机或 Windows 注入验收；浏览器 WebKit 回归仍未验证。证据：`.local/phone-rotation-build.log`、`.local/phone-rotation-ui.log`、`.local/ui-verification/viewer-phone-rotated.png`。

已从源码 5d7d978813a713f4f1f90bbb0974eb146d708f8e 发布。流水线 41560831202 的三平台构建和发布均成功。12 个完整匿名下载与 API 摘要、更新签名、校验和及原源码重建一致，固定通道一致；仅重试发布时不可变附件的 ID、摘要与时间戳不变。重复触发且尚未运行的流水线已取消。证据：`.local/client-public-0.1.34/verification.json` 和 `immutable-assets-retry.json`。
