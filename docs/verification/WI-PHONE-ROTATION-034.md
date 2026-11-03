**English** | [简体中文](WI-PHONE-ROTATION-034.zh-CN.md)

# Phone display rotation — 0.1.34

The persistent phone bar includes Rotate view, clockwise by 90 degrees per click; More actions restores the original direction. Only the computer image rotates. The phone and its permissions remain unchanged. Rotation belongs to this viewer, including view-only mode. No phone or coordinator update is required.

Swapped image bounds and inverse normalized coordinates align clicks, drags and wheel axes at all four directions. Rotation releases held input. Both contain and fill work; small windows retain all four buttons.

TypeScript/Vite build and complete Chromium viewer regression passed: screenshot corner colors, inverse clicks and drags, wheel directions, letterbox rejection at 360×640 and 1200×800, rotated fill and direction reset, existing fullscreen, permission and input cancellation. Synthetic IPC only; no new physical phone or Windows injection acceptance. Browser WebKit regression remains unverified. Evidence: `.local/phone-rotation-build.log`, `.local/phone-rotation-ui.log`, `.local/ui-verification/viewer-phone-rotated.png`.

Published from source 5d7d978813a713f4f1f90bbb0974eb146d708f8e. Workflow 41560831202 succeeded for all three platforms and publication. Twelve complete anonymous downloads matched API digests, updater signatures, checksums and exact-source reconstruction; the canonical feed matched. The publish-only retry retained immutable asset IDs, hashes and timestamps. The duplicate pending release-triggered run was cancelled. Evidence: `.local/client-public-0.1.34/verification.json` and `immutable-assets-retry.json`.
