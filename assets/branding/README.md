**English** | [简体中文](README.zh-CN.md)

<a id="farsail-图标资产"></a>

# FarSail icon assets

Date: 2026-09-28. Generation: built-in image_gen; platform sizes exported by the local Tauri CLI. The currently recommended candidate is v2, which has not yet been used as the identity of a released client.

<a id="文件"></a>

## Files

- `farsail-icon-master-v2.png`: the second original raster image. Dark teal background, ivory folded sail, mint open window and amber endpoint; no wordmark.
- `farsail-v2/icon.ico`: Windows icon.
- `farsail-v2/32x32.png`, `64x64.png`, `128x128.png`, `128x128@2x.png`: common sizes.
- `farsail-v2/ios/`, `farsail-v2/android/`: mobile-platform assets exported by Tauri.
- `farsail-icon-v2-prompt.txt`: the complete generation prompt for the second version.
- `farsail-icon-master-v1.png` and `farsail-v1/`: the retained first two-sail boat draft, not recommended for direct finalization.

The master image is an opaque square, with space left for system icon masks. The client has not yet been implemented; platform configuration, Android adaptive foreground/background and a monochrome tray version should be verified during integration. Generated PNGs are raster files, not editable vector sources.

<a id="相似性初筛"></a>

## Preliminary similarity screening

Method: public web/image keyword searches and actual inspection of candidate application icons. Searches covered sailboat apps, white/cyan twin sails on blue, abstract folded-sail windows and remote-desktop software identities. Project images were not uploaded for reverse-image searching, and graphical trademark-database searching was not completed. The findings cover only the samples found.

The first version revealed a case worth avoiding:

- [Smartboatia's App Store listing](https://apps.apple.com/kr/app/smartboatia/id1615378112); the [Apple icon asset](https://is1-ssl.mzstatic.com/image/thumb/Purple116/v4/16/43/1f/16431f2e-baf8-2fc2-7b0d-29f925203355/AppIcon-1x_U007emarketing-0-10-0-85-220.png/1200x630wa.png) returned by the search was actually inspected: dark blue background, white/light-blue twin sails and circuit-like lines below. The listing redirects to the regional store homepage in the local browser, so the visual comparison is based on the icon asset itself.
- v1 is not the same image: the sails' left/right colors, curves and hull structure differ. However, the overall recognizable combination of “twin sails + blue background + white/cyan palette” is similar, so v1 is not recommended directly as the final brand icon.

v2 uses a single abstract folded sail, an angled window and a square endpoint, changing the primary silhouette, number of elements and color relationships. It does not retain the twin-sail boat combination above. The second keyword-search round did not locate a sample clearly matching this combination, but that does not prove that no similar identity exists worldwide and is not a conclusion about trademark registration or infringement.

<a id="再生成平台资源"></a>

## Regenerating platform assets

Run from the project root:

```powershell
cargo tauri icon 'assets/branding/farsail-icon-master-v2.png' --output 'assets/branding/farsail-v2'
```

Save subsequent visual changes as new versions without overwriting original candidates or review records.
