ServerDesk Connection Manager v0.1 · 兼容性分析与实施建议
基线：ServerDesk 0.1.0（Tauri 2.12.5 / wry 0.57.1 / WebView2 156）
结论：需求可行，但有一个结构性约束（双 Profile）和两处必须先决的排序问题。
====================================================================


零、先说三个结论
--------------------------------------------------------------------
1. WebView2 的代理参数是"每个环境"的，而环境由用户数据目录决定。
   同一目录不能并存"走 SOCKS"和"不走 SOCKS"两套参数。
   => Tailnet 与 Public 必须使用两个独立的数据目录，切换连接方式必然是
      "销毁旧窗口 + 用另一个 Profile 重建"，不可能原地改代理。
      这与需求 3.3 的禁止项一致，不是妥协。

2. Vault 目前是 0 行代码（只有 docs/VAULT-DESIGN.md）。
   需求第六节和阶段四建立在 Vault 之上，无法按现有顺序执行。
   而且 Vault 设计里的授权键是 service_id + origin，新需求要求 endpoint_id。
   现在改设计零成本，等密钥材料写盘后再改就要做迁移。

3. Tauri/WebView2 不暴露"导航失败原因"，无法从页面层判断连接故障。
   Failover 只能做"开窗前预检"（TCP/SOCKS 连一下），不能做"页面加载失败后回退"。
   这恰好与需求"不得因 HTTP 500 切换"相容，但要知道它管不了"端口开着但应用挂了"。


一、现状核查（对应第 12 节的七项）
--------------------------------------------------------------------
1. 服务配置结构
   src-tauri/src/config.rs:20 `Service { id, name, category, url, icon,
   healthCheck, sortOrder, enabled }`，单一 url，没有入口概念；
   AppConfig.version = 1（config.rs:95/103），保存是全量覆盖写。

2. Rust 后端服务管理接口
   lib.rs：open_service / hide_all_services / close_service / set_content_rect /
   nav_back|forward|reload|home / active_url / check_services。
   全部以 service_id 为键，没有 endpoint 维度。

3. WebView2 窗口创建
   windows.rs:166 构造 builder；:203 `data_directory(app_local_data_dir/services-webview)`；
   :206 `proxy_url(通道 up 时)`。同一进程内所有服务窗口共用同一目录与同一套代理参数。

4. 代理与 Profile 管理
   只有一个 services-webview 目录；通道一旦 up，所有服务窗口都带 socks5 代理；
   通道未 up 时创建的服务窗口不带代理（并在通道就绪时被统一重建，
   windows.rs:267 reset_service_windows）。没有"部分走代理、部分直连"的能力。

5. tsnet Sidecar 生命周期
   channel.rs:117 start（stdin 管道 + CREATE_NO_WINDOW + 逐行 JSON），
   :104 stop，:254 logout。全局单实例、单端口，与具体服务无关。

6. 窗口的关闭 / 隐藏 / 重建
   切换服务只 hide（windows.rs:236）；删除服务或通道就绪时 destroy。
   重建后是新窗口，滚动位置丢失，Cookie 由 Profile 保留。

7. Vault 实现进度
   无。src-tauri/src/ 下只有 channel/config/health/lib/main/tray/windows 七个模块。
   因此第六节"与 Vault 集成"在当前代码上无对象可集成。


二、地基约束：为什么必须是两个 Profile
--------------------------------------------------------------------
WebView2 的 `--proxy-server` 属于 CoreWebView2EnvironmentOptions 的一部分，
而环境由 user data folder 唯一标识。微软的约束是：
同一个用户数据目录只能有一个环境参数集合，后创建且参数不同的环境会失败。

现状（windows.rs:203）把全部服务塞进一个 services-webview，
这等价于"全局一个网络路径"。要做 Per-service 模式，最小改动是：

  services-webview          -> Tailnet 模式（带 socks5 代理）
  services-webview-public   -> Public 模式（不带代理）

由此派生三个必然结果：
  a) 切换模式/入口 = 销毁窗口 + 用另一 Profile 重建（符合 3.3 的禁止原地改代理）。
  b) 同一 origin 在两个 Profile 里是两份独立会话。正常配置下两个入口 origin 不同，
     影响很小；但若用户把同一 origin 配成主备两个入口，会看到"两个互不相干的登录态"，
     需要在 UI 明确提示。
  c) Vault 的重置必须同时清理两个 Profile（原设计只考虑了一个，需要改）。


三、数据模型与迁移
--------------------------------------------------------------------
建议结构（保持 endpoint_id 稳定，Vault 授权将来挂在它上面）：

  Service {
    id, name, category, icon, healthCheck, sortOrder, enabled,
    connectionMode: "tailnet" | "public" | "failover",
    endpoints: [ Endpoint ],
    // 兼容字段：仅迁移时读取，迁移后写回 null
    url: Option<String>
  }

  Endpoint {
    endpointId: String,      // 稳定唯一，创建后不可变
    kind: "tailnet" | "public",
    url: String,
    expectedOrigin: String,  // 冗余存储，便于比对与授权
    enabled: bool,
    httpAckAt: Option<i64>,  // HTTP 风险确认时间（绑定 origin）
    httpAckOrigin: Option<String>
  }

activeEndpoint 是运行态，放内存（每个窗口/每个服务一条），不落盘。

迁移 v1 -> v2：
  1. 迁移前备份 config.json -> config.json.v1.bak
  2. 每个旧 Service：生成一个 tailnet Endpoint（endpointId = "ep-" + 稳定散列），
     connectionMode = "tailnet"，原 url 保留在 Service.url 供对照
  3. 保留未知字段（serde flatten），避免旧版本回退时丢数据（这也是审查报告 P8）
  4. 迁移后执行一次"体检"：若 endpoint 的主机不是私网网段
     （10/8、172.16/12、192.168/16、100.64/10、fc00::/7、回环），
     不自动改模式，只在首页/设置里提示"该地址不像私网地址，可能应改为公网直连"。
     原因：tsnet 默认只路由 tailnet 网段，把公网 IP 判成 tailnet 会直接连不上，
     而需求 11 又明确"不得自动改为 Public"，所以只能提示。
  5. 任何一步失败：保留原文件、报错、不写入半成品。


四、需要改动的现有代码（最小集）
--------------------------------------------------------------------
config.rs          Service/Endpoint 模型、迁移、保留未知字段      +120 行
windows.rs         按 endpoint 选 data_directory 与 proxy；
                   允许清单改为"所有 endpoint 的 host"；
                   新增 destroy_all_service_windows()             +60 行
lib.rs             check_services 按 endpoint 选路；
                   新增 connection_* 命令（状态/预检/重试）        +90 行
channel.rs         不改（继续作为"能力提供者"）
前端 store.ts      openService 门禁：从"全局通道 ready"改为
                   "该服务当前 endpoint 所需路径是否就绪"          +30 行
前端 设置/弹窗     连接方式选择、两个地址、HTTP 确认、状态文案      +150 行

不改：窗口贴合逻辑、托盘、开机自启、通道进程管理。

前端门禁语义变化（现在会拦掉所有服务）：
  现在 store.ts:191 `if (!channelReady())` 对所有服务生效。
  改后：tailnet endpoint 才要求通道 up；public endpoint 永远可打开；
        failover 在预检后才决定。这是需求"公网服务不得因 Tailscale 不可用而无法打开"的落点。


五、Failover 的检测边界（必须写进产品预期）
--------------------------------------------------------------------
能做：开窗前预检主入口（1.5s 超时）
  - 通道 sidecar 不可用（phase != up）
  - SOCKS/TCP 连接超时
  - 连接被明确拒绝
  - 预检失败 -> 用备用入口重建窗口

做不到（Tauri 2 未暴露导航错误码）：
  - 页面级失败（证书错误、DNS 失败之后的 Chromium 错误页、HTTP 4xx/5xx）
  - "端口开着但服务进程挂了"
  - 页面内 JS 报错

因此：切换只依据网络层证据，与需求"不得因 HTTP 500 / 证书错误切换"一致，
但也要接受"预检通过、页面仍打不开"时不会自动回退——此时提供手动"重试私网"。

切换后的窗口重建会丢滚动位置（Cookie 在 Profile 里，登录态不丢），
UI 必须明确显示"已切换到公网备用入口"，并给"重试私网"按钮。

补充一个已知行为：Chromium 默认对 127.0.0.1 / localhost 绕过代理。
所以 Tailnet 模式填本地地址会实际直连（不走 tsnet）。建议在保存时提示，
并把这个判断写进 health 与 autologin 的前置条件。


六、与 Vault 的兼容性（需要调整顺序）
--------------------------------------------------------------------
1. Vault 未实现，阶段四无法执行。建议改为：
   先修订 Vault 设计（授权键 service_id -> endpoint_id），再实现 Vault，
   最后做"连接切换不扩大授权"的联调。
   修订点很具体：
     - grants.json 的授权记录增加 endpoint_id，与 origin 一起做 AAD；
     - 授权匹配条件从 (service_id, origin) 改为 (endpoint_id, origin)；
     - 6.2 的"HTTP 自动登录二次确认"成为 Endpoint 上的一个独立 ack 字段。
   现在改是零成本（无密钥材料落盘），将来改需要迁移授权信封。

2. 若先做 Connection Manager、后做 Vault，两者互不阻塞：
   Endpoint 模型先稳定，Vault 实现时直接挂上去即可。
   这是推荐顺序。

3. HTTP 自动登录的禁止项（6.2）在 Vault 落地后再实现，
   在此之前不存在"自动提交凭据"的代码路径，风险为零。


七、与既有安全评审遗留项的关系
--------------------------------------------------------------------
评审 P1 已补上目标白名单：Go sidecar 默认拒绝所有 CONNECT，仅允许已启用
服务的私网入口 host:port。启动时从配置加载，配置变更时由桌面进程经 stdin 更新。
本机其他进程仍可访问这些允许的目标，tailnet ACL 继续作为第二道边界。


八、风险与限制
--------------------------------------------------------------------
L1（中）双 Profile 带来的会话分裂：同一 origin 两套 Cookie。UI 需提示。
L2（中）Failover 只能预检，页面级故障不可见（见第五节）。
L3（低）每次切换 = 重建窗口，滚动位置丢失；Cookie 保留。
L4（中）证书错误要等页面加载后才可见，预检无法发现；不自动降级（符合需求）。
L5（低）HTTP 风险确认按 origin 记录，DNS 重绑定无法根治。
L6（高，排序问题）Vault 未实现，阶段四必须后移。
L7（中）迁移把公网地址判为 tailnet 会导致不可达，只能提示不能自动改。
L8（低）目前没有"每服务独立 Profile"，同一 Profile 内多服务仍共享 Cookie 容器
   （同源策略保证不同 origin 隔离）。需求 7 的"每服务隔离"是可选加强，
   代价是内存与启动时间，建议 v0.1 不做。


九、分阶段计划（按依赖重排）
--------------------------------------------------------------------
阶段 0 · 模型与迁移（半天，行为不变）
  数据模型、v1->v2 迁移、体检提示、保留未知字段。
  验收：旧配置升级后所有服务仍按 tailnet 打开，配置不丢。

阶段 1 · Public 模式（1-2 天）
  第二个 Profile、按 endpoint 选目录与代理、前端门禁按服务、健康检查按 endpoint。
  验收：同一实例同时打开私网与公网服务；Tailscale 未登录时公网服务照常可用。

阶段 2 · HTTP 风险提示（1 天）
  HTTP 识别、首次连接确认（per-origin）、HTTP 徽标、证书错误不放行不降级。
  验收：公网 HTTP 首次拦截并说明；确认后可继续；地址变更后重新确认。

阶段 3 · Failover（1-2 天）
  主备配置、预检、超时、切换后重建、状态展示、手动重试。
  验收：私网不可达时切到备用；证书错误/401 不触发切换。

阶段 4 · Vault（先修订设计，再实现；与 Connection Manager 解耦）
  授权键改 endpoint_id；HTTP 自动登录二次确认；重置覆写两个 Profile。

阶段 5 · 异常测试（对应第十节 12 条）


十、需要你确认的四件事
--------------------------------------------------------------------
1. 迁移体检的处理方式
   A：只提示，用户自己改（推荐，符合"不得自动改为 Public"）
   B：对明显是公网地址的服务自动改为 Public（会自动改变行为，需你确认）

2. Profile 粒度
   A：全局两个目录（tailnet / public），推荐
   B：每个服务一个目录（隔离最强，内存与启动成本明显上升）

3. Failover 预检超时与重试
   建议超时 1.5s；切换后只提供手动"重试私网"，不做后台自动回切
   （自动回切会造成页面反复闪烁）。

4. Vault 与 Connection Manager 的顺序
   建议先做 Connection Manager 阶段 0-3，再回头修订 Vault 设计并实现。
   若你希望先做 Vault，则需要先把 endpoint_id 引入 Vault 设计，
   否则将来授权材料要迁移。

====================================================================
文件位置：docs/CONNECTION-DESIGN.md
