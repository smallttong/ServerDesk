# 服务器统一桌面控制中心（ServerDesk）

一个 Windows 桌面壳程序：把 AstrBot、NapCat、1Panel、哪吒监控，以及任何你自己添加的
Web 管理后台，统一收进一个类似 IDE 的窗口里。**它不重新实现任何管理功能**，只负责
「按配置加载原有 WebUI + 统一的导航/状态/托盘体验」。

## 下载与使用

从 [v0.1.1 Release](https://github.com/smallttong/ServerDesk/releases/tag/v0.1.1) 下载：

| 文件 | 说明 |
| --- | --- |
| `ServerDesk_0.1.1_x64-setup.exe` | 安装版，已包含网络通道程序 |
| `ServerDesk_0.1.1_x64-portable.zip` | 绿色版，解压后运行 `ServerDesk.exe`；请保留同目录的 `serverdesk-channel.exe` |

更新内容见 [CHANGELOG.md](CHANGELOG.md)。

首次使用请阅读 [中文使用说明](使用说明.md) 或 [English User Guide](USER_GUIDE_EN.md)。本项目由 AI 辅助开发，目前作为测试版提供。

系统要求：Windows 10/11 + WebView2 运行时（Win11 自带；Win10 装了新版 Edge 即具备）。

> 绿色版请把 `ServerDesk.exe` 和 `serverdesk-channel.exe` 一起复制到同一个文件夹，
> 否则通道无法启动（程序会提示"找不到通道程序"）。安装包没有这个问题。

## 内置组网通道

程序自带一个**用户态 Tailscale 节点**，服务器不需要暴露公网，系统里也不用装任何 VPN
客户端、不创建虚拟网卡、不需要管理员权限。

```
WebView2（服务窗口）── --proxy-server=socks5://127.0.0.1:PORT ──┐
                                                              ├── tsnet（Go sidecar）── WireGuard ──> tailnet
健康检查（Rust 侧 SOCKS5 CONNECT）──────────────────────────────┘
```

- 每个服务窗口的 WebView2 环境都带 `--proxy-server=socks5://127.0.0.1:端口`，页面里的
  相对跳转、绝对链接、Cookie、WebSocket 全部照常，URL 保持真实地址不变。
- sidecar（`channel/`，Go + `tailscale.com/tsnet`）在本地开一个 SOCKS5 端点，把连接导进
  tailnet；域名由 tailnet 侧解析，所以 MagicDNS 名字（`nas.tailnet-name.ts.net`）可以直接填。
- 健康检查走同一条通道（Rust 侧实现了最小 SOCKS5 客户端），用服务窗口完全一样的路径探测。
- 通道启动时如果还没有授权，会在「设置 → 网络通道」里给出一条 `login.tailscale.com` 授权
  链接，点一次浏览器授权即可；节点身份持久化在 `%APPDATA%\com.serverdesk.app\channel-state`，
  之后免登录。
- 首启默认只在**通道就绪**后才允许打开服务；通道未就绪时点击服务会提示并跳到设置页，
  不会出现白屏或长时间转圈。
- 「设备标签」可以填 `tag:serverdesk`，配合 tailnet ACL 把这个节点限制到指定端口，
  这样即使这台电脑丢失，能访问的范围也是收敛的。

## 连接方式（Connection Manager, v0.2）

每个服务独立选择连接方式，Tailscale 不再是运行前提：

| 模式 | 行为 | 数据目录 |
| --- | --- | --- |
| `tailnet`（私网） | 经内置 tsnet 通道访问，要求通道已授权 | `services-webview` |
| `public`（公网） | WebView2 直接访问，不依赖 Tailscale | `services-webview-public` |
| `failover`（自动切换） | 打开服务前探测私网 TCP 连通性；通道不可用、超时或拒绝连接时改用公网入口 | 跟随选中的入口 |

关键约束：WebView2 的代理参数属于**环境**级别，而环境由用户数据目录决定，所以
私网/公网必须使用两个目录，切换入口时会销毁并用另一 Profile 重建窗口——不能原地改代理。
副作用是同一 origin 在两个模式下有两套独立会话（正常配置下两个入口 origin 不同）。

配置版本已从 v1 迁移到 v2：旧服务默认变成 `tailnet` 模式，原地址作为私网入口，
迁移前自动备份为 `config.json.v1.bak`。若某个私网入口填的是公网 IP（内置通道只路由
tailnet 网段），界面会给出提示，但不会自动改模式。

首次打开公网 HTTP 入口时会显示风险确认；确认记录绑定完整地址，地址改变后需重新确认。

## 凭据保险箱（Vault, v0.2）

- 二级密码至少 8 个字符，经 Argon2id（64 MiB / t=3）派生 KEK，解封随机 DEK；凭据用
  `HKDF(DEK, credential_id)` 派生的独立 CEK 做 AES-256-GCM 加密，AAD 绑定
  `credential_id|service_id|endpoint_id|origin`，密文不能跨记录替换。
- 凭据绑定到具体入口（endpoint），一个入口一条；地址变更后不会自动沿用旧授权。
- 自动登录授权按入口签发，最长 7 天；"仅本次运行"只存在内存，
  跨重启的授权用 Windows DPAPI 保护（防护边界等于当前 Windows 账户）。
- 二级密码不支持修改；只能重置。重置会依次：断开授权 → 清空内存密钥 →
  清除服务 WebView2 的浏览数据 → 删除两个 Profile 目录 → 删除加密文件。
  服务地址、分组、通道身份不受影响。
- 自动登录：服务窗口加载页面、或地址切到 login/auth 路由时，若保险箱里存在
  绑定「该服务 + 该入口 + 该 Origin」的有效授权（且凭据开启了自动登录），会向页面注入
  一段脚本完成填充与提交。锁定状态下同样可用——它走的是授权，不需要解锁保险箱。
  注入脚本自带限制：只在该 Origin 内生效、页面必须有且仅有一个可见密码框、
  优先填它前面最近的一个文本输入框，然后点击文字为「登录 / login / sign in」的按钮。
  因此带验证码、2FA、多因素页面的服务仍需人工完成；「一个 Origin 多个密码框」的表单
  会被跳过而不是乱填。

## 面板登录保护（Auth, v0.2）

可选启用的本机单管理员认证，保护的是 ServerDesk 的操作入口，不替代各服务自己的登录，也不碰 Vault。

- 账号只有一名 administrator，密码至少 8 个字符，以 Argon2id（64 MiB / t=3）哈希保存，独立随机盐；
  `auth.json` 里没有任何明文，密码也不会进入日志。
- 启用/禁用/改密/关闭保护都需要验证当前密码；禁用后用同一密码才能重新启用。
- 忘记密码：启用登录保护时会生成一次性**恢复码**（`XXXX-XXXX-XXXX-XXXX`，只显示一次，
  只保存 Argon2id 哈希）。锁屏上的「忘记密码」输入恢复码后有两种选择：
  - **恢复**：直接登录，管理员账号与密码保持不变；
  - **重置**：关闭登录保护、清空管理员账号，并**清除所有服务在本机的登录状态**
    （两个 WebView2 Profile 的 Cookie / localStorage / IndexedDB 整体删除，两次点击确认）。
  **恢复**保留服务会话；若主认证文件损坏或丢失，会从独立备份还原同一账号。
  **重置**只清账号与服务会话，凭据保险箱、服务配置、通道身份仍然保留。
- 恢复码输错与密码错误共用同一退避计数，不能当作绕过限速的旁路。
- 注意：这是恢复，不是找回。恢复码只对应生成它的那次账号；重新启用会签发新的恢复码。
- 冷启动一律锁定；锁定后服务窗口被 hide，托盘的服务快捷入口不再下发。
- 自动锁定可选 关闭/5/15/30/60 分钟，只有主窗口的可信 IPC 会续期，远程页面无法延长。
- 登录失败 5 次后按指数退避限时（30s 起，最长 15 分钟）。
- 首次启动会明确写入免登录状态；`auth.state` 记录已初始化，`auth.backup.json` 保存独立认证备份。
  `auth.json` 丢失或损坏时保持锁定，可凭恢复码从备份恢复。两者都不可用时，须显式执行
  本机重置：清除服务会话、免密授权和 Tailscale 节点身份，保留服务列表和加密保险箱。
- 这些本地文件能防止误删 `auth.json` 直接放行；它们不构成对同一系统账号下恶意程序的隔离。
- 敏感命令统一走 `auth::require()`：打开/关闭服务窗口、导航、改配置、通道操作、Vault 全部在内。
  认证命令本身只接受 main 窗口且 URL 为 tauri/localhost 的调用方。

与 Vault 的关系：Auth 启用时，完整解锁保险箱仍需要 Vault 自己的二级密码，两者不共享
哈希或密钥；Auth 只额外要求当前处于已认证会话。Vault 的免密授权续期仍然只输二级密码。

## 技术栈

| 层 | 选型 | 说明 |
| --- | --- | --- |
| 桌面框架 | Tauri 2（稳定 API） | 不依赖 `unstable` 多 WebView 特性 |
| 页面渲染 | 系统 WebView2 | Cookie / Session / WebSocket / 上传下载全部交给它 |
| 前端 | Vue 3 + Vite + TypeScript | 侧边栏、首页、设置、工具栏 |
| 样式 | Tailwind CSS 4 + Lucide | 深色 IDE 风格 |
| 后端 | Rust（Tauri 命令） | 配置读写、健康检查、窗口拼接、托盘、开机自启、通道管理 |
| 组网 | Go + `tailscale.com/tsnet` | 用户态 WireGuard 节点 + 本地 SOCKS5 端点 |

## 窗口模型

Tauri 2 稳定 API 不支持「单窗口多 WebView」，所以这里采用**一个服务一个无边框
`WebViewWindow`** 的方案：

- 主窗口（Vue）负责侧边栏、顶栏工具栏、首页、设置页，并把自己的内容区
  `getBoundingClientRect()` 上报给 Rust。
- 点击服务时，Rust 创建一个 `decorations(false)`、`skip_taskbar(true)`、以主窗口为
  owner 的 WebViewWindow，加载服务的原始 URL，并精确贴合到内容区。
- 主窗口移动 / 缩放 / DPI 变化时，Rust 跟随重新定位，视觉上等同于「内嵌页面」。
- 切换服务只隐藏旧窗口、显示新窗口，窗口本身保留 ⇒ 登录态、滚动位置、WebSocket
  都不丢；程序重启后 Cookie 由 WebView2 的用户数据目录持久化。
- 内嵌窗口关掉了 DWM 阴影：带阴影的无边框窗口在 Windows 上仍保留一圈隐形调整边框，
  会让内嵌页比内容区大出约 11px；关闭阴影后窗口矩形与客户区完全对齐（实测 1440×1116）。
- 内嵌页四角用 DWM 圆角（DWMWA_WINDOW_CORNER_PREFERENCE），与主窗口一致；
  并在内容区内缩进 6px（`SERVICE_INSET`），让它看起来是壳内的一张页面卡片，而不是贴边的一整块。
- 主窗口最小化或收进托盘时，服务窗口会被 suspend（隐藏并禁止再次显示），
  即使此时通道重连、托盘事件触发了“重新打开服务”，也不会把窗口弹到桌面上；
  恢复主窗口时再把当前服务贴回去。

## 目录结构

```
src/                     前端（Vue）
  components/            Sidebar / TopBar / StatusDot / ServiceDialog
  views/                 HomeView / SettingsView
  lib/                   api.ts（invoke 封装） store.ts（状态） icons.ts types.ts
channel/                 通道 sidecar（Go）
  main.go                tsnet 节点 + SOCKS5 服务端 + JSON 事件输出
src-tauri/               后端（Rust）
  src/config.rs          配置模型、校验、原子写入、首启种子数据
  src/channel.rs         通道进程管理、状态机、登录 URL、退出登录
  src/health.rs          TCP / SOCKS5 连通性探测
  src/windows.rs         服务窗口创建、贴合、导航策略、工具栏命令
  src/tray.rs            系统托盘菜单
  src/lib.rs             Tauri 命令注册、窗口事件、启动流程
  src/binaries/          编译产物位置（sidecar 与主程序同名约定）
  capabilities/          IPC 权限（仅主窗口，远程页面无任何权限）
scripts/make-icon.mjs    用纯 Node 生成应用图标源图
scripts/dev/             验证用的辅助脚本（模拟服务、截图、点击、SOCKS 探测）
```

### 重新编译 sidecar

```bash
cd channel
go mod tidy
go build -trimpath -ldflags "-s -w" \
  -o ../src-tauri/binaries/serverdesk-channel-x86_64-pc-windows-msvc.exe .
```

文件名里的 target triple 是 Tauri `externalBin` 的约定，`tauri build` 会自动把它作为
`serverdesk-channel.exe` 复制到主程序旁边并打进安装包。

## 开发

```bash
npm install
npm run tauri dev      # 开发模式（需要 Rust 工具链 + MSVC + WebView2）
```

## 构建 Windows 安装包 / exe

```bash
npm run tauri build
```

产物：

- `src-tauri/target/release/server-desk.exe` —— 可直接运行的绿色 exe
- `src-tauri/target/release/bundle/nsis/ServerDesk_0.1.1_x64-setup.exe` —— NSIS 安装包
- 本次交付的成品另复制在 GitHub Release 中的绿色版 ZIP

> 注意：请用 `npm run tauri build` 构建，不要直接 `cargo build --release`。
> Tauri CLI 会注入内嵌前端资源所需的构建环境，直接用 cargo 构建出的程序会去连
> `http://localhost:1420`（Vite 开发服务器）而不是加载内嵌页面。

## 已验证（Windows 11 / WebView2 156 / 2560×1440@150%）

v0.2 新增部分的实测结果：

- 配置 v1 → v2 迁移：旧服务转为 `tailnet` + 单入口，生成 `expectedOrigin`，
  原始文件备份为 `config.json.v1.bak`。
- 公网模式在**通道未授权**的情况下打开真实 Komari 面板（测试地址已脱敏），
  独立 Profile `services-webview-public` 被创建；同一实例里私网服务的健康检查被正确跳过
  （显示"未检测"，而不是误报离线）。
- 保险箱：创建后 `meta.json` 只有 salt/KDF 参数/包装后的 DEK；新增凭据后
  `vault.json` 里只有 nonce 与密文，全目录 grep 不到明文密码。
- 错误二级密码：界面提示"二级密码错误"，保险箱保持锁定；正确密码可解锁并列出凭据。
- 重置：`vault.json`/`meta.json`/`grants.json` 全部删除，两个 WebView2 Profile 目录被移除，
  `config.json`（服务、分组、连接方式、通道开关）与 `channel-state` 完整保留。
- 单元测试 6 项通过：AES-GCM 往返、AAD 跨记录绑定失败、每凭据密钥互异、KDF 确定性、
  Origin 归一化、授权 AAD 绑定。

历史版本（v0.1）已验证：服务内嵌加载、WebSocket/Cookie、窗口贴合与跟随、
托盘驻留与恢复、通道登录与代理链路、MagicDNS 域名透传。

在真机上逐项跑过：

- 首次启动自动生成配置文件与 4 条示例服务，首页统计/卡片/分组渲染正常。
- 服务状态检测：本地起了一个模拟服务，在线 2 条（延迟 3ms）、离线 1 条、占位地址 1 条「待配置」，
  与预期一致。
- 点击 AstrBot / NapCat 在程序内嵌加载页面；模拟页面的 WebSocket 回显、Cookie 写入、
  文件选择组件均正常工作（页面自检显示「正常 / webview-ok」）。
- 服务之间切换：旧窗口隐藏、新窗口显示，窗口不销毁，状态与登录会话保留。
- 主窗口移动与缩放时，内嵌页面窗口逐像素跟随（含 150% DPI 缩放换算）。
- 点击「首页」内嵌页面隐藏；点击窗口关闭按钮后隐藏到托盘，进程存活。
- 关闭到托盘后再次启动程序（单实例回调 → 同一个 `show_main`），主窗口与上次打开的服务一起还原。
- 设置页增删改：通过弹窗添加 Jellyfin，配置文件即时写入，侧边栏/设置列表同步更新。
- 启动时按 `startPage` 设置回到首页。
- 通道：进程随程序启动，未授权时产出 `login.tailscale.com/a/...` 登录链接并显示「等待
  Tailscale 授权」；未授权时点击服务会被拦下并跳到设置页（不白屏、不长时间转圈）；
  未授权时 SOCKS5 立即返回 host-unreachable，而不是挂到超时。
- 通道代理链路：用一个"直连版"临时 sidecar（不进仓库）让状态进入 Running，验证了
  WebView2 `--proxy-server=socks5://` 的实际效果——页面、WebSocket、Cookie 全部正常；
  并用一个本机无法解析的域名 `mock.tailnet.test:6185` 成功加载，证明 WebView2 把主机名
  交给代理解析（MagicDNS 名字可用）。
- 服务窗口使用独立的 WebView2 用户数据目录（`services-webview`），与主窗口环境隔离。

未在真机上点到的：

- 托盘图标的右键菜单本身（图标默认折叠在 Windows 溢出区，无法稳定用脚本点击）；菜单构建
  与「打开控制中心 / 设置 / 重新检测 / 服务快捷入口 / 退出」的事件处理与 `show_main` 为同一套
  代码路径，已通过单实例回调间接验证。
- 真实 tailnet 内的端到端访问（没有可用的 Tailscale 账号与授权节点），只验证了代理链路本身
  与登录引导。首次授权后建议先用一个服务确认节点可达。

## 配置文件

位置：`%APPDATA%\com.serverdesk.app\config.json`（界面「设置 → 关于」里可直接看到并复制）。

首次启动会自动写入 4 条示例服务（AstrBot / NapCat / 1Panel / 哪吒监控），
地址是 `http://YOUR-SERVER:端口` 占位符，把它们改成你的 Tailscale 私网地址即可。

```json
{
  "version": 1,
  "services": [
    {
      "id": "astrbot",
      "name": "AstrBot",
      "category": "AI",
      "url": "http://YOUR-SERVER:6185",
      "icon": "bot",
      "healthCheck": true,
      "sortOrder": 10,
      "enabled": true
    }
  ],
  "settings": {
    "autostart": false,
    "closeToTray": true,
    "startPage": "home",
    "channelEnabled": true,
    "channelHostname": "serverdesk",
    "channelTags": ""
  }
}
```

- `channelEnabled`：是否启用内置通道；关闭后服务页面直接访问配置里的地址。
- `channelHostname`：节点在 Tailscale 管理台里显示的名字。
- `channelTags`：可选的设备标签（如 `tag:serverdesk`），配合 ACL 收敛权限。
- 通道的节点身份不在配置里，而是放在 `%APPDATA%\com.serverdesk.app\channel-state`；
  删掉该目录或在界面里点「退出登录」就等于注销这个节点。

- `id`：仅允许字母/数字/`-`/`_`，唯一，同时决定窗口 label。
- `icon`：Lucide 图标名（`bot` `server` `dashboard` `activity` `globe` `container` …）。
- `category`：任意字符串，侧边栏按它分组；留空归入「其他」。
- `healthCheck`：开启后每 30 秒做一次 TCP 连接探测（不碰 HTTP，避免 CORS / 自签证书 / 登录墙导致误报）。
- 配置文件损坏时会自动备份为 `config.json.bak` 并重建默认配置。

## 安全设计

- 只加载用户显式配置过的地址。服务窗口的导航被 Rust 侧 `on_navigation` 拦截：
  目标主机不在「配置过的 host + 本地回环」白名单内时，取消加载并交给系统浏览器。
- 远程页面**不授予任何 Tauri IPC 权限**（capabilities 仅绑定 `main` 窗口），
  服务页面里的 JS 无法调用后端命令。
- 用户主动存入保险箱的服务凭据会加密保存；其他服务认证由各自的 WebUI 会话负责。
- 内置通道不写系统路由、不装驱动、不建虚拟网卡，只在本机 `127.0.0.1` 上开一个 SOCKS5
  端口；代理端点不做认证，只监听回环地址，并仅接受已启用服务的私网入口 `host:port`。
  本机其他进程仍可借此访问这些已配置目标；更严格的限制需依赖 tailnet ACL。
- 节点身份（tailnet 私钥）持久化在 `%APPDATA%\com.serverdesk.app\channel-state`，
  相当于凭据，不要拷贝给别人；「退出登录」会删除它并重新生成。
- 可以在 tailnet ACL 里用 `tag:serverdesk` 把该节点限制到指定端口，缩小暴露面。
- 建议全程走 Tailscale 等私网，不把管理后台暴露到公网。

## 第一版范围

已实现：侧边栏导航、首页概览、服务增删改查与排序、服务页面内嵌加载、后退/前进/刷新/
回首页/复制地址/外部打开、健康检查、系统托盘、开机自启、关窗驻留、启动页设置、配置持久化、
内置 tsnet 组网通道（登录引导 / 重启 / 退出登录 / 设备标签 / 代理与健康检查统一走通道）。

未实现（按需求留作后续）：详细服务器监控、Docker/SSH/文件管理、权限系统、云同步、
插件市场、自动服务发现、日志系统。
