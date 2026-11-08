import { computed, nextTick, reactive } from "vue";
import { api } from "./api";
import type {
  AppConfig,
  AuthStatus,
  ChannelStatus,
  CredentialInput,
  CredentialView,
  Endpoint,
  EndpointKind,
  GrantView,
  HealthResult,
  Rect,
  ServiceConfig,
  VaultStatus,
} from "./types";

export type ViewName = "home" | "settings" | "service";
export type Status = "online" | "offline" | "unknown" | "disabled";

export interface Toast {
  id: number;
  kind: "info" | "error" | "success";
  text: string;
}

const EMPTY: AppConfig = {
  version: 2,
  services: [],
  settings: {
    autostart: false,
    closeToTray: true,
    startPage: "home",
    channelEnabled: true,
    channelHostname: "serverdesk",
    channelTags: "",
  },
};

const EMPTY_CHANNEL: ChannelStatus = {
  phase: "stopped",
  socksPort: null,
  loginUrl: null,
  ip: null,
  message: null,
  hostname: null,
};

export const state = reactive({
  ready: false,
  config: structuredClone(EMPTY) as AppConfig,
  channel: structuredClone(EMPTY_CHANNEL) as ChannelStatus,
  view: "home" as ViewName,
  activeId: null as string | null,
  health: {} as Record<string, HealthResult>,
  checking: false,
  toasts: [] as Toast[],
  httpConfirmServiceId: null as string | null,
  rect: { x: 0, y: 0, width: 0, height: 0 } as Rect,
  configFile: "",
  vault: { initialized: false, unlocked: false, pendingReset: false, lockAfterSecs: 300 } as VaultStatus,
  auth: { enabled: false, registered: false, authenticated: true, locked: false, username: null, userId: null, autoLockMinutes: 15, error: null } as AuthStatus,
  vaultCredentials: [] as CredentialView[],
  vaultGrants: [] as GrantView[],
});

/** Only tailnet endpoints depend on the built-in channel. */
export function channelReady(): boolean {
  return state.config.settings.channelEnabled && state.channel.phase === "up";
}

export function endpointOf(
  service: ServiceConfig,
  kind: EndpointKind,
): Endpoint | undefined {
  return service.endpoints.find(
    (endpoint) => endpoint.kind === kind && endpoint.enabled && isUrlOk(endpoint.url),
  );
}

/** Which endpoint the service currently uses (mirrors the Rust resolver). */
export function activeEndpoint(service: ServiceConfig): Endpoint | undefined {
  switch (service.connectionMode) {
    case "tailnet":
      return endpointOf(service, "tailnet");
    case "public":
      return endpointOf(service, "public");
    default:
      return channelReady()
        ? endpointOf(service, "tailnet") ?? endpointOf(service, "public")
        : endpointOf(service, "public") ?? endpointOf(service, "tailnet");
  }
}

export function serviceUrl(service: ServiceConfig): string {
  return activeEndpoint(service)?.url ?? service.endpoints[0]?.url ?? "";
}

export function modeLabel(service: ServiceConfig): string {
  switch (service.connectionMode) {
    case "tailnet":
      return "私网";
    case "public":
      return "公网";
    default:
      return "自动切换";
  }
}

/** A tailnet endpoint pointing at a public address cannot work: tsnet only
 *  routes tailnet ranges. Used to show a non-blocking hint. */
export function looksLikePublicHost(url: string): boolean {
  const match = /^https?:\/\/(\[[^\]]+\]|[^/:?#]+)/i.exec(url.trim());
  if (!match) return false;
  const host = match[1].replace(/^\[|\]$/g, "");
  if (/^\d+\.\d+\.\d+\.\d+$/.test(host)) {
    const [a, b] = host.split(".").map(Number);
    if (a === 10) return false;
    if (a === 172 && b >= 16 && b <= 31) return false;
    if (a === 192 && b === 168) return false;
    if (a === 127) return false;
    if (a === 100 && b >= 64 && b <= 127) return false;
    return true;
  }
  if (host.includes(":")) {
    const lower = host.toLowerCase();
    if (
      lower === "::1" ||
      lower.startsWith("fc") ||
      lower.startsWith("fd") ||
      lower.startsWith("fe80")
    ) {
      return false;
    }
    return true;
  }
  return false;
}

export const channelLabel = computed(() => {
  if (!state.config.settings.channelEnabled) return "通道已关闭";
  switch (state.channel.phase) {
    case "up":
      return state.channel.ip ? `通道已连接 · ${state.channel.ip}` : "通道已连接";
    case "login":
      return "等待 Tailscale 授权";
    case "starting":
      return "通道启动中";
    case "error":
      return "通道异常";
    default:
      return "通道未启动";
  }
});

let toastSeq = 1;
export function toast(text: string, kind: Toast["kind"] = "info") {
  const id = toastSeq++;
  state.toasts.push({ id, kind, text });
  setTimeout(() => {
    const i = state.toasts.findIndex((t) => t.id === id);
    if (i >= 0) state.toasts.splice(i, 1);
  }, 3800);
}

/** A URL is usable only if it looks like http(s)://host and isn't the seeded placeholder. */
export function isUrlOk(url: string): boolean {
  if (!url) return false;
  if (/YOUR[-_]SERVER/i.test(url)) return false;
  return /^https?:\/\/[^\s/]+/i.test(url.trim());
}

export function statusOf(service: ServiceConfig): Status {
  if (!service.enabled) return "disabled";
  if (!activeEndpoint(service)) return "unknown";
  const h = state.health[service.id];
  if (!h) return "unknown";
  return h.online ? "online" : "offline";
}

export const enabledServices = computed(() =>
  state.config.services.filter((s) => s.enabled),
);

export const groupedServices = computed(() => {
  const list = state.config.services
    .filter((s) => s.enabled)
    .slice()
    .sort(
      (a, b) =>
        a.sortOrder - b.sortOrder || a.name.localeCompare(b.name, "zh-CN"),
    );
  const groups = new Map<string, ServiceConfig[]>();
  for (const s of list) {
    const key = (s.category || "").trim() || "其他";
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(s);
  }
  const preferred = ["AI", "服务器", "运维", "媒体", "其他"];
  const rank = (name: string) => {
    const i = preferred.indexOf(name);
    return i < 0 ? 50 : i;
  };
  return [...groups.entries()]
    .sort((a, b) => rank(a[0]) - rank(b[0]) || a[0].localeCompare(b[0], "zh-CN"))
    .map(([name, items]) => ({ name, items }));
});

export const stats = computed(() => {
  const list = state.config.services;
  let online = 0;
  let offline = 0;
  let pending = 0;
  for (const s of list) {
    if (!s.enabled) continue;
    const st = statusOf(s);
    if (st === "online") online++;
    else if (st === "offline") offline++;
    else pending++;
  }
  return { total: list.length, online, offline, pending };
});

export async function init() {
  await refreshAuthStatus();
  try {
    state.config = await api.getConfig();
    state.configFile = await api.configPath();
    state.ready = true;
  } catch (e) {
    toast(`读取配置失败：${e}`, "error");
  }
  try {
    state.channel = await api.channelStatus();
  } catch {
    /* channel starts right after the UI, status will arrive by event */
  }
  await refreshVaultStatus();
  if (!authLocked.value) refreshHealth();
  window.setInterval(() => {
    if (!document.hidden && !authLocked.value) refreshHealth();
  }, 30_000);
}

export function restoreStartPage() {
  const cfg = state.config;
  if (cfg.settings.startPage !== "last") return;
  const lastId = localStorage.getItem("last-service");
  if (!lastId) return;
  const svc = cfg.services.find((s) => s.id === lastId && s.enabled);
  if (!svc) return;
  const endpoint = activeEndpoint(svc);
  if (!endpoint) return;
  if (svc.connectionMode === "tailnet" && !channelReady()) return;
  openService(lastId);
}

export async function syncRect(rect: Rect) {
  state.rect = rect;
  try {
    await api.setContentRect(rect);
  } catch {
    /* window not ready yet */
  }
}

export async function openService(id: string) {
  const svc = state.config.services.find((s) => s.id === id);
  if (!svc) return;
  const endpoint = activeEndpoint(svc);
  if (!endpoint) {
    toast(`「${svc.name || "该服务"}」还没有配置对应连接方式的地址`, "error");
    goSettings();
    return;
  }
  if (svc.connectionMode === "tailnet" && !channelReady()) {
    toast(
      state.channel.phase === "login"
        ? "请先在设置里完成 Tailscale 授权，私网服务就绪后再打开"
        : "私网通道尚未就绪，请到设置里检查；或把该服务改为公网直连",
      "error",
    );
    goSettings();
    return;
  }
  state.view = "service";
  state.activeId = id;
  localStorage.setItem("last-service", id);
  await nextTick();
  try {
    await api.openService(id);
  } catch (e) {
    if (String(e).includes("HTTP_CONFIRM_REQUIRED")) {
      await goHome();
      state.httpConfirmServiceId = id;
      return;
    }
    toast(`打开失败：${e}`, "error");
    await goHome();
  }
}

export async function confirmPublicHttp() {
  const id = state.httpConfirmServiceId;
  const svc = state.config.services.find((service) => service.id === id);
  const endpoint = svc && endpointOf(svc, "public");
  if (!id || !endpoint || !endpoint.url.trim().startsWith("http://")) {
    state.httpConfirmServiceId = null;
    return;
  }
  endpoint.httpAckAt = Date.now();
  endpoint.httpAckUrl = endpoint.url.trim();
  try {
    await api.saveConfig(JSON.parse(JSON.stringify(state.config)));
    state.httpConfirmServiceId = null;
    await openService(id);
  } catch (e) {
    endpoint.httpAckAt = null;
    endpoint.httpAckUrl = null;
    toast(`保存确认失败：${e}`, "error");
  }
}

export async function goHome() {
  state.view = "home";
  state.activeId = null;
  await api.hideAllServices().catch(() => {});
}

export async function goSettings() {
  state.view = "settings";
  state.activeId = null;
  await api.hideAllServices().catch(() => {});
}

export async function persist() {
  try {
    await api.saveConfig(JSON.parse(JSON.stringify(state.config)));
  } catch (e) {
    toast(`保存配置失败：${e}`, "error");
  }
}

export async function refreshHealth(ids?: string[]) {
  if (state.checking) return;
  const targets =
    ids ??
    state.config.services
      .filter((s) => {
        if (!s.enabled || !s.healthCheck) return false;
        const endpoint = activeEndpoint(s);
        if (!endpoint) return false;
        // Do not paint tailnet services red while the tunnel is simply not up.
        if (s.connectionMode === "tailnet" && !channelReady()) return false;
        return true;
      })
      .map((s) => s.id);
  if (!targets.length) {
    state.health = {};
    return;
  }
  state.checking = true;
  try {
    const results = await api.checkHealth(targets);
    for (const r of results) state.health[r.id] = r;
  } catch {
    /* keep previous state on transient errors */
  } finally {
    state.checking = false;
  }
}

export function blankService(partial: Partial<ServiceConfig> = {}): ServiceConfig {
  const maxOrder = state.config.services.reduce(
    (m, s) => Math.max(m, s.sortOrder),
    0,
  );
  return {
    id: crypto.randomUUID(),
    name: "",
    category: "其他",
    icon: "globe",
    healthCheck: true,
    sortOrder: maxOrder + 10,
    enabled: true,
    connectionMode: "tailnet",
    endpoints: [
      {
        endpointId: crypto.randomUUID(),
        kind: "tailnet",
        url: "",
        expectedOrigin: "",
        enabled: true,
      },
    ],
    ...partial,
  };
}

export async function upsertService(svc: ServiceConfig) {
  const i = state.config.services.findIndex((s) => s.id === svc.id);
  if (i >= 0) state.config.services[i] = { ...svc };
  else state.config.services.push({ ...svc });
  await persist();
  refreshHealth([svc.id]);
}

export async function removeService(id: string) {
  state.config.services = state.config.services.filter((s) => s.id !== id);
  delete state.health[id];
  await api.closeService(id).catch(() => {});
  if (state.activeId === id) {
    state.activeId = null;
    state.view = "settings";
  }
  await persist();
}

export async function toggleService(id: string, enabled: boolean) {
  const svc = state.config.services.find((s) => s.id === id);
  if (!svc) return;
  svc.enabled = enabled;
  await persist();
  if (!enabled) await api.closeService(id).catch(() => {});
  refreshHealth();
}

export async function moveService(id: string, dir: -1 | 1) {
  const sorted = state.config.services
    .slice()
    .sort((a, b) => a.sortOrder - b.sortOrder);
  const i = sorted.findIndex((s) => s.id === id);
  const j = i + dir;
  if (i < 0 || j < 0 || j >= sorted.length) return;
  [sorted[i], sorted[j]] = [sorted[j], sorted[i]];
  sorted.forEach((s, idx) => {
    s.sortOrder = (idx + 1) * 10;
  });
  await persist();
}

export async function setSetting<K extends keyof AppConfig["settings"]>(
  key: K,
  value: AppConfig["settings"][K],
) {
  state.config.settings[key] = value;
  await persist();
  if (key === "autostart") {
    try {
      await api.setAutostart(Boolean(value));
    } catch (e) {
      toast(`设置开机自启失败：${e}`, "error");
    }
  }
}

export function retryService(id: string) {
  refreshHealth([id]);
}

export async function channelStart() {
  try {
    state.channel = await api.channelStart();
  } catch (e) {
    toast(`启动通道失败：${e}`, "error");
  }
}

export async function channelStop() {
  try {
    await api.channelStop();
  } catch (e) {
    toast(`停止通道失败：${e}`, "error");
  }
}

export async function channelLogout() {
  try {
    await api.channelLogout();
    toast("已清除通道身份，需要重新授权", "success");
  } catch (e) {
    toast(`退出登录失败：${e}`, "error");
  }
}

export async function channelOpenLogin() {
  const url = state.channel.loginUrl;
  if (!url) return;
  try {
    await api.channelOpenLogin(url);
  } catch (e) {
    toast(`打开授权页失败：${e}`, "error");
  }
}

// ---- Vault ----------------------------------------------------------------

export async function refreshVaultStatus() {
  try {
    state.vault = await api.vaultStatus();
  } catch {
    return;
  }
  if (state.vault.unlocked) {
    await loadVaultData();
  } else {
    state.vaultCredentials = [];
    state.vaultGrants = [];
  }
}

async function loadVaultData() {
  try {
    state.vaultCredentials = await api.vaultListCredentials();
    state.vaultGrants = await api.vaultListAuthorizations();
  } catch {
    /* locked race: ignore */
  }
}

export async function vaultInitialize(password: string) {
  await api.vaultInitialize(password);
  await refreshVaultStatus();
}

export async function vaultUnlock(password: string) {
  await api.vaultUnlock(password);
  await refreshVaultStatus();
}

export async function vaultLock() {
  await api.vaultLock();
  await refreshVaultStatus();
}

export async function vaultSaveCredential(input: CredentialInput) {
  if (input.credentialId) await api.vaultUpdateCredential(input);
  else await api.vaultAddCredential(input);
  await loadVaultData();
}

export async function vaultDeleteCredential(credentialId: string) {
  await api.vaultDeleteCredential(credentialId);
  await loadVaultData();
}

export async function vaultReveal(credentialId: string): Promise<string> {
  return api.vaultRevealCredential(credentialId);
}

export async function vaultAuthorize(credentialId: string, days: number) {
  await api.vaultAuthorize(credentialId, days);
  await loadVaultData();
}

export async function vaultRevoke(grantId: string) {
  await api.vaultRevokeAuthorization(grantId);
  await loadVaultData();
}

export async function vaultResetAll() {
  await api.vaultReset();
  state.vaultCredentials = [];
  state.vaultGrants = [];
  await refreshVaultStatus();
}

// ---- Panel auth -----------------------------------------------------------

export const authLocked = computed(
  () => state.auth.enabled && !state.auth.authenticated,
);

export async function refreshAuthStatus() {
  try {
    state.auth = await api.authStatus();
  } catch {
    /* keep previous state on a transient IPC failure */
  }
}

export async function authLogin(username: string, password: string) {
  state.auth = await api.authLogin(username, password);
  if (!authLocked.value) {
    await refreshHealth();
    restoreStartPage();
  }
}

/** Returns the one-time recovery code (only issued when the account is created). */
export async function authInitialize(
  username: string,
  password: string,
  confirm: string,
): Promise<string | null> {
  const result = await api.authInitialize(username, password, confirm);
  state.auth = result.status;
  return result.recoveryCode;
}

export async function authEnableExisting(password: string): Promise<string | null> {
  const username = state.auth.username ?? "";
  const result = await api.authInitialize(username, password, password);
  state.auth = result.status;
  return result.recoveryCode;
}

export async function authRecoverWithCode(code: string) {
  state.auth = await api.authRecoverWithCode(code);
  if (!authLocked.value) {
    await refreshHealth();
    restoreStartPage();
  }
}

export async function authResetWithRecovery(code: string) {
  await api.authResetWithRecovery(code);
  await refreshAuthStatus();
}

export async function authFactoryReset() {
  await api.authFactoryReset();
  await refreshAuthStatus();
  await refreshVaultStatus();
}

export async function authLockPanel() {
  await api.authLock();
  await refreshAuthStatus();
}

export async function authDisablePanel(password: string) {
  await api.authDisable(password);
  await refreshAuthStatus();
}

export async function authChangePanelPassword(
  current: string,
  next: string,
  confirm: string,
) {
  await api.authChangePassword(current, next, confirm);
  await refreshAuthStatus();
}

export async function authSetAutoLock(minutes: number) {
  state.auth = await api.authUpdateLockSettings(minutes);
}
