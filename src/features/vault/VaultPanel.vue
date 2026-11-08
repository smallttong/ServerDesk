<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import {
  Eye,
  EyeOff,
  KeyRound,
  Lock,
  LockOpen,
  Plus,
  ShieldAlert,
  Trash2,
} from "lucide-vue-next";
import {
  refreshVaultStatus,
  serviceUrl,
  state,
  toast,
  vaultAuthorize,
  vaultDeleteCredential,
  vaultInitialize,
  vaultLock,
  vaultResetAll,
  vaultReveal,
  vaultRevoke,
  vaultSaveCredential,
  vaultUnlock,
} from "../../lib/store";

const password = ref("");
const password2 = ref("");
const busy = ref(false);

const form = reactive({
  editingId: null as string | null,
  serviceId: "",
  endpointId: "",
  username: "",
  secret: "",
  autoLogin: false,
});
const formOpen = ref(false);

const revealId = ref<string | null>(null);
const revealed = ref("");

const confirmPhrase = ref("");
const showReset = ref(false);

const dayOptions = [
  { value: 0, label: "仅本次运行" },
  { value: 1, label: "1 天" },
  { value: 3, label: "3 天" },
  { value: 7, label: "7 天" },
];

const services = computed(() => state.config.services.filter((svc) => svc.enabled));
const endpoints = computed(
  () => services.value.find((svc) => svc.id === form.serviceId)?.endpoints ?? [],
);
const origin = computed(
  () => endpoints.value.find((ep) => ep.endpointId === form.endpointId)?.expectedOrigin ?? "",
);

function serviceName(id: string): string {
  const svc = state.config.services.find((item) => item.id === id);
  return svc ? svc.name || svc.id : id;
}

function fmtDate(ms: number): string {
  if (!ms) return "—";
  return new Date(ms).toLocaleString("zh-CN", { hour12: false });
}

function resetForm() {
  form.editingId = null;
  form.serviceId = services.value[0]?.id ?? "";
  form.endpointId = services.value[0]?.endpoints[0]?.endpointId ?? "";
  form.username = "";
  form.secret = "";
  form.autoLogin = true;
}

watch(
  () => state.vault.unlocked,
  (unlocked) => {
    resetForm();
    formOpen.value = false;
    showReset.value = false;
    confirmPhrase.value = "";
    password.value = "";
    if (!unlocked) revealId.value = null;
  },
  { immediate: true },
);

watch(
  () => form.serviceId,
  () => {
    form.endpointId = endpoints.value[0]?.endpointId ?? "";
  },
);

async function run(action: () => Promise<void>, success?: string) {
  busy.value = true;
  try {
    await action();
    if (success) toast(success, "success");
  } catch (e) {
    toast(String(e), "error");
  } finally {
    busy.value = false;
  }
}

async function doInitialize() {
  if (Array.from(password.value).length < 8) {
    toast("二级密码至少 8 个字符", "error");
    return;
  }
  if (password.value !== password2.value) {
    toast("两次输入的密码不一致", "error");
    return;
  }
  const value = password.value;
  await run(async () => {
    await vaultInitialize(value);
    password.value = "";
    password2.value = "";
  }, "保险箱已创建");
}

async function doUnlock() {
  const value = password.value;
  await run(async () => {
    await vaultUnlock(value);
    password.value = "";
  });
}

const RESET_PHRASE = "重置保险箱";

async function doReset() {
  const typed = confirmPhrase.value.trim();
  if (typed !== RESET_PHRASE && typed.toUpperCase() !== "RESET") {
    toast("请输入「重置保险箱」或 RESET 以确认", "error");
    return;
  }
  await run(async () => {
    await vaultResetAll();
    showReset.value = false;
    confirmPhrase.value = "";
  }, "保险箱已重置，本地凭据与会话已清除");
}

function editCredential(id: string) {
  const record = state.vaultCredentials.find((item) => item.credentialId === id);
  if (!record) return;
  form.editingId = record.credentialId;
  form.serviceId = record.serviceId;
  form.endpointId = record.endpointId;
  form.username = record.username;
  form.secret = "";
  form.autoLogin = record.autoLoginEnabled;
  formOpen.value = true;
}

async function submitCredential() {
  if (!form.serviceId || !form.endpointId) {
    toast("请选择服务与入口", "error");
    return;
  }
  if (!origin.value) {
    toast("该入口没有有效的 Origin", "error");
    return;
  }
  if (!form.username.trim()) {
    toast("请填写账号", "error");
    return;
  }
  if (!form.editingId && !form.secret) {
    toast("请填写密码", "error");
    return;
  }
  await run(async () => {
    await vaultSaveCredential({
      credentialId: form.editingId,
      serviceId: form.serviceId,
      endpointId: form.endpointId,
      origin: origin.value,
      username: form.username.trim(),
      secret: form.secret || null,
      credentialType: "password",
      autoLoginEnabled: form.autoLogin,
    });
    formOpen.value = false;
    resetForm();
  }, "凭据已保存");
}

async function doReveal(id: string) {
  if (revealId.value === id) {
    revealId.value = null;
    revealed.value = "";
    return;
  }
  await run(async () => {
    revealed.value = await vaultReveal(id);
    revealId.value = id;
    setTimeout(() => {
      if (revealId.value === id) {
        revealId.value = null;
        revealed.value = "";
      }
    }, 20000);
  });
}

async function doDelete(id: string) {
  await run(() => vaultDeleteCredential(id), "凭据已删除");
}

async function doAuthorize(id: string, days: number) {
  await run(() => vaultAuthorize(id, days), "已授权");
}

function startAdd() {
  resetForm();
  formOpen.value = true;
}

function onAuthorizeChange(id: string, event: Event) {
  const select = event.target as HTMLSelectElement;
  const raw = select.value;
  select.value = "";
  if (raw === "") return;
  void doAuthorize(id, Number(raw));
}
</script>

<template>
  <section>
    <div class="flex items-start justify-between gap-4">
      <div>
        <h2 class="text-[13px] font-semibold">凭据保险箱</h2>
        <p class="mt-0.5 text-[11.5px] leading-relaxed text-muted">
          凭据加密保存在本机，只有输入二级密码后才能查看；自动登录授权按服务入口单独签发，
          最长 7 天。二级密码不支持修改，忘记只能重置。
        </p>
      </div>
      <span class="shrink-0 rounded-full px-2 py-0.5 text-[10.5px]"
        :class="state.vault.unlocked ? 'bg-emerald-500/15 text-emerald-300' : 'bg-panel-2 text-muted'">
        {{ state.vault.unlocked ? "已解锁" : state.vault.initialized ? "已锁定" : "未初始化" }}
      </span>
    </div>

    <div class="card mt-3 p-4">
      <!-- 未初始化 -->
      <div v-if="!state.vault.initialized" class="space-y-3">
        <div class="flex items-center gap-2 text-[12px]">
          <KeyRound class="size-4 text-accent" />
          设置二级密码（至少 8 个字符，可以用短语）
        </div>
        <div class="grid grid-cols-2 gap-3">
          <input v-model="password" type="password" class="field" placeholder="二级密码" />
          <input v-model="password2" type="password" class="field" placeholder="再次输入" />
        </div>
        <p class="text-[10.5px] text-muted">
          这个密码只用于解锁保险箱，不发送到任何服务器，也无法找回。
        </p>
        <button class="btn btn-primary" :disabled="busy" @click="doInitialize">
          创建保险箱
        </button>
      </div>

      <!-- 已初始化但锁定 -->
      <div v-else-if="!state.vault.unlocked" class="space-y-3">
        <div class="flex items-center gap-2 text-[12px]">
          <Lock class="size-4 text-muted" />
          保险箱已锁定，输入二级密码解锁
        </div>
        <div class="flex gap-2">
          <input
            v-model="password"
            type="password"
            class="field max-w-xs"
            placeholder="二级密码"
            @keydown.enter="doUnlock"
          />
          <button class="btn btn-primary" :disabled="busy" @click="doUnlock">解锁</button>
        </div>
        <div class="pt-2">
          <button
            class="text-[11px] text-rose-400/80 hover:underline"
            @click="showReset = !showReset"
          >
            忘记密码？只能重置保险箱（会清除全部凭据与本地登录状态）
          </button>
        </div>
      </div>

      <!-- 已解锁 -->
      <div v-else class="space-y-4">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2 text-[12px] text-muted">
            <LockOpen class="size-4 text-emerald-400" />
            {{ state.vaultCredentials.length }} 条凭据 ·
            {{ Math.round(state.vault.lockAfterSecs / 60) }} 分钟无操作自动锁定
          </div>
          <div class="flex gap-2">
            <button class="btn h-7 px-2.5" :disabled="busy" @click="run(vaultLock)">
              立即锁定
            </button>
            <button class="btn h-7 px-2.5" @click="startAdd">
              <Plus class="size-3.5" /> 添加凭据
            </button>
          </div>
        </div>

        <div v-if="state.vaultCredentials.length" class="divide-y divide-line/60 rounded-lg border border-line/70">
          <div
            v-for="credential in state.vaultCredentials"
            :key="credential.credentialId"
            class="flex items-center gap-3 px-3 py-2.5"
          >
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2 text-[12px]">
                <span class="font-medium">{{ credential.username }}</span>
                <span class="rounded bg-panel-2 px-1.5 py-0.5 text-[10px] text-muted">
                  {{ serviceName(credential.serviceId) }}
                </span>
                <span
                  v-if="credential.autoLoginEnabled"
                  class="rounded bg-accent/15 px-1.5 py-0.5 text-[10px] text-accent"
                >
                  可自动登录
                </span>
              </div>
              <div class="truncate font-mono text-[10.5px] text-muted">
                {{ revealId === credential.credentialId ? revealed : credential.origin }}
              </div>
            </div>
            <button class="icon-btn" title="显示/隐藏密码" @click="doReveal(credential.credentialId)">
              <EyeOff v-if="revealId === credential.credentialId" class="size-3.5" />
              <Eye v-else class="size-3.5" />
            </button>
            <select
              class="field h-7 w-[7.5rem] text-[11px]"
              @change="onAuthorizeChange(credential.credentialId, $event)"
            >
              <option value="">授权时长…</option>
              <option v-for="option in dayOptions" :key="option.value" :value="option.value">
                {{ option.label }}
              </option>
            </select>
            <button class="btn h-7 px-2 text-[11px]" @click="editCredential(credential.credentialId)">
              编辑
            </button>
            <button
              class="icon-btn text-rose-400/80"
              title="删除"
              @click="doDelete(credential.credentialId)"
            >
              <Trash2 class="size-3.5" />
            </button>
          </div>
        </div>
        <div v-else class="rounded-lg border border-dashed border-line/70 px-3 py-6 text-center text-[11.5px] text-muted">
          还没有保存任何凭据
        </div>

        <!-- 添加/编辑表单 -->
        <div v-if="formOpen" class="rounded-lg border border-line/70 bg-app/40 p-3">
          <div class="grid grid-cols-3 gap-2">
            <div>
              <label class="label">服务</label>
              <select v-model="form.serviceId" class="field">
                <option v-for="svc in services" :key="svc.id" :value="svc.id">
                  {{ svc.name || svc.id }}
                </option>
              </select>
            </div>
            <div>
              <label class="label">入口</label>
              <select v-model="form.endpointId" class="field">
                <option v-for="ep in endpoints" :key="ep.endpointId" :value="ep.endpointId">
                  {{ ep.kind === "tailnet" ? "私网" : "公网" }} · {{ ep.url }}
                </option>
              </select>
            </div>
            <div>
              <label class="label">账号</label>
              <input v-model="form.username" class="field" placeholder="用户名" />
            </div>
          </div>
          <div class="mt-2 grid grid-cols-2 gap-2">
            <div>
              <label class="label">
                密码{{ form.editingId ? "（留空则不修改）" : "" }}
              </label>
              <input v-model="form.secret" type="password" class="field" placeholder="密码" />
            </div>
            <div>
              <label class="label">绑定 Origin</label>
              <input :value="origin" readonly class="field font-mono text-[11px]" />
            </div>
          </div>
          <div class="mt-2 flex items-center justify-between">
            <label class="flex items-center gap-2 text-[11.5px]">
              <button class="switch" :data-on="form.autoLogin" @click="form.autoLogin = !form.autoLogin" />
              允许此凭据用于自动登录
            </label>
            <div class="flex gap-2">
              <button class="btn h-7 px-2.5" @click="formOpen = false">取消</button>
              <button class="btn btn-primary h-7 px-2.5" :disabled="busy" @click="submitCredential">
                保存
              </button>
            </div>
          </div>
        </div>

        <!-- 授权列表 -->
        <div v-if="state.vaultGrants.length">
          <div class="mb-1 text-[11px] font-medium text-muted">自动登录授权</div>
          <div class="divide-y divide-line/60 rounded-lg border border-line/70">
            <div
              v-for="grant in state.vaultGrants"
              :key="grant.grantId"
              class="flex items-center gap-3 px-3 py-2 text-[11px]"
            >
              <span class="rounded bg-panel-2 px-1.5 py-0.5 text-[10px] text-muted">
                {{ grant.storage === "session" ? "本次运行" : "跨重启" }}
              </span>
              <span class="flex-1 truncate">
                {{ serviceName(grant.serviceId) }} · {{ grant.origin }}
              </span>
              <span :class="grant.expired ? 'text-rose-400' : 'text-muted'">
                {{ grant.expired ? "已过期" : "至 " + fmtDate(grant.expiresAt) }}
              </span>
              <button class="btn h-6 px-2 text-[10.5px]" @click="run(() => vaultRevoke(grant.grantId))">
                撤销
              </button>
            </div>
          </div>
        </div>

        <div class="pt-1">
          <button
            class="text-[11px] text-rose-400/80 hover:underline"
            @click="showReset = !showReset"
          >
            重置保险箱（清除全部凭据、授权与本地登录状态）
          </button>
        </div>
      </div>

      <!-- 重置确认 -->
      <div v-if="showReset" class="mt-3 rounded-lg border border-rose-500/30 bg-rose-500/[0.06] p-3">
        <div class="flex items-start gap-2">
          <ShieldAlert class="mt-0.5 size-4 shrink-0 text-rose-400" />
          <p class="text-[11.5px] leading-relaxed text-rose-200/90">
            重置保险箱将永久删除 ServerDesk 保存的全部服务账号密码、自动登录授权和本地登录会话。
            所有内部服务需要重新登录，操作不可撤销。不会影响远程服务端，也不会改服务地址与通道配置。
          </p>
        </div>
        <div class="mt-2 flex items-center gap-2">
          <input
            v-model="confirmPhrase"
            class="field max-w-[16rem]"
            placeholder="输入「重置保险箱」或 RESET"
          />
          <button class="btn btn-danger h-8" :disabled="busy" @click="doReset">确认重置</button>
        </div>
      </div>
    </div>

    <p v-if="state.vault.pendingReset" class="mt-2 text-[11px] text-amber-400">
      上次重置未完成，仍在清理本地登录数据；完成后会自动消失。
    </p>
  </section>
</template>
