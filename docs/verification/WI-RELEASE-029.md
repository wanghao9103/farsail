**English** | [简体中文](WI-RELEASE-029.zh-CN.md)

# RELEASE-029 — publish the combined 0.1.17 Windows preview

The user explicitly requests committing the code and making GitHub build a new version. This extends the previous local-only candidate scope to a scoped source push, GitHub verification/packaging and delivery of the tested preview. Existing GitHub About English text is already verified. Baseline code `be31a02922f43489bc4400e658f436510f15d7aa`; bilingual documentation is finalized from `e0f92ef`. Only the managed mouse-control-022 checkout is written; primary paused WIP, installed applications, profiles and server remain untouched.

## Scope and checks

Commit all previously authorized input, reconnect, UI, notice and documentation changes. Freshly fetched origin/main is `f7d7ddb1ba45322b09e1f93c81f4326a03b467f4` and is an ancestor, so push without force. Version0.1.17 is not yet publicly released. Preserve original0.1.11 release and use a new exact-source tag.

Knowledge bootstrap: package/installed-native/public-file equality are separate verification layers. Package through the existing no-binary-patching GitHub workflow; do not substitute the local candidate. Complete required source CI, installer lifecycle, metadata/hash checks and anonymous downloads with normal verified TLS. No server deployment or image rebuild is necessary. Local browser/native UI automation remains unrun; the user-requested GitHub pipeline uses the repository's existing isolated regression/install jobs.

Preflight found compact sharing status hidden and three ambiguous viewer-test locators. Retain the accessible status dot at narrow widths and scope tests to the intended dialog/empty-state/paragraph. Add21 pure viewer/feedback regressions to client/installer CI, documentation checking and4 fault tests to its own workflow. New test scripts trigger client CI. Python caches are ignored. These narrow release fixes do not widen permissions or change the media protocol.

## Delivery criteria

All relevant GitHub runs must pass at the exact embedded source commit. Download the4 installer artifacts: installer, SHA256SUMS.txt, release-metadata.json and installation-verification.json. Check source, version, installer/application hashes and installation report binding before prerelease publication. Publish English release introduction/notes, then anonymously download all4 public originals and compare bytes/hash. Update both documentation languages with actual evidence; do not claim physical two-computer/multi-display success or complete source-design visual fidelity from synthetic/native CI checks.
