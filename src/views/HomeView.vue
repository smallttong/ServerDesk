<script setup lang="ts">
import { computed } from "vue";
import {
  ArrowUpRight,
  RefreshCw,
  Shield,
  ShieldAlert,
  Sparkles,
} from "lucide-vue-next";
import {
  activeEndpoint,
  channelLabel,
  channelReady,
  goSettings,
  isUrlOk,
  modeLabel,
  openService,
  refreshHealth,
  serviceUrl,
  state,
  stats,
  statusOf,
} from "../lib/store";
import { resolveIcon } from "../lib/icons";
import StatusDot from "../components/StatusDot.vue";

const cards = computed(() =>
  state.config.services
    .filter((s) => s.enabled)
    .slice()
    .sort(
      (a, b) =>
        a.sortOrder - b.sortOrder || a.name.localeCompare(b.name, "zh-CN"),
    ),
);

const needsTailnet = computed(
  () =>
    !channelReady() &&
    state.config.services.some(
      (svc) => svc.enabled && activeEndpoint(svc)?.kind === 'tailnet',
    ),
);

const latency = (id: string) => {
  const h = state.health[id];
  if (!h || !h.online || h.latencyMs == null) return "";
  return `${h.latencyMs} ms`;
};
</script>

<template>
  <div class="mx-auto w-full max-w-5xl px-7 py-7">
    <div class="flex items-end justify-between gap-4">
      <div>
        <h1 class="text-[19px] font-semibold tracking-tight">你好，欢迎回来</h1>
        <p class="mt-1 text-[12px] text-muted">
          这里是所有服务器管理后台的统一入口，点击卡片即可在程序内打开。
        </p>
      </div>
      <button class="btn" @click="refreshHealth()">
        <RefreshCw class="size-3.5" :class="state.checking ? 'animate-spin' : ''" />
        检测状态
      </button>
    </div>

    <div
      v-if="needsTailnet"
      class="mt-4 flex items-start gap-2.5 rounded-xl border border-amber-500/25 bg-amber-500/[0.06] px-4 py-3"
    >
      <ShieldAlert class="mt-0.5 size-4 shrink-0 text-amber-400/90" />
      <div class="min-w-0 flex-1">
        <div class="text-[12px] font-medium text-amber-300">{{ channelLabel }}</div>
        <p class="mt-0.5 text-[11px] leading-relaxed text-muted">
          服务页面通过内置通道访问，通道就绪后才能打开。到设置里完成一次 Tailscale 授权即可。
        </p>
      </div>
      <button class="btn h-7 shrink-0 px-2.5" @click="goSettings()">去设置</button>
    </div>

    <div class="mt-6 grid grid-cols-3 gap-3">
      <div class="card px-4 py-3.5">
        <div class="text-[11px] text-muted">服务总数</div>
        <div class="mt-1 flex items-baseline gap-1.5">
          <span class="text-[24px] font-semibold leading-none">{{ stats.total }}</span>
          <span class="text-[11px] text-muted">个</span>
        </div>
      </div>
      <div class="card px-4 py-3.5">
        <div class="text-[11px] text-muted">在线</div>
        <div class="mt-1 flex items-baseline gap-1.5">
          <span class="text-[24px] font-semibold leading-none text-emerald-400">
            {{ stats.online }}
          </span>
          <span class="text-[11px] text-muted">个</span>
        </div>
      </div>
      <div class="card px-4 py-3.5">
        <div class="text-[11px] text-muted">离线 / 待检测</div>
        <div class="mt-1 flex items-baseline gap-1.5">
          <span class="text-[24px] font-semibold leading-none text-rose-400">
            {{ stats.offline }}
          </span>
          <span class="text-[11px] text-muted">/ {{ stats.pending }}</span>
        </div>
      </div>
    </div>

    <div class="mt-7 flex items-center justify-between">
      <h2 class="text-[12px] font-semibold uppercase tracking-[0.12em] text-muted">
        常用服务
      </h2>
      <button class="text-[11.5px] text-accent hover:underline" @click="goSettings()">
        管理服务
      </button>
    </div>

    <div v-if="cards.length" class="mt-3 grid grid-cols-2 gap-3">
      <button
        v-for="svc in cards"
        :key="svc.id"
        class="card group flex flex-col gap-3 p-4 text-left transition-colors hover:border-accent/40 hover:bg-panel-2/50"
        @click="openService(svc.id)"
      >
        <div class="flex items-start gap-3">
          <div
            class="grid size-9 shrink-0 place-items-center rounded-lg bg-panel-2 text-accent transition-colors group-hover:bg-accent/15"
          >
            <component :is="resolveIcon(svc.icon)" class="size-4" />
          </div>
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="truncate text-[13px] font-medium">
                {{ svc.name || "未命名服务" }}
              </span>
              <span
                v-if="svc.category"
                class="shrink-0 rounded bg-panel-2 px-1.5 py-0.5 text-[10px] text-muted"
              >
                {{ svc.category }}
              </span>
              <span
                class="shrink-0 rounded px-1.5 py-0.5 text-[10px]"
                :class="svc.connectionMode === 'public' ? 'bg-sky-500/15 text-sky-300' : 'bg-accent/15 text-accent'"
              >
                {{ modeLabel(svc) }}
              </span>
            </div>
            <div class="mt-0.5 truncate font-mono text-[10.5px] text-muted">
              {{ serviceUrl(svc) || "尚未配置地址" }}
            </div>
          </div>
          <StatusDot :status="statusOf(svc)" show-label />
        </div>

        <div class="flex items-center justify-between text-[11px] text-muted">
          <span v-if="!activeEndpoint(svc)" class="text-amber-400/90">
            待配置地址
          </span>
          <span v-else-if="latency(svc.id)">延迟 {{ latency(svc.id) }}</span>
          <span v-else class="text-muted/60">未检测</span>
          <span
            class="flex items-center gap-0.5 text-accent opacity-0 transition-opacity group-hover:opacity-100"
          >
            打开 <ArrowUpRight class="size-3" />
          </span>
        </div>
      </button>
    </div>

    <div
      v-else
      class="card mt-3 flex flex-col items-center gap-2 border-dashed px-6 py-14 text-center"
    >
      <Sparkles class="size-6 text-muted/60" />
      <div class="text-[13px] font-medium">还没有添加任何服务</div>
      <p class="max-w-sm text-[11.5px] leading-relaxed text-muted">
        AstrBot、NapCat、1Panel、哪吒监控，以及任何自建的 Web 管理后台，都可以加入这里。
      </p>
      <button class="btn btn-primary mt-1" @click="goSettings()">添加服务</button>
    </div>

    <div
      class="mt-7 flex items-start gap-2.5 rounded-xl border border-line/70 bg-panel/40 px-4 py-3"
    >
      <Shield class="mt-0.5 size-4 shrink-0 text-emerald-400/80" />
      <p class="text-[11.5px] leading-relaxed text-muted">
        所有服务建议通过 Tailscale 等私有网络访问，不要直接把管理后台暴露到公网。本程序不保存任何账号、密码或 Token，登录状态由各服务自己的会话维护。
      </p>
    </div>
  </div>
</template>
