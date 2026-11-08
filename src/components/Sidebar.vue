<script setup lang="ts">
import { computed } from "vue";
import { House, Layers, Settings as SettingsIcon } from "lucide-vue-next";
import {
  channelLabel,
  goHome,
  goSettings,
  groupedServices,
  openService,
  serviceUrl,
  state,
  statusOf,
} from "../lib/store";
import { resolveIcon } from "../lib/icons";
import StatusDot from "./StatusDot.vue";

const itemCls =
  "group flex w-full items-center gap-2.5 rounded-lg px-2.5 h-8 text-[12.5px] text-muted transition-colors hover:bg-panel-2 hover:text-ink";
const activeCls = "bg-panel-2 text-ink shadow-[inset_0_0_0_1px_rgba(255,255,255,0.04)]";

const channelDot = computed(() => {
  if (!state.config.settings.channelEnabled) return "bg-slate-600";
  switch (state.channel.phase) {
    case "up":
      return "bg-emerald-400 shadow-[0_0_6px_rgba(52,211,153,0.6)]";
    case "login":
      return "bg-amber-400";
    case "error":
      return "bg-rose-400";
    case "starting":
      return "bg-sky-400";
    default:
      return "bg-slate-500";
  }
});
</script>

<template>
  <aside class="relative flex h-full w-60 shrink-0 flex-col border-r border-line bg-panel/95">
    <div class="flex h-14 shrink-0 items-center gap-2.5 border-b border-line px-4">
      <div
        class="grid size-7 shrink-0 place-items-center rounded-lg bg-gradient-to-br from-accent to-indigo-500 shadow-lg shadow-accent/25"
      >
        <Layers class="size-4 text-white" />
      </div>
      <div class="min-w-0 leading-tight">
        <div class="truncate text-[13px] font-semibold">服务器控制中心</div>
        <div class="text-[10px] tracking-wide text-muted">SERVER DESK</div>
      </div>
    </div>

    <div class="px-3 pt-3">
      <button
        :class="[itemCls, state.view === 'home' ? activeCls : '']"
        @click="goHome()"
      >
        <House class="size-4 shrink-0" />
        <span>首页</span>
      </button>
    </div>

    <nav class="flex-1 space-y-5 overflow-y-auto px-3 py-4">
      <div v-for="group in groupedServices" :key="group.name">
        <div
          class="px-2 pb-1.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted/60"
        >
          {{ group.name }}
        </div>
        <div class="space-y-0.5">
          <button
            v-for="svc in group.items"
            :key="svc.id"
            :title="serviceUrl(svc)"
            :class="[
              itemCls,
              state.view === 'service' && state.activeId === svc.id ? activeCls : '',
            ]"
            @click="openService(svc.id)"
          >
            <component
              :is="resolveIcon(svc.icon)"
              class="size-4 shrink-0 opacity-90"
            />
            <span class="flex-1 truncate text-left">{{ svc.name || "未命名" }}</span>
            <StatusDot :status="statusOf(svc)" />
          </button>
        </div>
      </div>

      <div
        v-if="!groupedServices.length"
        class="rounded-lg border border-dashed border-line/80 px-3 py-6 text-center text-[11px] leading-relaxed text-muted"
      >
        还没有启用的服务<br />
        <button class="mt-1 text-accent hover:underline" @click="goSettings()">
          去设置里添加
        </button>
      </div>
    </nav>

    <div class="pointer-events-none absolute bottom-16 left-2 right-2 z-[60] flex flex-col gap-2">
      <TransitionGroup name="fade">
        <div v-for="t in state.toasts" :key="t.id" class="card flex items-start gap-2 px-3 py-2 text-[11px] shadow-xl">
          <span class="mt-1 size-1.5 shrink-0 rounded-full" :class="t.kind === 'error' ? 'bg-rose-400' : t.kind === 'success' ? 'bg-emerald-400' : 'bg-slate-400'" />
          <span class="break-words leading-relaxed">{{ t.text }}</span>
        </div>
      </TransitionGroup>
    </div>
    <div class="border-t border-line p-3">
      <button
        class="mb-1 flex w-full items-center gap-2 rounded-lg px-2.5 h-7 text-[11px] text-muted transition-colors hover:bg-panel-2 hover:text-ink"
        :title="state.channel.message || channelLabel"
        @click="goSettings()"
      >
        <span class="size-1.5 shrink-0 rounded-full" :class="channelDot" />
        <span class="truncate">{{ channelLabel }}</span>
      </button>
      <button
        :class="[itemCls, state.view === 'settings' ? activeCls : '']"
        @click="goSettings()"
      >
        <SettingsIcon class="size-4 shrink-0" />
        <span>设置</span>
      </button>
    </div>
  </aside>
</template>
