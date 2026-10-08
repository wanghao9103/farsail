**English** | [简体中文](CODEC_SURVEY.zh-CN.md)

<a id="遥舟视频与桌面画面编码全景调研"></a>

# FarSail: Comprehensive Survey of Video and Desktop Image Codecs

Survey date: 2026-09-28. This covers video, still-image, and graphics-transport coding relevant to remote desktops, including major current standards, professional formats, and historical families. It is not an exhaustive catalog of proprietary codecs, audio codecs, or file containers. Conclusions come from official documentation; FarSail has not performed hardware performance tests yet.

<a id="如何比较"></a>

## How to Compare

A coding standard defines bitstream syntax and decoding rules. Actual compression ratio and speed depend on the encoder, hardware generation, low-latency settings, and image content. Savings reported in different papers/vendor tests cannot be combined into a single ranking.

- Keep input, resolution, frame rate, chroma subsampling, bit depth, and latency budget identical; check both text legibility and motion.
- Measure sender encoding, receiver decoding, total multi-monitor load, bandwidth, input-to-picture response, and phone heat/power use together. Movie playback capability does not establish low-latency multi-monitor remote control.
- “The system supports this format” does not establish hardware acceleration or support for every profile, level, and resolution. Clients must actually create codec sessions after probing.
- MP4, MKV, MOV, and WebM are containers; RTP, QUIC, and WebRTC are transport protocols/technologies; x264, x265, and SVT-AV1 are encoder implementations. They belong to different layers from the coding formats below.

<a id="通用视频编码候选"></a>

## General-purpose Video Candidates

“FarSail assessment” in the table is an engineering recommendation based on project goals, not an official cross-device performance ranking.

| Codec family                 | Verified positioning/status                                                                                                                      | FarSail assessment                                                                                                                                       |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| H.264 / AVC / MPEG-4 Part 10 | Widely used for video and remote desktops; mature support in Android and Apple ecosystems                                                        | Retain as the compatibility baseline and establish the first real-time video path                                                                        |
| H.265 / HEVC                 | Usually saves bitrate relative to H.264 at similar visual quality; Windows offers low-latency, bitrate, and keyframe controls                    | Primary video candidate; prefer when hardware encoding/decoding and low latency meet requirements                                                        |
| VP8                          | Encoding/decoding supported on platforms such as Android; a mature Web-video format                                                              | Evaluate for future browser/WebRTC integration; no priority reason to add it to current native clients                                                   |
| VP9                          | Decoding supported on platforms such as Android; common in Web video                                                                             | Alternative, added only if target-device measurements outperform selected paths                                                                          |
| AV1                          | Consumer hardware encoders/decoders exist; Android’s format table lists support, but software and hardware implementations must be distinguished | Key enhancement candidate for comparison with HEVC; do not force software AV1 decoding on older phones                                                   |
| AV2                          | AOMedia released the v1.0.0 specification (2026-05-28), including screen-content improvements, with reference software available                 | Track and experiment. Hardware encoding/decoding paths across the three target platforms remain unverified, so it cannot directly replace release codecs |
| H.266 / VVC                  | Released standard; the current 4th edition listed by ITU is dated 2026-01, targeting greater efficiency and more applications                    | Worth researching, but a complete real-time Windows→Android/iPhone implementation and hardware coverage require separate proof                           |
| MPEG-5 EVC                   | Released, with Baseline/Main capabilities; standardization considered efficiency, real-time complexity, and commercial deployment                | Extended research alongside HEVC/VVC; FarSail has no verified three-platform path yet                                                                    |
| MPEG-5 LCEVC                 | An enhancement layer used with existing base codecs to improve compression efficiency/complexity, not an ordinary standalone replacement codec   | Experiment with HEVC/AVC + LCEVC, requiring additional codec/composition modules; do not assume existing mobile decoders can decode it fully             |
| AVS / AVS+ / AVS2 / AVS3     | AVS lists the related standards; AVS2/3 cover UHD, broadcasting, and mixed screen content                                                        | Validate if specific domestic-device requirements arise later; broadcast/TV chip support does not establish availability to iPhone/Android apps          |

Sources: [Android format table](https://developer.android.com/media/platform/supported-formats), [Apple HEVC explanation](https://support.apple.com/en-ae/116944), [Windows HEVC encoding interfaces](https://learn.microsoft.com/en-us/windows/win32/medfound/h-265---hevc-video-encoder), [NVIDIA Video Codec SDK](https://developer.nvidia.com/video-codec-sdk), [released AV2 specification](https://av2.aomedia.org/), [ITU H.266](https://www.itu.int/rec/T-REC-H.266), [EVC](https://www.mpeg.org/standards/MPEG-5/1/), [LCEVC](https://www.mpeg.org/standards/MPEG-5/2/), and [AVS standards catalog](https://www.avs.org.cn/index/list?catid=22).

**New-standard boundary**: in 2026-07, ITU launched a call for next-generation coding proposals beyond VVC, targeting a new standard in 2029. This research process cannot currently be presented as a deployable “H.267” product capability. [ITU announcement](https://www.itu.int/hub/2026/07/beyond-vvc-call-for-proposals-on-future-video-coding/)

<a id="专业低延迟与无损格式"></a>

## Professional, Low-latency, and Lossless Formats

| Codec/family                            | Main characteristics                                                                                                                              | FarSail assessment                                                                                                             |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| JPEG / Motion JPEG                      | Independent frames simplify prototype debugging; do not fully exploit cross-frame repetition                                                      | Useful for connectivity prototypes and low-frequency previews; prioritize inter-frame video for the HD cross-network main path |
| JPEG XS                                 | Targets visually lossless image/video transport with very low latency and complexity                                                              | Research for dedicated high-bandwidth LAN/KVM; first measure whether bitrate suits public-network data-saving goals            |
| JPEG 2000 / Motion JPEG 2000 / HTJ2K    | Image, professional film/video, lossless/lossy, and high-throughput applications                                                                  | Not a release path for cross-network mobile remote control                                                                     |
| Apple ProRes / ProRes RAW               | Primarily high-quality production/postproduction; some formats have very high data rates                                                          | Possible future local-recording research, not the current bandwidth-saving transport format                                    |
| DNxHD / DNxHR, CineForm, DV, VC-2       | Professional production, intermediate formats, or traditional video workflows                                                                     | Professional formats with no current need for integration                                                                      |
| APV                                     | Professional high-quality recording/editing; lightweight intra-frame coding with high bit depth/bitrate; already listed in Android’s format table | Suitability for professional production does not establish suitability for public-network data savings; exclude from release   |
| Lossless video such as FFV1 and HuffYUV | Exactly restores pre-encoding pixels, often for preservation/intermediate processing                                                              | Useful test reference data; no promise that dynamic HD lossless video fits narrowband public networks                          |
| Raw RGB / YUV                           | Uncompressed pixels                                                                                                                               | Retain for internal processing/test input; do not transmit directly across public networks                                     |

Sources: [JPEG XS](https://jpeg.org/jpegxs/documentation.html), [JPEG 2000](https://jpeg.org/jpeg2000/), [HTJ2K white paper](https://ds.jpeg.org/whitepapers/jpeg-htj2k-whitepaper.pdf), [Apple explanation of professional-format data rates](https://developer.apple.com/videos/play/wwdc2019/506/), [OpenAPV](https://github.com/AcademySoftwareFoundation/openapv), and [FFmpeg codec support catalog](https://www.ffmpeg.org/general.html).

Historical compatibility formats also include H.261, H.262/MPEG-2 Video, H.263, MPEG-1 Video, MPEG-4 Part 2 (DivX/Xvid implementations), VC-1/WMV, Theora, early VP families, RealVideo, Sorenson, Cinepak, and Indeo. They may still appear in files or older devices, but FarSail need not implement all of them for new real-time paths. The FFmpeg catalog helps find additional formats; a particular FFmpeg build’s support list does not represent mobile system hardware capabilities. [Official FFmpeg catalog](https://www.ffmpeg.org/general.html)

<a id="桌面静态区域还可使用哪些图像格式"></a>

## Image Formats for Static Desktop Regions

| Format                | Potential FarSail use                                                       | Costs to measure                                                                                          |
| --------------------- | --------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| PNG                   | Lossless text regions, small updates, screenshots                           | Bandwidth and encoding time under continuous large-area changes                                           |
| WebP (lossy/lossless) | Text/icon regions, small tiles, or previews                                 | Encoder presets, decode CPU, and region composition cost                                                  |
| JPEG XL               | Lossy/lossless images and progressive sharpening                            | Decoder integration on all three platforms and actual encoding time; do not assume native WebView support |
| AVIF                  | Still images coded with AV1, useful for still-image compression comparisons | Do not infer continuous real-time video efficiency from static-image efficiency                           |
| HEIF/HEIC             | HEIF is an image-container system; common HEIC uses HEVC for images         | Not synonymous with an HEVC video stream; do not wrap every video frame in HEIC                           |

Sources: [WebP](https://developers.google.com/speed/webp), [JPEG XL](https://jpeg.org/jpegxl/), [AVIF](https://aomedia.org/specifications/avif/), and [Apple HEIF/HEVC](https://support.apple.com/en-ae/116944). Animated images such as GIF/APNG can represent successive pictures, but are also excluded from the primary remote-desktop video path.

<a id="对遥舟最有价值的后续实验"></a>

## Most Valuable Follow-up Experiments for FarSail

1. **Hardware video baseline**: compare H.264 and HEVC first, then add AV1 for hardware-supported combinations. Cover target Intel/AMD/NVIDIA machines and physical Android/iPhone devices; probe actual capabilities rather than applying blanket brand assumptions.
2. **Content adaptation**: HEVC/AV1 for large dynamic regions; research changed-region updates, caching, and lossless small tiles for static text/icons. Treat this as a separate post-baseline experiment, merging only if overall latency/bandwidth/clarity improve.
3. **Hybrid-mode correctness**: video and tiles share layout versions, frame sequence numbers, and region ownership. Full/incremental updates have explicit bases, preventing stale tiles over new video and text remnants after scrolling. Include extra composition memory, CPU, and both paths’ bandwidth in the budget.
4. **Track new standards**: enable AV2, VVC, EVC, LCEVC, and AVS3 only with a runnable implementation, actual devices, and acceptable latency/energy use. A published specification or vendor support claim only justifies candidacy.

Microsoft RDP’s official design already demonstrates hybrid text, image, and video coding and explains that full-screen video may perform worse than hybrid mode for text-heavy content. This supports the experiments above, but does not prove FarSail will necessarily obtain equivalent gains. [Microsoft remote-desktop graphics encoding](https://learn.microsoft.com/en-us/azure/virtual-desktop/graphics-encoding)

Current recommendation: retain HEVC + H.264 on the main video path and add AV1 capability negotiation/experimental interfaces. Reserve extension layers for tile updates and future codecs. All strategies share a total bandwidth ceiling, input priority, reference-frame integrity, and bounded queues. Do not add unverified release paths merely to support more formats.
