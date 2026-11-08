<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { X } from "lucide-vue-next";
import type { ConnectionMode, Endpoint, ServiceConfig } from "../lib/types";
import { ICON_CHOICES, resolveIcon } from "../lib/icons";
import { blankService, looksLikePublicHost } from "../lib/store";

const props = defineProps<{
  open: boolean;
  service?: ServiceConfig | null;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "save", svc: ServiceConfig): void;
}>();

const form = reactive<ServiceConfig>(blankService());
const tailnetUrl = ref("");
const publicUrl = ref("");
const error = ref("");

const categories = ["AI", "服务器", "运维", "媒体", "其他"];

const modes: { value: ConnectionMode; label: string; hint: string }[] = [
  { value: "tailnet", label: "私网", hint: "经内置通道访问" },
  { value: "public", label: "公网", hint: "直接访问" },
  { value: "failover", label: "自动切换", hint: "私网优先" },
];

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    error.value = "";
    const source = props.service ? { ...props.service } : blankService();
    Object.assign(form, JSON.parse(JSON.stringify(source)));
    tailnetUrl.value = form.endpoints.find((e) => e.kind === "tailnet")?.url ?? "";
    publicUrl.value = form.endpoints.find((e) => e.kind === "public")?.url ?? "";
  },
);

const title = computed(() => (props.service ? "编辑服务" : "添加服务"));
const tailnetLooksPublic = computed(
  () => form.connectionMode !== "public" && looksLikePublicHost(tailnetUrl.value),
);

function originOf(url: string): string {
  try {
    const parsed = new URL(url.trim());
    return `${parsed.protocol}//${parsed.host}`;
  } catch {
    return "";
  }
}

/** Keep the endpoint id stable when the address is edited. */
function buildEndpoints(): Endpoint[] {
  const existing = new Map(form.endpoints.map((endpoint) => [endpoint.kind, endpoint]));
  const wanted: { kind: "tailnet" | "public"; url: string }[] = [];
  if (form.connectionMode !== "public") {
    wanted.push({ kind: "tailnet", url: tailnetUrl.value.trim() });
  }
  if (form.connectionMode !== "tailnet") {
    wanted.push({ kind: "public", url: publicUrl.value.trim() });
  }
  return wanted
    .filter((entry) => entry.url.length > 0)
    .map((entry) => {
      const previous = existing.get(entry.kind);
      return {
        endpointId: previous?.endpointId ?? crypto.randomUUID(),
        kind: entry.kind,
        url: entry.url,
        expectedOrigin: originOf(entry.url),
        enabled: true,
        httpAckAt: previous?.url === entry.url ? (previous?.httpAckAt ?? null) : null,
        httpAckOrigin: previous?.url === entry.url ? (previous?.httpAckOrigin ?? null) : null,
        httpAckUrl: previous?.url === entry.url ? (previous?.httpAckUrl ?? null) : null,
      };
    });
}

function validateUrl(value: string, label: string): string | null {
  if (!/^https?:\/\/[^\s/]+/i.test(value)) {
    return `${label}需要以 http:// 或 https:// 开头`;
  }
  return null;
}

function submit() {
  if (!form.name.trim()) {
    error.value = "请填写服务名称";
    return;
  }
  const checks: [string, string][] = [];
  if (form.connectionMode !== "public") checks.push(["私网地址", tailnetUrl.value.trim()]);
  if (form.connectionMode !== "tailnet") checks.push(["公网地址", publicUrl.value.trim()]);
  if (form.connectionMode === "failover" && checks.some(([, url]) => !url)) {
    error.value = "自动切换模式需要同时填写私网和公网地址";
    return;
  }
  for (const [label, url] of checks) {
    if (!url) {
      error.value = `请填写${label}`;
      return;
    }
    const message = validateUrl(url, label);
    if (message) {
      error.value = message;
      return;
    }
  }

  emit("save", {
    ...JSON.parse(JSON.stringify(form)),
    name: form.name.trim(),
    category: form.category.trim() || "其他",
    endpoints: buildEndpoints(),
  });
}
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div
        v-if="open"
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-6 backdrop-blur-[2px]"
        @mousedown.self="emit('close')"
      >
        <div class="card w-[540px] max-w-full overflow-hidden shadow-2xl">
          <div class="flex items-center justify-between border-b border-line px-5 py-3.5">
            <div class="text-[13px] font-semibold">{{ title }}</div>
            <button class="icon-btn" @click="emit('close')">
              <X class="size-4" />
            </button>
          </div>

          <div class="max-h-[70vh] space-y-4 overflow-y-auto px-5 py-4">
            <div class="grid grid-cols-2 gap-3">
              <div>
                <label class="label">名称</label>
                <input v-model="form.name" class="field" placeholder="例如 AstrBot" />
              </div>
              <div>
                <label class="label">分类</label>
                <input
                  v-model="form.category"
                  class="field"
                  list="category-list"
                  placeholder="AI / 服务器 / 其他"
                />
                <datalist id="category-list">
                  <option v-for="c in categories" :key="c" :value="c" />
                </datalist>
              </div>
            </div>

            <div>
              <label class="label">连接方式</label>
              <div class="grid grid-cols-3 gap-2">
                <button
                  v-for="mode in modes"
                  :key="mode.value"
                  class="rounded-lg border px-2 py-2 text-left transition-colors"
                  :class="
                    form.connectionMode === mode.value
                      ? 'border-accent/60 bg-accent/10'
                      : 'border-line bg-app/40 hover:bg-panel-2'
                  "
                  @click="form.connectionMode = mode.value"
                >
                  <div class="text-[12px] font-medium">{{ mode.label }}</div>
                  <div class="text-[10.5px] text-muted">{{ mode.hint }}</div>
                </button>
              </div>
            </div>

            <div v-if="form.connectionMode !== 'public'">
              <label class="label">
                {{ form.connectionMode === "failover" ? "私网地址（主入口）" : "私网地址" }}
              </label>
              <input
                v-model="tailnetUrl"
                class="field font-mono"
                placeholder="http://<Tailscale-IP>:端口"
                spellcheck="false"
              />
              <p class="mt-1.5 text-[10.5px] leading-relaxed text-muted">
                通过内置通道访问，不需要公网地址。
              </p>
              <p
                v-if="tailnetLooksPublic"
                class="mt-1 text-[10.5px] leading-relaxed text-amber-400/90"
              >
                这个地址看起来是公网 IP。内置通道只路由 tailnet 网段，该地址可能连不上；
                如果它确实在公网，请改用「公网」模式。
              </p>
            </div>

            <div v-if="form.connectionMode !== 'tailnet'">
              <label class="label">
                {{ form.connectionMode === "failover" ? "公网地址（备用入口）" : "公网地址" }}
              </label>
              <input
                v-model="publicUrl"
                class="field font-mono"
                placeholder="https://panel.example.com"
                spellcheck="false"
              />
              <p class="mt-1.5 text-[10.5px] leading-relaxed text-muted">
                直接用 Windows 网络访问，不依赖 Tailscale。HTTP 地址会以明文传输，建议使用 HTTPS。
              </p>
            </div>

            <div>
              <label class="label">图标</label>
              <div class="grid grid-cols-10 gap-1.5">
                <button
                  v-for="name in ICON_CHOICES"
                  :key="name"
                  class="grid aspect-square place-items-center rounded-md border text-muted transition-colors hover:text-ink"
                  :class="
                    form.icon === name
                      ? 'border-accent/60 bg-accent/15 text-accent'
                      : 'border-line/70 bg-app/40'
                  "
                  :title="name"
                  @click="form.icon = name"
                >
                  <component :is="resolveIcon(name)" class="size-3.5" />
                </button>
              </div>
            </div>

            <div class="flex items-center gap-6 pt-1">
              <label class="flex items-center gap-2 text-[12px]">
                <button
                  class="switch"
                  :data-on="form.enabled"
                  @click="form.enabled = !form.enabled"
                />
                启用此服务
              </label>
              <label class="flex items-center gap-2 text-[12px]">
                <button
                  class="switch"
                  :data-on="form.healthCheck"
                  @click="form.healthCheck = !form.healthCheck"
                />
                参与健康检查
              </label>
            </div>

            <p v-if="error" class="text-[11.5px] text-rose-400">{{ error }}</p>
          </div>

          <div class="flex items-center justify-end gap-2 border-t border-line px-5 py-3">
            <button class="btn" @click="emit('close')">取消</button>
            <button class="btn btn-primary" @click="submit">保存</button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
