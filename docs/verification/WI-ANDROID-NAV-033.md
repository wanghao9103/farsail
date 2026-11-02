**English** | [简体中文](WI-ANDROID-NAV-033.zh-CN.md)

# Phone navigation — 0.1.33

Back, Home and Recents were hidden inside More actions. They now remain visible below the phone image, including fullscreen. The bar reserves its own space and preserves fitted image geometry and pointer coordinates. View-only, paused, disconnected or unavailable control cannot dispatch navigation. Android without switchable media profiles or file transfer no longer displays a misleading update requirement for those absent capabilities; desktop behavior is retained.

The existing authenticated key pairs and Android accessibility global actions are reused. No Android app or coordinator upgrade is needed for these navigation buttons.

Local TypeScript/Vite build and full Chromium viewer regression passed: three navigation key pairs, view-only denial, keyboard Home activation without leaking Enter, hidden toolbar, fullscreen, portrait rotation, screenshot corner pixels, fitted clicks and letterbox denial at 360×640, 640×900, 1200×800 and 1920×1080. The local bundled WebKit executable was unavailable; browser-specific WebKit regression remains unverified. Native Linux package checks do not replace it. No new physical-device navigation acceptance is claimed.

Desktop client-v0.1.33 is published from source 018cee4ed32e178decd9ed5ef68128820e154e97. Workflow 41422438784 completed successfully for Windows x64 and Ubuntu amd64/arm64. All 12 public assets were downloaded anonymously and verified against source reconstruction, API digests, checksums and updater signatures; the canonical feed matched. The publish-only retry retained immutable asset IDs, digests and timestamps. Evidence: `.local/client-public-0.1.33/verification.json` and `immutable-assets-retry.json`.
