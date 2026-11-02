# Android viewer aspect ratio — 0.1.32

The inherited desktop fill preference stretched portrait Android frames across a wide window. Android now has an independent display preference, defaulting to contain. Desktop preferences are retained. Rendering and pointer coordinates use the same existing fit geometry, including rejection of new clicks in letterboxes and input release on fit changes.

After the authenticated Android capability is observed, the viewer adjusts its initial window once, to at most 640×900 logical pixels bounded by the current monitor; later polling and screen rotation do not reset a user-resized window. The native window retains maximize, restore, fullscreen and minimize actions.

Local validation: TypeScript/Vite production build; viewer policy tests; full synthetic Chromium viewer regression including 582×1280 at three viewport sizes, actual screenshot corner colors, normalized clicks, letterbox rejection, rotation to 1280×582 and independent preferences; Rust desktop library tests (50 passed, one owned-Windows test ignored); isolated Xvfb/Openbox native viewer probe including actual phone size (640×900), maximize/restore/fullscreen/minimize and window IPC isolation.

Synthetic images and owned native windows only. No physical Windows/phone visual acceptance or new relay performance measurement is claimed. This desktop change requires no coordinator upgrade. Android lock-screen behavior and Android packages are a separate change.
