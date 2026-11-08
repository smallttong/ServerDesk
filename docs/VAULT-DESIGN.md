ServerDesk Vault v0.1 · 模块设计与风险分析
基线：ServerDesk 0.1.0（Tauri 2.12.5 / wry 0.57.1 / WebView2）
结论：需求可行，但有 4 处需要在设计层明确妥协，见第九节风险表 R2/R3/R6/R9。
====================================================================


一、现状核查（对应第 9 节的六项）
--------------------------------------------------------------------
1. WebView2 Profile 与用户数据目录
   主窗口使用 Tauri 默认目录（%LOCALAPPDATA%\com.serverdesk.app\EBWebView）。
   服务窗口在 src-tauri/src/windows.rs:203 显式指定
   %LOCALAPPDATA%\com.serverdesk.app\services-webview，所有服务共用这一个 Profile。
   结论：服务会话与主窗口完全隔离；但服务之间共享一个 Profile。
   影响：清理只能按 Profile 整体执行，无法只清某一个服务（见 R2）。

2. 服务窗口创建 / 隐藏 / 销毁
   windows.rs:135 open() 创建（decorations(false) + skip_taskbar + owner=主窗口
   + data_directory + 可选 proxy_url）；切换时 hide()（:236）；删除服务与通道就绪时
   destroy()（:254 / :269）。窗口常驻，登录态依赖 Cookie 持久化。
   结论：Vault 可以直接复用这套生命周期，不新建窗口管理器。

3. Rust IPC 权限与窗口来源隔离
   capabilities/default.json 只绑定 main 窗口（core:default + opener:default）。
   Tauri 在 webview/mod.rs:2080 对 !is_local 的调用强制走 ACL，无授权即 reject。
   已实测：远程页面调用 get_config 返回 "Command get_config not allowed by ACL"。
   另：Webview 实现了 CommandArg（webview/mod.rs:2604），命令可以拿到调用者，
   校验 label 与 url 是可行的（见第六节）。

4. 服务配置存储
   %APPDATA%\com.serverdesk.app\config.json，全量覆盖写，tmp+rename 原子替换，
   serde 结构在 config.rs。Vault 数据不放进这个文件，另建目录。

5. tsnet Sidecar 生命周期
   channel.rs:117 start（stdin 管道 + CREATE_NO_WINDOW + 逐行 JSON），
   :104 stop（关 stdin 后 kill 兜底），:254 logout。与 Vault 无耦合，
   但自动登录依赖通道就绪（见 R10）。

6. 现有 Cookie / 会话管理
   代码里没有任何显式 Cookie 操作，完全依赖 WebView2 默认持久化。
   Tauri 2.12.5 提供了所需能力（均为稳定 API，无需 unstable）：
     - Webview::clear_all_browsing_data()  webview/mod.rs:2397
       Windows 走 ICoreWebView2Profile2::ClearBrowsingDataAll，按 Profile 生效，
       且文档明确"删除是异步的，函数返回时数据不一定已消失"。
     - Webview::cookies() / cookies_for_url() / set_cookie() / delete_cookie()
       webview/mod.rs:2447 / 2418 / 2456 / 2469
     - WebviewWindowBuilder::on_page_load()  webview_window.rs:421
   结论：重置所需的"清 Cookie / LocalStorage / IndexedDB"有原生支持，
   不需要删目录，也不需要自己实现清理逻辑。


二、总体架构：旁挂，不改现有模块
--------------------------------------------------------------------
新增 src-tauri/src/vault/，对外只暴露命令；windows.rs 与 channel.rs 只加"钩子"，
不重构。

  lib.rs
    ├─ manage(VaultState)                 新增
    ├─ 注册 vault_* 命令                  新增
    └─ setup(): 检查 reset.pending        新增 3 行

  windows.rs（改动最小）
    ├─ open(): 创建窗口时挂 on_page_load         +5 行（调用 vault::autologin::on_page_load）
    ├─ reset_service_windows(): 保持不动
    └─ 新增 destroy_all_service_windows()        +8 行（给重置用，复用现有遍历）

  config.rs（不改结构）
    services 增加可选字段 authAdapter / authOrigin（serde default，向后兼容）

  vault/
    mod.rs           状态机与对外命令
    crypto.rs        Argon2id / HKDF / AES-256-GCM / DPAPI 封装
    storage.rs       文件布局、原子写、reset.pending
    credential.rs    凭据 CRUD（仅解锁态）
    authorization.rs 授权签发、校验、过期
    autologin.rs     适配器注册表 + 注入
    reset.rs         销毁流程
    audit.rs         无秘密的操作记录

前端 src/features/vault/：设置里的"保险箱"分区、锁定态提示、解锁弹窗、
凭据列表（默认掩码），不持有任何长期密钥。


三、数据布局与加密
--------------------------------------------------------------------
目录：%APPDATA%\com.serverdesk.app\vault\

  meta.json      非秘密：version、kdf 参数（argon2id m/t/p）、salt(b64)、
                 wrapped_dek{n,t}、created_at、vault_id
  vault.json     密文记录：credential_id、service_id、origin、username、
                 secret{n,t}、type、created_at、updated_at、auto_login
  grants.json    授权：grant_id、credential_id、service_id、endpoint_id、origin、
                 expires_at、wrapped_cek{n,t}、dpapi_master(b64 非明文密钥)
  audit.log      JSONL，只记动作与结果，不含秘密
  reset.pending  重置未完成标记

密钥层次：
  KEK = Argon2id(password, salt, m=64MiB, t=3, p=1) -> 32B
  DEK = random 32B                     （CSPRNG）
  wrapped_dek = AES-256-GCM(KEK, nonce, DEK, aad="serverdesk-vault-dek-v1")
  cek_i = HKDF-SHA256(DEK, info="cred/"+credential_id)   -> 32B   每凭据独立
  secret_i = AES-256-GCM(cek_i, nonce, plaintext,
                         aad = credential_id | service_id | origin | version)
  grant_master = random 32B -> DPAPI 保护后存 grants.json
  wrapped_cek_i = AES-256-GCM(grant_master, nonce, cek_i,
                              aad = grant_id | service_id | endpoint_id | origin | expires_at)

为什么每凭据一把 cek（而不是直接用 DEK）：
  授权需要"只解开这一条"，用 HKDF 派生后单独包装 cek_i，就无法借授权推出其他凭据的密钥。
  这是"授权 ≠ 完整解锁"在密码学层面的保证，不是靠代码 if 判断。

依赖（全部成熟库，不自研算法）：
  argon2、aes-gcm、hkdf、sha2、rand、zeroize、serde_json、
  windows（DPAPI CryptProtectData/CryptUnprotectData）

参数基线：Argon2id 64MiB / t=3 / p=1，实测在这台机器约 0.4-0.8s；
若目标机是低配，降 m 会削弱抗暴力能力，需在设置里显示"解锁耗时"而不是偷偷降参。


四、状态机与超时
--------------------------------------------------------------------
  Uninitialized --initialize(pwd)--> Locked
  Locked --unlock(pwd [+面板认证])--> Unlocked
  Unlocked --lock / 空闲 5 分钟--> Locked
  Locked/Unlocked --reset--> Uninitialized

权限状态全部在 Rust（vault::VaultState），前端只读 vault_get_status。
unlocked 时内存里持有 DEK；lock 时 drop + zeroize。
空闲判定：记录最后一次 vault_* 调用时间，后台线程每 30s 检查；
只统计 Vault 命令，不统计普通窗口操作（查看凭据即刷新计时）。

面板认证（Auth 组件）预留为 trait：
  trait PanelAuth { fn verify(&self) -> Result<(), String>; }
  默认实现为空（未安装 Auth）；Auth 安装后由它注册实现。
  Vault 不 import Auth 的任何类型，只依赖这个 trait，避免强耦合。


五、授权模型（免密自动登录）
--------------------------------------------------------------------
签发（仅 Unlocked 态）：
  1. 校验 credential 的 origin 与当前 config 中服务 URL 的 origin 一致（R7）。
  2. 派生 cek_i，用 grant_master 包装，写入 grant_id/expires_at。
  3. expires_at = now + min(用户选择, 7 天)。
  4. 记审计：service_id、credential_id、时长（不含秘密）。

使用（Locked 态也允许）：
  1. 取 grants.json，DPAPI 解出 grant_master。
  2. 找到 service_id + endpoint_id + origin 三者都匹配、未过期的 grant。
  3. 解出 cek_i，只解密这一条 secret。
  4. 立即 zeroize 明文，注入完即丢弃。
  5. 任何一项不匹配（服务/Origin/过期/版本）→ 拒绝，记录失败原因码。

重新授权：只需 unlock 输入二级密码，再签一次；不要求面板认证（除非 Auth 启用）。
终止：删除单条 grant，或重置时全删。

"本次运行"授权：只放内存，不写 grants.json，进程退出即失效。
跨重启授权：写 grants.json + DPAPI。这是 3.5 的待确认项，默认行为待你定。

DPAPI 的真实强度（必须写进 UI 文案）：
  同一 Windows 用户下的任何进程都能解密 DPAPI 数据。
  它防的是"拷走文件到另一台机器/另一个账户"，不防本机同用户恶意程序。
  因此 7 天免密授权的实质是"把安全边界降到 Windows 账户"。


六、自动登录：适配器机制与注入
--------------------------------------------------------------------
不做"通用表单填充"。每个服务显式配置适配器，默认关闭：

  config.services[].authAdapter = "astrbot" | null

适配器接口（Rust）：
  fn login_matcher(&self) -> &[&str]          // 例如 ["/#/auth/login"]
  fn fill_script(&self, user, secret) -> String

触发链路：
  on_page_load(窗口加载完成)
    -> 通道是否 up？（R10：否则不尝试）
    -> 该服务是否配置了 authAdapter？
    -> 当前 URL 是否匹配适配器登录页？
    -> 该 service+origin 是否有有效授权？
    -> 取 secret，生成脚本，eval 到页面

注入脚本必须自带二次校验（防 TOCTOU）：
  if (location.origin !== "<expected>") return;
  if (document.readyState === "loading") return;
  // 只在匹配的选择器存在时填，填完不重复执行
  window.__sdAuthTried = true;
Rust 侧的 origin 判断只作为前置过滤，真正的门闩在页面内这一句——
检查与填写在同一个 JS 上下文里完成，中间无法被导航劫持。

不做的：
  - 不自动点"记住我"
  - 不提交验证码/2FA
  - 不跨 origin 复用脚本
  - 登录成功后不记录"成功"（无法可靠判断），只观察 URL 离开登录页


七、IPC 边界
--------------------------------------------------------------------
命令统一签名带调用者：
  #[tauri::command] fn vault_unlock(webview: Webview, state: State<VaultState>, ...)

守卫（每个 vault_* 命令第一行）：
  if webview.label() != "main" { return Err("forbidden") }
  if webview.url()?.scheme() != "tauri" { return Err("forbidden") }
远程页面本来就被 ACL 拦（已验证），这是第二道。

返回策略：
  - 列表接口只返回 username 与掩码（"••••"），永不返回 secret。
  - 只有 vault_reveal_credential(id) 返回单条明文，且要求 Unlocked；
    前端显示 20 秒后自动隐藏，不写入任何持久化状态。
  - 不通过事件广播任何凭据字段。
  - 审计日志不含 secret、不含 Cookie 值。

命令集：vault_initialize / unlock / lock / get_status / list_credentials /
add / update / delete / authorize_service / revoke_authorization /
list_authorizations / reveal_credential / reset。


八、重置流程（重点：顺序、异步、幂等）
--------------------------------------------------------------------
前提：clear_all_browsing_data 的删除是异步的（官方文档明说），
所以"清完立刻删密钥"是不可靠的。必须有可验证的等待。

步骤（每步幂等）：
  1. 写 reset.pending（fsync）
  2. 立刻删除 grants.json（先断自动登录，后面崩了也不会恢复授权）
  3. drop + zeroize DEK
  4. 对所有 svc-* 窗口各调用一次 clear_all_browsing_data()
  5. destroy 全部 svc-* 窗口
  6. 建一个隐藏的临时 webview（同 data_directory，about:blank），
     再调一次 clear_all_browsing_data()，然后 cookies() 轮询直到为空（最多 10s）
  7. 删除 vault.json / meta.json / grants.json
  8. 记为 Uninitialized，删除 reset.pending

为什么 6 要在销毁之后再清一次：第 4 步清理时页面还在运行，
可能在清除与销毁之间把 Cookie 写回去；销毁后再清一次覆盖这个窗口期。

失败处理：
  - 任一步失败 → 保留 reset.pending，状态返回 "reset_pending"，
    前端只允许"继续清理"，不允许解锁、不允许打开服务。
  - 下次启动 setup() 最先检查 reset.pending，先跑 2/4/5/6/7 再继续启动。

重启后竞态：清理完到下次启动之间，用户手动打开服务会重新登录并写入 Cookie。
这是可接受的（那是新会话），但 reset.pending 未清时禁止打开服务窗口。

不动的数据：config.json（服务名/地址/分组/排序/启停）、channel-state（tsnet 身份）。
明确不做：不向远程服务发登出/撤销请求；不承诺服务端会话失效。


九、风险分析
--------------------------------------------------------------------
R1（中）DPAPI 对同用户透明
   免密授权 = Windows 账户强度。必须在设置页直白说明，不能写成"加密保护"。

R2（中）Profile 级清理，无法只清单个服务
   services-webview 是共享 Profile。重置会清掉该 Profile 下所有服务的会话，
   包括将来不在 Vault 里管理的服务。接受这个粒度（需求本身也是"全部受管服务"），
   但要在文档里写清楚。

R3（中高）清理异步 + 页面写回
   已按"清→销毁→再清→验证 cookies 为空"设计。若 WebView2 运行时过旧
   （ClearBrowsingDataAll 不可用），clear 会返回错误 → 重置进入 reset_pending，
   提示用户升级 WebView2；不静默降级为"只删密钥"。
   残余风险：清理完成但浏览器进程仍缓存于内存时，理论上仍可能写回；
   文件层校验（cookies 为空）是目前能做的最强验证。

R4（中）明文必然进入页面上下文
   自动登录的本质就是把密码填进页面。恶意页面（或页面被注入）能读到它。
   只能靠"精确 origin + 指定适配器 + 页面内二次校验"把范围压到最小。

R5（低）前端可见性
   生产构建关闭 devtools；secret 只在 reveal 的返回值里出现一次，
   不进入 Pinia/ref 根状态的时间超过展示期。

R6（中）密码学实现正确性
   不自研算法；参数写死并有版本号，便于将来升级；AAD 绑定记录身份，
   防止密文跨记录替换（这是需求 5 明确要求的）。

R7（中）服务地址变更导致凭据错配
   每次解锁时对比 config 中服务 URL 的 origin 与凭据 origin：
   不一致 → 凭据标记 stale，禁止自动登录，UI 提示"该服务地址已变更，请重新绑定"。
   绝不自动迁移授权。

R8（低）"二级密码不可修改"的实现风险
   只要不存在"用新密码重新包装 DEK"的代码路径，规则自然成立。
   代码评审时以 grep 为准：vault 模块内不允许出现二次 wrap DEK。

R9（中）无密码重置可被本机滥用
   需求明确不要求旧密码。结果是同机攻击者能销毁数据（DoS），但不能读取。
   缓解：两步确认 + 输入固定短语"重置保险箱"（或长按 3 秒），不用旧密码。

R10（低）通道未就绪时注入无意义
   自动登录前置检查 channel 状态（channelEnabled 且 phase=up，或通道关闭）。

R11（中）同 origin 多凭据
   共享 Profile 意味着同 origin 只有一个会话，多账号必然互相覆盖。
   v0.1 限制：一个 origin 只允许一条凭据；多账号需要每服务独立 Profile（后续）。

R12（低）审计泄露服务拓扑
   audit.log 含服务 id 与时间，不含秘密。限制 512KB 滚动。


十、测试映射（对应第 8 节 14 条）
--------------------------------------------------------------------
可自动化（Rust 集成测试 / 无 UI）：
  1 正确密码解锁、错误密码失败              -> 单元测试
  5 授权期限不超过 7 天                     -> 单元测试（越界传参被夹到 7 天）
  6 改 Origin 后旧凭据不用于新 Origin       -> 单元测试（AAD 与 origin 比对）
  8/9 重置后凭据授权失效、配置仍在          -> 集成测试（临时目录 + 假 Profile）
  10 重置中途退出不恢复旧授权               -> 集成测试（注入失败点 + 重启）
  13 无改密入口                             -> 代码检索 + 命令清单断言
  14 不调用远程登出                         -> 代码检索（网络调用点白名单）

需人工（涉及 WebView2 真实行为）：
  2 锁定态前端调不动敏感 IPC                -> 手动：在 DevTools 里直接 invoke
  3/7 授权不能读别的凭据、服务间不串         -> 手动：两条凭据 + 两窗口交叉尝试
  4 过期后仅密码续期                        -> 手动 + 拨时钟（或测试用过期注入）
  11 重置时窗口不写回                       -> 手动：登录后立刻重置，观察 cookies
  12 无 Auth 时 Vault 独立可用              -> 手动


十一、实施顺序与工作量
--------------------------------------------------------------------
阶段一 基础保险箱（约 230 行 Rust + 180 行前端）
  crypto/storage/状态机/CRUD/自动锁定 + 设置页
阶段二 授权系统（约 160 行）
  grant 签发/校验/过期 + "本次运行"与 DPAPI 两种模式
阶段三 WebView2 集成（约 120 行）
  on_page_load 钩子 + AstrBot 适配器（首个试点）
阶段四 重置（约 140 行）
  顺序化清理 + reset.pending + 启动恢复 + cookies 为空校验
阶段五 面板认证接口（约 30 行）
  trait + 空实现 + 文档

不动：windows.rs 的贴合逻辑、channel.rs、config 结构（只加可选字段）。


十二、需要你确认的四件事
--------------------------------------------------------------------
1. 跨 Windows 重启的免密授权
   3.5 标为待确认。选 A：v0.1 只做"本次运行"（不落盘，最安全）；
   选 B：做 DPAPI 落盘（默认 7 天，体验好，但边界降到 Windows 账户）。
   我建议 A 起步，阶段二末尾再加 B，这样重置逻辑与授权逻辑都先跑通再谈持久化。

2. 重置的本地确认强度
   需求禁止用旧密码。可选：输入固定短语 / 长按 3 秒 / Windows Hello（若可用）。
   我建议"输入短语 + 二次弹窗"，Windows Hello 作为增强（有设备时启用）。

3. 同 origin 多凭据
   建议 v0.1 直接禁止（一个 origin 一条），UI 上加一句说明。
   若要支持，需要改每服务独立 Profile，影响窗口模型，属于另一次改动。

4. 首个自动登录适配器选谁
   建议 AstrBot（登录页结构简单，你已在用，验收最快）。

====================================================================
文件位置：docs/VAULT-DESIGN.md
