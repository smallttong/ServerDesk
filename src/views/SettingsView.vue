<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  ArrowDown,
  ArrowUp,
  Check,
  Copy,
  FolderCog,
  Pencil,
  Plus,
  RotateCcw,
  Trash2,
  X,
} from "lucide-vue-next";
import {
  channelLabel,
  channelLogout,
  channelOpenLogin,
  channelStart,
  modeLabel,
  moveService,
  openService,
  removeService,
  serviceUrl,
  setSetting,
  state,
  statusOf,
  toast,
  toggleService,
  upsertService,
} from "../lib/store";
import { resolveIcon } from "../lib/icons";
import type { ServiceConfig } from "../lib/types";
import StatusDot from "../components/StatusDot.vue";
import ServiceDialog from "../components/ServiceDialog.vue";
import VaultPanel from "../features/vault/VaultPanel.vue";
import AuthSettings from "../features/auth/AuthSettings.vue";

const dialogOpen = ref(false);
const editing = ref<ServiceConfig | null>(null);
const confirming = ref<string | null>(null);
const confirmLogout = ref(false);

const hostnameDraft = ref(state.config.settings.channelHostname);
const tagsDraft = ref(state.config.settings.channelTags);

watch(
  () => state.config.settings.channelHostname,
  (value) => (hostnameDraft.value = value),
);
watch(
  () => state.config.settings.channelTags,
  (value) => (tagsDraft.value = value),
);

const channelTone = computed(() => {
  if (!state.config.settings.channelEnabled) return "text-muted";
  switch (state.channel.phase) {
    case "up":
      return "text-emerald-400";
    case "login":
      return "text-amber-400";
    case "error":
      return "text-rose-400";
    case "starting":
      return "text-sky-400";
    default:
      return "text-muted";
  }
});

const channelDetail = computed(() => {
  if (!state.config.settings.channelEnabled) {
    return "内置通道已关闭，服务页面会直接访问配置里的地址";
  }
  const channel = state.channel;
  switch (channel.phase) {
    case "up":
      return `节点 ${channel.hostname || "serverdesk"} 已加入 tailnet${
        channel.socksPort ? ` · 本地代理 127.0.0.1:${channel.socksPort}` : ""
      }`;
    case "login":
      return channel.message || "需要在浏览器里完成一次 Tailscale 授权";
    case "error":
      return channel.message || "通道启动失败";
    case "starting":
      return "正在连接 Tailscale 协调服务器…";
    default:
      return "通道未启动";
  }
});

async function saveHostname() {
  const value = hostnameDraft.value.trim();
  if (!value || value === state.config.settings.channelHostname) return;
  await setSetting("channelHostname", value);
}

async function saveTags() {
  const value = tagsDraft.value.trim();
  if (value === state.config.settings.channelTags) return;
  await setSetting("channelTags", value);
}

async function doLogout() {
  confirmLogout.value = false;
  await channelLogout();
}

const rows = () =>
  state.config.services
    .slice()
    .sort((a, b) => a.sortOrder - b.sortOrder);

function addService() {
  editing.value = null;
  dialogOpen.value = true;
}

function editService(svc: ServiceConfig) {
  editing.value = svc;
  dialogOpen.value = true;
}

async function saveService(svc: ServiceConfig) {
  await upsertService(svc);
  dialogOpen.value = false;
  toast("服务已保存", "success");
}

async function deleteService(id: string) {
  await removeService(id);
  confirming.value = null;
  toast("服务已删除", "success");
}

async function copyPath() {
  try {
    await navigator.clipboard.writeText(state.configFile);
    toast("配置路径已复制", "success");
  } catch {
    toast("复制失败", "error");
  }
}

async function setStartPage(value: "home" | "last") {
  await setSetting("startPage", value);
}
</script>

<template>
  <div class="mx-auto w-full max-w-4xl px-7 py-7">
    <section>
      <div class="flex items-center justify-between">
        <div>
          <h2 class="text-[13px] font-semibold">服务列表</h2>
          <p class="mt-0.5 text-[11.5px] text-muted">
            名称、地址、分类、图标与排序全部保存在本地配置文件里。
          </p>
        </div>
        <button class="btn btn-primary" @click="addService">
          <Plus class="size-3.5" />
          添加服务
        </button>
      </div>

      <div class="card mt-3 overflow-hidden">
        <div
          v-for="(svc, index) in rows()"
          :key="svc.id"
          class="flex items-center gap-3 border-b border-line/60 px-3.5 py-2.5 last:border-0 hover:bg-panel-2/35"
        >
          <div class="flex flex-col">
            <button
              class="grid size-5 place-items-center rounded text-muted hover:bg-panel-2 hover:text-ink disabled:opacity-25"
              :disabled="index === 0"
              title="上移"
              @click="moveService(svc.id, -1)"
            >
              <ArrowUp class="size-3" />
            </button>
            <button
              class="grid size-5 place-items-center rounded text-muted hover:bg-panel-2 hover:text-ink disabled:opacity-25"
              :disabled="index === rows().length - 1"
              title="下移"
              @click="moveService(svc.id, 1)"
            >
              <ArrowDown class="size-3" />
            </button>
          </div>

          <div
            class="grid size-8 shrink-0 place-items-center rounded-lg bg-panel-2 text-accent"
          >
            <component :is="resolveIcon(svc.icon)" class="size-4" />
          </div>

          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="truncate text-[12.5px] font-medium">
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
            <div class="truncate font-mono text-[10.5px] text-muted">
              {{ serviceUrl(svc) || "尚未配置地址" }}
            </div>
          </div>

          <StatusDot :status="statusOf(svc)" show-label />

          <button
            class="icon-btn"
            :title="svc.enabled ? '停用' : '启用'"
            @click="toggleService(svc.id, !svc.enabled)"
          >
            <span class="switch" :data-on="svc.enabled" />
          </button>

          <button class="icon-btn" title="打开" @click="openService(svc.id)">
            <FolderCog class="size-3.5" />
          </button>
          <button class="icon-btn" title="编辑" @click="editService(svc)">
            <Pencil class="size-3.5" />
          </button>

          <template v-if="confirming === svc.id">
            <button class="btn btn-danger h-7 px-2" @click="deleteService(svc.id)">
              <Check class="size-3.5" /> 确认删除
            </button>
            <button class="icon-btn" title="取消" @click="confirming = null">
              <X class="size-3.5" />
            </button>
          </template>
          <button
            v-else
            class="icon-btn"
            title="删除"
            @click="confirming = svc.id"
          >
            <Trash2 class="size-3.5" />
          </button>
        </div>

        <div
          v-if="!rows().length"
          class="flex flex-col items-center gap-1.5 px-6 py-12 text-center"
        >
          <div class="text-[12.5px] font-medium">服务列表是空的</div>
          <p class="text-[11.5px] text-muted">
            点击右上角「添加服务」，填入任意 Web 管理后台的地址即可。
          </p>
        </div>
      </div>
    </section>

    <div class="mt-8">
      <AuthSettings />
    </div>

    <div class="mt-8">
      <VaultPanel />
    </div>

    <section class="mt-8">
      <div class="flex items-center justify-between">
        <div>
          <h2 class="text-[13px] font-semibold">网络通道</h2>
          <p class="mt-0.5 text-[11.5px] text-muted">
            程序内置一个用户态 Tailscale 节点，服务器不用暴露公网，系统里也不用装 VPN 客户端。
          </p>
        </div>
        <span class="text-[11.5px] font-medium" :class="channelTone">
          {{ channelLabel }}
        </span>
      </div>

      <div class="card mt-3 divide-y divide-line/60">
        <div class="flex items-center justify-between gap-4 px-4 py-3">
          <div class="min-w-0">
            <div class="flex items-center gap-2 text-[12.5px]">
              <span
                class="size-1.5 rounded-full"
                :class="
                  channelTone === 'text-emerald-400'
                    ? 'bg-emerald-400'
                    : channelTone === 'text-amber-400'
                      ? 'bg-amber-400'
                      : channelTone === 'text-rose-400'
                        ? 'bg-rose-400'
                        : channelTone === 'text-sky-400'
                          ? 'bg-sky-400'
                          : 'bg-slate-500'
                "
              />
              {{ channelLabel }}
            </div>
            <div class="mt-1 text-[11px] leading-relaxed text-muted">
              {{ channelDetail }}
            </div>
          </div>
          <div class="flex shrink-0 items-center gap-2">
            <button
              v-if="state.channel.loginUrl"
              class="btn btn-primary h-7 px-2.5"
              @click="channelOpenLogin()"
            >
              在浏览器中授权
            </button>
            <button class="btn h-7 px-2.5" @click="channelStart()">
              <RotateCcw class="size-3.5" />
              重启通道
            </button>
            <template v-if="confirmLogout">
              <button class="btn btn-danger h-7 px-2" @click="doLogout()">
                <Check class="size-3.5" /> 确认退出
              </button>
              <button class="icon-btn" @click="confirmLogout = false">
                <X class="size-3.5" />
              </button>
            </template>
            <button
              v-else
              class="btn h-7 px-2.5"
              :disabled="state.channel.phase === 'stopped'"
              @click="confirmLogout = true"
            >
              退出登录
            </button>
          </div>
        </div>

        <div class="flex items-center justify-between px-4 py-3">
          <div>
            <div class="text-[12.5px]">启用内置通道</div>
            <div class="text-[11px] text-muted">
              关闭后服务页面将直接访问配置里的地址（要求那些地址本身可达）
            </div>
          </div>
          <button
            class="switch"
            :data-on="state.config.settings.channelEnabled"
            @click="setSetting('channelEnabled', !state.config.settings.channelEnabled)"
          />
        </div>

        <div class="flex items-center justify-between gap-4 px-4 py-3">
          <div>
            <div class="text-[12.5px]">节点名称</div>
            <div class="text-[11px] text-muted">在 Tailscale 管理台里显示的名字</div>
          </div>
          <input
            v-model="hostnameDraft"
            class="field w-56"
            spellcheck="false"
            @blur="saveHostname"
            @keydown.enter="($event.target as HTMLInputElement).blur()"
          />
        </div>

        <div class="flex items-center justify-between gap-4 px-4 py-3">
          <div>
            <div class="text-[12.5px]">设备标签（可选）</div>
            <div class="text-[11px] text-muted">
              例如 tag:serverdesk，配合 ACL 只允许这个节点访问指定端口
            </div>
          </div>
          <input
            v-model="tagsDraft"
            class="field w-56 font-mono text-[11.5px]"
            placeholder="tag:serverdesk"
            spellcheck="false"
            @blur="saveTags"
            @keydown.enter="($event.target as HTMLInputElement).blur()"
          />
        </div>
      </div>
    </section>

    <section class="mt-8">
      <h2 class="text-[13px] font-semibold">通用</h2>
      <div class="card mt-3 divide-y divide-line/60">
        <div class="flex items-center justify-between px-4 py-3">
          <div>
            <div class="text-[12.5px]">开机自启</div>
            <div class="text-[11px] text-muted">登录 Windows 后自动启动控制中心</div>
          </div>
          <button
            class="switch"
            :data-on="state.config.settings.autostart"
            @click="setSetting('autostart', !state.config.settings.autostart)"
          />
        </div>

        <div class="flex items-center justify-between px-4 py-3">
          <div>
            <div class="text-[12.5px]">关闭窗口后驻留托盘</div>
            <div class="text-[11px] text-muted">
              关闭主窗口时隐藏到系统托盘，服务保持后台运行
            </div>
          </div>
          <button
            class="switch"
            :data-on="state.config.settings.closeToTray"
            @click="setSetting('closeToTray', !state.config.settings.closeToTray)"
          />
        </div>

        <div class="flex items-center justify-between px-4 py-3">
          <div>
            <div class="text-[12.5px]">启动时打开的页面</div>
            <div class="text-[11px] text-muted">程序启动后默认显示的位置</div>
          </div>
          <div class="flex overflow-hidden rounded-lg border border-line">
            <button
              class="px-3 py-1.5 text-[11.5px]"
              :class="
                state.config.settings.startPage === 'home'
                  ? 'bg-accent/20 text-accent'
                  : 'text-muted hover:bg-panel-2'
              "
              @click="setStartPage('home')"
            >
              首页
            </button>
            <button
              class="border-l border-line px-3 py-1.5 text-[11.5px]"
              :class="
                state.config.settings.startPage === 'last'
                  ? 'bg-accent/20 text-accent'
                  : 'text-muted hover:bg-panel-2'
              "
              @click="setStartPage('last')"
            >
              上次的服务
            </button>
          </div>
        </div>
      </div>
    </section>

    <section class="mt-8">
      <h2 class="text-[13px] font-semibold">关于</h2>
      <div class="card mt-3 divide-y divide-line/60">
        <div class="flex items-center justify-between px-4 py-3">
          <div class="text-[12.5px]">版本</div>
          <div class="font-mono text-[11.5px] text-muted">0.1.0</div>
        </div>
        <div class="flex items-center justify-between gap-4 px-4 py-3">
          <div class="text-[12.5px]">配置文件</div>
          <div class="flex min-w-0 items-center gap-2">
            <span class="truncate font-mono text-[10.5px] text-muted">
              {{ state.configFile || "…" }}
            </span>
            <button class="icon-btn size-7" title="复制路径" @click="copyPath">
              <Copy class="size-3.5" />
            </button>
          </div>
        </div>
      </div>
    </section>
  </div>

  <ServiceDialog
    :open="dialogOpen"
    :service="editing"
    @close="dialogOpen = false"
    @save="saveService"
  />
</template>
