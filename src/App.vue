<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import LockScreen from "./features/auth/LockScreen.vue";
import Sidebar from "./components/Sidebar.vue";
import TopBar from "./components/TopBar.vue";
import HomeView from "./views/HomeView.vue";
import SettingsView from "./views/SettingsView.vue";
import {
  activeEndpoint,
  authLocked,
  confirmPublicHttp,
  refreshAuthStatus,
  goHome,
  goSettings,
  init,
  openService,
  refreshHealth,
  refreshVaultStatus,
  restoreStartPage,
  state,
  syncRect,
} from "./lib/store";
import type { AuthStatus, ChannelStatus } from "./lib/types";

const mainRef = ref<HTMLElement | null>(null);
const unlisteners: UnlistenFn[] = [];
let observer: ResizeObserver | undefined;

function measure() {
  const el = mainRef.value;
  if (!el) return;
  const r = el.getBoundingClientRect();
  syncRect({ x: r.left, y: r.top, width: r.width, height: r.height });
}

onMounted(async () => {
  await init();
  await nextTick();
  measure();

  observer = new ResizeObserver(() => measure());
  if (mainRef.value) observer.observe(mainRef.value);
  window.addEventListener("resize", measure);

  if (!authLocked.value) restoreStartPage();

  unlisteners.push(
    await listen<{ view?: string; service?: string }>("nav", (event) => {
      const payload = event.payload || {};
      if (payload.service) openService(payload.service);
      else if (payload.view === "settings") goSettings();
      else if (payload.view === "home") goHome();
    }),
  );

  unlisteners.push(
    await listen("recheck", () => {
      refreshHealth();
    }),
  );

  // Restored from the tray: re-attach the service window to the shell layout.
  unlisteners.push(
    await listen("main-shown", () => {
      measure();
      if (state.view === "service" && state.activeId) {
        openService(state.activeId);
      }
    }),
  );

  // Panel auth reports lock/unlock transitions from the backend.
  unlisteners.push(
    await listen<AuthStatus>("auth-status", (event) => {
      state.auth = event.payload;
      if (!event.payload.authenticated) {
        state.view = "home";
        state.activeId = null;
      }
    }),
  );

  // Vault auto-lock and reset both report through this event.
  unlisteners.push(
    await listen("vault-status", () => {
      refreshVaultStatus();
    }),
  );

  // The built-in tailnet channel reports login URLs and connection state here.
  unlisteners.push(
    await listen<ChannelStatus>("channel-status", (event) => {
      state.channel = event.payload;
      if (!state.config.settings.channelEnabled) {
        state.health = {};
        return;
      }
      if (state.channel.phase === "up") {
        // Windows created before the tunnel came up are recreated by the
        // backend, so the active one has to be opened again.
        if (state.view === "service" && state.activeId) {
          openService(state.activeId);
        }
      } else {
        state.health = {};
        refreshHealth();
        if (state.view === "service") {
          const active = state.config.services.find((s) => s.id === state.activeId);
          const endpoint = active ? activeEndpoint(active) : undefined;
          // Public services keep working without the tunnel.
          if (active?.connectionMode === "failover") {
            openService(active.id);
          } else if (!endpoint || endpoint.kind === "tailnet") {
            state.view = "home";
            state.activeId = null;
          }
        }
      }
    }),
  );
});

watch(authLocked, async (locked) => {
  if (!locked) {
    await nextTick();
    measure();
  }
});

onBeforeUnmount(() => {
  observer?.disconnect();
  window.removeEventListener("resize", measure);
  unlisteners.forEach((fn) => fn());
});
</script>

<template>
  <LockScreen v-if="authLocked" />

  <div v-else class="flex h-full w-full overflow-hidden bg-app text-ink">
    <Sidebar />

    <div class="flex min-w-0 flex-1 flex-col">
      <TopBar />

      <main ref="mainRef" class="min-h-0 flex-1 overflow-y-auto">
        <HomeView v-if="state.view === 'home'" />
        <SettingsView v-else-if="state.view === 'settings'" />
        <div
          v-else
          class="flex h-full flex-col items-center justify-center gap-2 text-muted"
        >
          <div class="text-[12px]">正在加载服务页面…</div>
          <div class="font-mono text-[10.5px] opacity-60">WebView2</div>
        </div>
      </main>
    </div>

    <div v-if="state.httpConfirmServiceId" class="fixed inset-0 z-[80] flex items-center justify-center bg-black/70 p-5">
      <div class="card w-full max-w-md p-5 shadow-2xl">
        <h2 class="text-sm font-semibold">确认使用公网 HTTP</h2>
        <p class="mt-3 text-xs leading-relaxed text-muted">此连接未加密，账号、密码和页面内容可能被网络中的其他人读取。确认后才会打开；地址变更时会再次询问。</p>
        <p class="mt-3 break-all rounded bg-app p-2 font-mono text-[11px]">{{ state.config.services.find(s => s.id === state.httpConfirmServiceId)?.endpoints.find(e => e.kind === 'public')?.url }}</p>
        <div class="mt-5 flex justify-end gap-2">
          <button class="rounded border border-line px-3 py-1.5 text-xs" @click="state.httpConfirmServiceId = null">取消</button>
          <button class="rounded bg-accent px-3 py-1.5 text-xs text-white" @click="confirmPublicHttp">了解风险，继续</button>
        </div>
      </div>
    </div>

  </div>
</template>
