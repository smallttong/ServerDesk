<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import {
  ChevronLeft,
  ChevronRight,
  Copy,
  ExternalLink,
  House,
  LoaderCircle,
  Lock,
  RefreshCw,
} from "lucide-vue-next";
import { api } from "../lib/api";
import { authLockPanel, refreshHealth, serviceUrl, state, toast } from "../lib/store";
import { resolveIcon } from "../lib/icons";

const currentUrl = ref("");
const loading = ref(false);

const svc = computed(
  () => state.config.services.find((s) => s.id === state.activeId) ?? null,
);
const pageTitle = computed(() =>
  state.view === "settings" ? "设置" : "首页",
);
const pageSubtitle = computed(() =>
  state.view === "settings" ? "服务配置与程序偏好" : "统一入口 · 配置本地保存",
);

async function pollUrl() {
  if (state.view !== "service") return;
  try {
    currentUrl.value = (await api.activeUrl()) ?? "";
  } catch {
    /* ignore */
  }
}

watch(
  () => [state.view, state.activeId],
  () => {
    currentUrl.value = "";
    pollUrl();
  },
);

const timer = window.setInterval(pollUrl, 2000);
onBeforeUnmount(() => window.clearInterval(timer));

async function nav(action: () => Promise<void>) {
  try {
    await action();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function reload() {
  loading.value = true;
  await nav(api.navReload);
  setTimeout(() => (loading.value = false), 550);
}

async function copyUrl() {
  const url = currentUrl.value;
  if (!url) return;
  try {
    await navigator.clipboard.writeText(url);
    toast("地址已复制", "success");
  } catch {
    toast("复制失败", "error");
  }
}

async function openExternal() {
  const url = currentUrl.value || (svc.value ? serviceUrl(svc.value) : "");
  if (!url) return;
  await nav(() => api.openExternal(url));
}

async function lockPanel() {
  try {
    await authLockPanel();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function recheckAll() {
  await refreshHealth();
  toast("已重新检测服务状态", "success");
}
</script>

<template>
  <header
    class="flex h-14 shrink-0 items-center gap-1 border-b border-line bg-panel/60 px-3"
  >
    <template v-if="state.view === 'service'">
      <button class="icon-btn" title="后退" @click="nav(api.navBack)">
        <ChevronLeft class="size-4" />
      </button>
      <button class="icon-btn" title="前进" @click="nav(api.navForward)">
        <ChevronRight class="size-4" />
      </button>
      <button class="icon-btn" title="刷新" @click="reload">
        <LoaderCircle v-if="loading" class="size-4 animate-spin" />
        <RefreshCw v-else class="size-4" />
      </button>
      <button class="icon-btn" title="回到服务首页" @click="nav(api.navHome)">
        <House class="size-4" />
      </button>

      <div class="mx-1.5 h-5 w-px bg-line" />

      <component
        :is="resolveIcon(svc?.icon)"
        class="ml-0.5 size-4 shrink-0 text-accent"
      />
      <span class="max-w-40 truncate text-[12.5px] font-medium">
        {{ svc?.name || "服务" }}
      </span>
      <span
        class="ml-2 hidden min-w-0 max-w-[380px] truncate rounded-md bg-app/70 px-2 py-1 font-mono text-[11px] text-muted lg:inline-block"
        :title="currentUrl"
      >
        {{ currentUrl || (svc ? serviceUrl(svc) : "") }}
      </span>

      <div class="ml-auto flex items-center gap-1">
        <button
          v-if="state.auth.enabled"
          class="icon-btn"
          title="锁定 ServerDesk"
          @click="lockPanel"
        >
          <Lock class="size-4" />
        </button>
        <button class="icon-btn" title="复制当前地址" @click="copyUrl">
          <Copy class="size-4" />
        </button>
        <button class="icon-btn" title="在系统浏览器中打开" @click="openExternal">
          <ExternalLink class="size-4" />
        </button>
      </div>
    </template>

    <template v-else>
      <div class="min-w-0">
        <div class="text-[13px] font-semibold leading-tight">{{ pageTitle }}</div>
        <div class="text-[11px] leading-tight text-muted">{{ pageSubtitle }}</div>
      </div>
      <div class="ml-auto flex items-center gap-2">
        <button
          v-if="state.auth.enabled"
          class="btn h-7 px-2.5"
          title="锁定 ServerDesk"
          @click="lockPanel"
        >
          <Lock class="size-3.5" />
          锁定
        </button>
        <button class="btn h-7 px-2.5" @click="recheckAll">
          <RefreshCw class="size-3.5" :class="state.checking ? 'animate-spin' : ''" />
          重新检测
        </button>
      </div>
    </template>
  </header>
</template>
