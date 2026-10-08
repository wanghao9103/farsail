[English](README.md) | **简体中文**

<a id="farsail-icon-assets"></a>

# FarSail 图标资产

日期：2026-09-28。生成方式：内置 image_gen；平台尺寸由本机 Tauri CLI 导出。当前推荐候选为 v2，尚未作为已发布客户端标识使用。

<a id="files"></a>

## 文件

- `farsail-icon-master-v2.png`：第二版原始栅格图。深青底、象牙白折帆、薄荷色开放窗口、琥珀色端点；没有字标。
- `farsail-v2/icon.ico`：Windows 图标。
- `farsail-v2/32x32.png`、`64x64.png`、`128x128.png`、`128x128@2x.png`：常用尺寸。
- `farsail-v2/ios/`、`farsail-v2/android/`：Tauri 导出的移动平台资源。
- `farsail-icon-v2-prompt.txt`：第二版完整生成提示词。
- `farsail-icon-master-v1.png` 与 `farsail-v1/`：保留的首版双帆船草案，不推荐直接定稿。

主图为不透明正方形，图形保留系统图标遮罩空间。尚未实现客户端，平台配置、Android 自适应前景/背景和托盘单色版本应在接入时再验证。生成的 PNG 是栅格文件，不是可编辑矢量源。

<a id="preliminary-similarity-screening"></a>

## 相似性初筛

方法：公开网页/图片关键词检索，以及实际查看候选应用图标。检索了帆船应用、蓝底白青双帆、抽象折帆窗口及远程桌面软件标识。未上传项目图片进行反向搜图，未完成商标数据库图形检索，结论仅覆盖检索到的样本。

首版发现值得避开的案例：

- [Smartboatia 的应用商店条目](https://apps.apple.com/kr/app/smartboatia/id1615378112)；通过检索返回的 [Apple 图标资源](https://is1-ssl.mzstatic.com/image/thumb/Purple116/v4/16/43/1f/16431f2e-baf8-2fc2-7b0d-29f925203355/AppIcon-1x_U007emarketing-0-10-0-85-220.png/1200x630wa.png) 实际查看：深蓝底、白/浅蓝双帆，下部为电路式线条。条目在本地浏览器会跳转到区域商店首页，视觉比对依据是图标资源本身。
- v1 与它不是相同图像：帆的左右颜色、曲线、船体结构均不同；但“双帆＋蓝底＋白青配色”的整体识别组合接近，因此不推荐把 v1 直接作为最终品牌图标。

v2 调整为单片抽象折帆、折角窗口与方形端点，改变主轮廓、元素数量和配色关系；没有沿用上述双帆船组合。第二轮关键词检索未定位到与该组合明显一致的样本，但这不能证明全球不存在相似标识，也不是商标注册或侵权结论。

<a id="regenerating-platform-assets"></a>

## 再生成平台资源

在项目根目录运行：

```powershell
cargo tauri icon 'assets/branding/farsail-icon-master-v2.png' --output 'assets/branding/farsail-v2'
```

后续视觉调整应保存为新的版本，不覆盖原始候选与审查记录。
