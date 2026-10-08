[English](WI-UI-RELEASE-013.md) | **简体中文**

<a id="windows-preview-013--desktop-operations-and-computer-names"></a>

# Windows 预览版 0.1.3 — 桌面操作与计算机名

<a id="release-baseline-and-boundaries"></a>

## 发布基线与边界

- 基线：`5f8fb54`（远程 main，已验证的 0.1.2）。隔离分支 `codex/desktop-ui-013` 位于受管理的 desktop-ui-release 工作树。
- 从主检出整合 UI-012/013/014。保留 0.1.2 原生独立查看器、限定作用域的 IPC、输入取消、重连/租约行为和明确的同账号远程监视控件。
- 排除主检出的 Rust/Cargo/文件传输进行中工作。不要修改其文件或发布它。无需更新服务器。
- 写集：桌面前端、原生计算机元数据模块及状态端点、Tauri 版本、桌面 UI 测试和文档。现有 0.1.2 运行时代码仍是权威依据。
- 运行时：工作树的 node_modules/target/.local 和本地 Vite 1420；使用临时 Windows CI 验证真实安装程序生命周期。不要覆盖用户真实应用/配置进行安装。

<a id="verification-and-release-evidence"></a>

## 验证与发布证据

- 本地 `npm run build`、`cargo fmt --all -- --check` 和 `git diff --check` 通过。
- 整合原生窗口流程后，两套浏览器回归均通过。主 UI 覆盖本地限定的名称迁移/自定义别名/回退、清晰操作、邀请跨页保留及退出登录时清除、一步共享、恢复失败/重试、明确的远程监视开/关、桌面/紧凑尺寸下的列表滚动。查看器套件保留焦点/输入、终止帧清除、排队输入取消、重连取消和只读权限检查。
- 子查看器继续使用限定作用域的 `viewer_window_action`/`viewer_reconnect`；它不调用主窗口账号 API。新增计算机元数据仍保留 `viewer::main_only` 对 `state` 的限制。
- 历史 UI-012/013/014 文档描述各自原始基线。0.1.3 发布保留独立的 0.1.2 查看器；它取代早先内嵌查看器的布局/结束流程说明。下方 CI 构建和安装程序证据将取代原始脏检出的 Cargo 锁文件限制。
- 首次安装程序运行 `36527204879` 在退出登录状态的共享状态无障碍断言处停止。紧凑侧栏 CSS 在运行器有效视口宽度下隐藏了状态。状态现在仍以圆点显示，并带明确的无障碍名称/工具提示；浏览器测试断言其在全部三个桌面尺寸下可见。原断言得到保留。未发布该失败运行的任何产物。

<a id="published-release"></a>

## 已公开发布的版本

- 实现：`0987d424f1ee3ff71f6dccc9ec145712cd5607b9`；最终紧凑状态修复及发布源码：`f2f5c6b4a4dff913c43bd1ff6e8c043eb204a6df`。
- [Windows 客户端 CI 36527793606](https://github.com/wanghao9103/farsail/actions/runs/36527793606)：成功，包括原生查看器作用域/生命周期、两套浏览器测试、Rust 测试和 Clippy。
- [安装程序 CI 36527795316](https://github.com/wanghao9103/farsail/actions/runs/36527795316)：成功，包括锁定发布构建、实际安装/设置 IPC/重启/重装/卸载保留、不包含调试探针、针对性 Rust 测试和 Clippy。早先主检出的锁文件不一致不影响这个干净的发布基线。
- [0.1.3 发布版本](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.3-f2f5c6b) / [Windows x64 安装程序](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.3-f2f5c6b/FarSail_0.1.3_x64-setup.exe)。

| 公开附件                       |    字节数 | SHA-256                                                            |
| ------------------------------ | --------: | ------------------------------------------------------------------ |
| FarSail_0.1.3_x64-setup.exe    | 7,853,825 | `dc8f43cc79af6d1b24757d217bd502653aef9b6e27bd1df258c1a851443ae258` |
| SHA256SUMS.txt                 |        95 | `77fc60845086173af656ef25eabf540bdbb60933f19ac1bad0ba966702720bc5` |
| release-metadata.json          |       651 | `5d522066b682c322612c7faf7401b722d76e29622311dbfb745431dabe18e0c4` |
| installation-verification.json |       422 | `cbb560dbb1c42edb946c3b7d57feeb463ffcdb66bf19f6a6e057db6392638fe0` |

全部四个公开附件均使用未认证的 curl（禁用 curlrc）完整下载，然后通过字节数和 SHA-256 与已测试的 CI 原件比较并匹配。安装报告和元数据绑定到相同的源码/安装程序哈希。安装程序仍未签名。不新增实体双机/跨 NAT 验证声明。

运行时已停止：本任务的 Vite 和浏览器测试均已退出，未触及生产安装/配置或服务器。保留此受管理的发布检出作为当前代码入口；主检出仍停留在旧基线，带有先前 UI 编辑及无关且已暂停的文件传输进行中工作。不要将该过时检出重新发布覆盖本版本。后续 UI/发布工作复用此检出；另行协调无关的文件传输工作。
