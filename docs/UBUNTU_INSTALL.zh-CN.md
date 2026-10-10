[English](UBUNTU_INSTALL.md) | **简体中文**

# Ubuntu 桌面客户端

Ubuntu 客户端提供账号、设备和连接界面，并复用查看端协议连接 Windows 被控电脑。暂不支持共享 Ubuntu 本机屏幕或远程值守；设置页和本机设备面板会说明这一限制。当前源码候选可通过独立批准的[文件连接](FILES.zh-CN.md)发送与接收单个文件，旧版已发布安装包不包含这一新增能力。Windows 采集与输入保持原有实现。

## 从源码运行

需要已登录的 Ubuntu 桌面、解锁的登录密钥环、Rust 1.93.0 和 Node/npm。Ubuntu CI 的构建目标为 24.04；本机原生检查在 26.04.1 上执行。更早的 Ubuntu 和 ARM 构建尚未验证。Tauri 在 Linux 上使用 WebKitGTK 4.1，参见[官方前置要求](https://v2.tauri.app/start/prerequisites/)。

```bash
sudo apt update
sudo apt install build-essential curl pkg-config libwebkit2gtk-4.1-dev \
  libgtk-3-dev librsvg2-dev patchelf gnome-keyring fonts-noto-cjk pkexec
npm ci
npm run tauri -w @farsail/desktop -- dev
```

在设置页配置协调服务地址，登录并添加这台电脑。设备绑定使用平台 `linux`，默认名称读取系统主机名。选择已开启共享的在线 Windows 设备，申请查看或控制，获得批准后连接。Linux 应用也可使用已有的自建协调服务，控制端桌面无需 Docker。

协调服务也需要升级以接受 `linux`；启动时应用新增的 `20261009000000_linux_platform.sql` 迁移，不修改历史迁移校验和。Linux 设备不能声明屏幕被控能力。当前源码候选需单独开启文件接收，才会声明文件能力；协调服务也需升级以接受 Linux 文件能力，并独立于屏幕共享逐次批准文件连接。

首次使用需要添加一次本机。再次打开客户端或退出后登录同一账号，会自动恢复已有设备连接并立即更新在线状态，保留设备 ID 与名称；退出仍会撤销旧登录与设备令牌。设备身份按服务器和账号隔离，已停用或已解绑的设备不会自动恢复。密钥环或原配置目录被清空、以及旧客户端退出后已丢失绑定记录的情况，仍需添加一次；Ubuntu 在线仅表示客户端已连接，不表示支持共享本机屏幕。恢复失败时会提示原因，可以检查网络后刷新重试。

## 构建 Debian 安装包

```bash
npm run tauri -w @farsail/desktop -- build --bundles deb \
  --config '{"bundle":{"createUpdaterArtifacts":false}}'
sudo apt install ./target/release/bundle/deb/*.deb
```

Tauri 自动合并 `apps/desktop/src-tauri/tauri.linux.conf.json`，选择 Debian 目标并声明运行库和中文字体依赖。Windows 配置继续选择 NSIS。请在计划支持的最早 Ubuntu 版本上构建：新版发行版生成的包可能依赖较新的系统库。本地安装包属于候选制品，不代表已公开发布。

## 凭据存储

Ubuntu 通过 GNOME Keyring 使用桌面 Secret Service，并根据应用配置目录划分命名空间。账号会话、设备凭据、签名私钥和设置保留在 Rust 与密钥环中，令牌不会进入 React 或浏览器存储。密钥环缺失或不可访问时会返回解锁指引，不回退到明文存储。请在桌面会话中运行，不要使用 root 或系统服务方式启动。

## 验证

```bash
sudo apt install xvfb dbus-x11
bash scripts/test-desktop-linux.sh
```

脚本运行 Rust 测试、Clippy、前端与原生构建，再创建隔离的会话总线、数据目录和密钥环。真实 WebKitGTK 窗口检查 Linux 元数据、不支持的共享入口、工作区边界、设置 IPC 和二进制媒体 IPC。Ubuntu 工作流还会使用 Playwright WebKit 执行账号/设备与查看端的合成回归，并构建 Debian 制品。本地检查不代表 Ubuntu 到 Windows 的物理双机远控、Wayland 会话、Ubuntu 24.04 安装或公开发布已验收。

Ubuntu 绑定返回 422 或需要新的登录提示时，按[现网升级步骤](UPGRADE_UBUNTU_LOGIN.zh-CN.md)先升级协调服务，再安装客户端 0.1.18。
