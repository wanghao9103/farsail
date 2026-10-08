[English](WI-UI-014.md) | **简体中文**

<a id="ui-014--use-the-system-computer-name"></a>

# UI-014 — 使用系统计算机名

- 基线：`024eb05` 加现有未提交的 UI-012/013。保留所有无关文件传输进行中工作。
- 写集：桌面端 `src/main.tsx`、`src-tauri/src/lib.rs`、新增 `src-tauri/src/computer.rs`、`scripts/test-desktop-ui.cjs`、本文档。仅使用现有依赖和忽略的 `.local/ui-verification/` 运行时输出。
- 原生状态提供可选 `computerName`，使用 Windows `GetComputerNameExW(ComputerNamePhysicalDnsHostname)` 获取。原生调用失败返回 null；表单允许手动输入，不虚构系统名称。
- 新绑定默认使用系统名称。刷新时，仅通过现有重命名 API 同步此客户端与历史默认名称完全匹配的名称；自定义别名和其他设备保持不变。迁移失败时保留现有名称，并说明如何重试。每台设备必须运行更新后的客户端，才能报告自己的名称。
- API 契约参考：[GetComputerNameExW](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/nf-sysinfoapi-getcomputernameexw)、[COMPUTER_NAME_FORMAT](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/ne-sysinfoapi-computer_name_format)。

<a id="verification"></a>

## 验证

- `rustc --edition=2024 --test apps/desktop/src-tauri/src/computer.rs -o .local/ui-verification/computer-name-tests.exe` 及其生成的可执行文件在 Windows 上通过。实际原生 API 返回了非空、不含 NUL、稳定的名称；未记录机器身份。
- `npm run build` 通过（TypeScript 和 Vite）。`scripts/test-desktop-ui.cjs` 通过现有回归覆盖并增加：绑定时提交默认计算机名；仅迁移一次本地历史名称；其他使用历史名称的设备保持不变；现有自定义别名保持不变；重命名失败保留原名称并允许手动恢复；null 主机名仍允许手动输入名称。已检查截图 `computer-name.png`。
- `git diff --check` 通过。`rustfmt` 仅针对两个改动的原生文件，跳过子模块。
- `cargo check --locked -p farsail-desktop --lib` 无法继续，因为现有工作区清单/锁文件需要更新锁文件。本次改动不增加依赖，也不改变现有 Cargo/文件传输进行中工作。完整原生集成/安装程序构建尚未验证；未生成安装程序或发布版本。
- 历史迁移仅识别历史上的精确字符串 `这台 Windows 电脑`；未存储名称来源字段。它不会追踪未来的操作系统重命名，也不会覆盖已保存的名称。每个客户端的启动仅负责自己绑定的设备。每次应用运行中，每个账号/服务器/设备尝试一次，避免反复失败的写入。
