# Windows preview 0.1.6 — ordinary remote pointer

- Baseline `c72d76a`, clean current managed checkout; branch `codex/viewer-cursor-016`.
- User clarified the crosshair merely looks non-interactive, not an observed control failure. The CSS crosshair is replaced with a normal arrow; paused control has a not-allowed pointer and an explanatory tooltip.
- Scope: pointer CSS, viewer tooltip/state style, release version and docs.
- Runtime: current checkout caches, owned foreground test only, loopback Vite/QUIC, disposable Windows CI. Primary WIP and production profile unchanged.

## Evidence
Pending build/browser verification and release.
