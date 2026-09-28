# 遥舟：视频与桌面画面编码全景调研

调研日期：2026-09-28。范围是远程桌面有关的视频、静态图像和图形传输编码，覆盖当前主要标准、专业格式及历史家族；不是所有私有编码、音频编码和文件容器的穷举目录。结论来自官方文档，尚未做遥舟实机性能测试。

## 如何比较

编码标准决定码流语法与解码规则，具体编码器、硬件代际、低延迟参数和画面内容决定实际压缩率及速度。不能把不同论文/厂商测试的节省百分比串联成统一排行榜。

- 比较条件固定为相同输入、分辨率、帧率、色度采样、位深和延迟预算；同时检查文字可读性与动态画面。
- 同时测量发送端编码、接收端解码、多路屏幕总负载、带宽、操作到画面响应、手机发热耗电。能播放电影不代表能完成低延迟多屏远控。
- “系统支持某格式”不等于具有硬件加速，也不等于支持所有 profile、level 和分辨率。客户端探测后还必须实际创建编解码会话。
- MP4、MKV、MOV、WebM 是容器；RTP、QUIC、WebRTC 是传输协议/技术；x264、x265、SVT-AV1 是编码器实现。它们与下面的编码格式不是同一层。

## 通用视频编码候选

表中“遥舟判断”是结合项目目标作出的工程建议，不是官方跨设备性能排名。

| 编码家族 | 已核实的定位/状态 | 遥舟判断 |
| --- | --- | --- |
| H.264 / AVC / MPEG-4 Part 10 | 广泛应用于视频及远程桌面；Android 与 Apple 生态都有成熟支持 | 保留兼容基线，建立第一条实时视频链路 |
| H.265 / HEVC | 相近视觉质量下通常比 H.264 节省码率；Windows 提供低延迟、码率和关键帧控制接口 | 主要视频候选，硬编/硬解与低延迟都达标时优先 |
| VP8 | Android 等平台已有编码/解码支持，属于 Web 视频生态的成熟格式 | 将来接入浏览器/WebRTC 时可评估；当前原生客户端无优先引入理由 |
| VP9 | Android 等平台已有解码支持，Web 视频生态常用 | 备选，只有目标设备和实测结果优于已选路径时增加 |
| AV1 | 已有消费级硬件编码/解码实现；Android 格式表列出支持，但必须区分软硬实现 | 与 HEVC 对比的重点增强候选，不强制老手机使用软件 AV1 解码 |
| AV2 | AOMedia 已发布 v1.0.0 正式规范（2026-05-28），包含屏幕内容等改进；已提供参考软件 | 纳入跟踪与实验。目前尚未验证目标三端的硬编/硬解链路，不能直接替代首发格式 |
| H.266 / VVC | 标准已发布；ITU 列出的现行第 4 版为 2026-01。目标包括更高压缩效率和更多应用 | 有研究价值，但需要单独证明 Windows→Android/iPhone 的完整实时实现和硬件覆盖 |
| MPEG-5 EVC | 已发布，分 Baseline/Main 等能力；标准制定时同时考虑效率、实时复杂度和商业部署 | 与 HEVC/VVC 一起列为扩展研究项；遥舟尚无已验证的三端链路 |
| MPEG-5 LCEVC | 与现有基础编码配合的增强层，可改善压缩效率/复杂度；并非普通的单独替代编码 | 可实验 HEVC/AVC + LCEVC，但需要额外编解码/合成模块，不默认认为手机原有解码器即可完整解码 |
| AVS / AVS+ / AVS2 / AVS3 | AVS 官方列出相关标准；AVS2/3 覆盖超高清、广播及屏幕混合内容等方向 | 若未来有特定国产设备需求再验证；不把广播/电视芯片支持推断为 iPhone/Android App 可用 |

来源：[Android 格式表](https://developer.android.com/media/platform/supported-formats)、[Apple HEVC 说明](https://support.apple.com/en-ae/116944)、[Windows HEVC 编码接口](https://learn.microsoft.com/en-us/windows/win32/medfound/h-265---hevc-video-encoder)、[NVIDIA 视频编解码 SDK](https://developer.nvidia.com/video-codec-sdk)、[AV2 正式规范](https://av2.aomedia.org/)、[ITU H.266](https://www.itu.int/rec/T-REC-H.266)、[EVC](https://www.mpeg.org/standards/MPEG-5/1/)、[LCEVC](https://www.mpeg.org/standards/MPEG-5/2/)、[AVS 标准目录](https://www.avs.org.cn/index/list?catid=22)。

**新标准边界**：ITU 在 2026 年 7 月启动了超越 VVC 的新一代编码提案征集，目标在 2029 年形成新标准。当前不能把这一研究进程写成已经可部署的“H.267”产品能力。[ITU 公告](https://www.itu.int/hub/2026/07/beyond-vvc-call-for-proposals-on-future-video-coding/)

## 专业、低延迟与无损格式

| 编码/家族 | 主要特点 | 遥舟判断 |
| --- | --- | --- |
| JPEG / Motion JPEG | 每帧独立，原型容易定位问题；不充分利用跨帧重复 | 连通性原型、低频预览可用，高清跨网主链路优先验证帧间视频编码 |
| JPEG XS | 目标是视觉无损、很低延迟、低复杂度的图像/视频传输 | 专用高带宽局域网/KVM 可研究；公网省流目标下先测码率是否合适 |
| JPEG 2000 / Motion JPEG 2000 / HTJ2K | 图像、专业影视、无损/有损及高吞吐方向 | 不作为手机跨网远控首发路径 |
| Apple ProRes / ProRes RAW | 主要用于高质量制作与后期工作流，部分格式数据率很高 | 可用于未来本地录制研究，不作为当前省带宽传输主格式 |
| DNxHD / DNxHR、CineForm、DV、VC-2 | 专业制作、中间格式或传统视频工作流 | 归入专业格式，当前无需引入 |
| APV | 专业高质量录制/编辑；轻量帧内编码，支持高位深、高码率；Android 格式表已列出 | 适合专业制作的特征不等于适合公网省流，首发不选 |
| FFV1、HuffYUV 等无损视频 | 可精确恢复编码前像素，常用于保存或中间处理 | 可作为测试参考数据；不承诺动态高清无损视频能适应窄带公网 |
| Raw RGB / YUV | 无压缩像素 | 保留为内部处理和测试输入，不直接跨公网传输 |

来源：[JPEG XS](https://jpeg.org/jpegxs/documentation.html)、[JPEG 2000](https://jpeg.org/jpeg2000/)、[HTJ2K 白皮书](https://ds.jpeg.org/whitepapers/jpeg-htj2k-whitepaper.pdf)、[Apple 对专业格式数据率的说明](https://developer.apple.com/videos/play/wwdc2019/506/)、[OpenAPV](https://github.com/AcademySoftwareFoundation/openapv)、[FFmpeg 编码支持目录](https://www.ffmpeg.org/general.html)。

历史兼容格式还包括 H.261、H.262/MPEG-2 Video、H.263、MPEG-1 Video、MPEG-4 Part 2（DivX/Xvid 实现）、VC-1/WMV、Theora、早期 VP 系列、RealVideo、Sorenson、Cinepak、Indeo 等。它们仍可能出现在文件或旧设备中，遥舟无需为新建实时链路全部实现。FFmpeg 的目录可用于查询更多具体格式，但某个 FFmpeg 构建的支持列表不能代表手机系统硬件能力。[FFmpeg 官方目录](https://www.ffmpeg.org/general.html)

## 桌面静态区域还可使用哪些图像格式

| 格式 | 遥舟可能用途 | 需要测量的代价 |
| --- | --- | --- |
| PNG | 无损文字区域、小块更新、截图 | 连续大面积变化时的带宽和编码时间 |
| WebP（有损/无损） | 文字/图标区域、小图块或预览候选 | 编码预设、解码 CPU 与区域合成开销 |
| JPEG XL | 有损/无损图像、渐进式清晰化候选 | 三端解码器集成及实际编码耗时，不假设 WebView 已原生支持 |
| AVIF | 使用 AV1 编码的静态图像格式，适合比较静态图像压缩 | 不以静态图片效率推断连续实时视频效率 |
| HEIF/HEIC | HEIF 是图像容器体系，常见 HEIC 使用 HEVC 压缩图像 | 不作为 HEVC 视频流的同义词，也不为了视频逐帧封装 HEIC |

来源：[WebP](https://developers.google.com/speed/webp)、[JPEG XL](https://jpeg.org/jpegxl/)、[AVIF](https://aomedia.org/specifications/avif/)、[Apple HEIF/HEVC](https://support.apple.com/en-ae/116944)。GIF/APNG 等动画图片虽可表达连续画面，也不列为远程桌面主视频通路。

## 对遥舟最有价值的后续实验

1. **硬件视频基线**：先比较 H.264 与 HEVC，再对有硬件支持的组合加入 AV1。覆盖 Intel/AMD/NVIDIA 的目标机器与 Android/iPhone 真机；实际探测能力，不按品牌一刀切。
2. **内容适配**：动态大区域用 HEVC/AV1；静止文字与图标研究变化区域更新、缓存及无损小图块。将它作为基线之后的独立实验，只有整体延迟/带宽/清晰度改善才合入。
3. **混合模式的正确性**：视频与图块共享屏幕布局版本、帧序号和区域所有权；完整更新与增量更新有明确基底，避免旧图块覆盖新视频、滚动后文字残留。额外合成内存、CPU 和两路带宽都计入预算。
4. **新标准跟踪**：AV2、VVC、EVC、LCEVC、AVS3 以“可运行实现 + 实际设备 + 延迟与能耗达标”为启用条件。规范发布或某厂商宣称支持只作为候选依据。

Microsoft RDP 的官方设计已经展示文字、图片和视频混合编码，并说明以文字为主时全屏视频编码可能不如混合模式。这支持开展上述内容适配实验，但不是遥舟实现必然获得同等收益的证明。[Microsoft 远程桌面图形编码](https://learn.microsoft.com/en-us/azure/virtual-desktop/graphics-encoding)

现阶段建议：视频主路径保留 HEVC + H.264，加入 AV1 的能力协商与实验接口；扩展层预留图块更新和后续 codec。所有策略共同遵守总带宽上限、输入优先、参考帧完整性与有界队列。首发不因“支持更多格式”而增加未经验证的运行路径。
