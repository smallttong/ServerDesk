<script setup lang="ts">
import { computed, ref } from "vue";
import { Lock, ShieldCheck, ShieldOff } from "lucide-vue-next";
import {
  authChangePanelPassword,
  authDisablePanel,
  authEnableExisting,
  authInitialize,
  authLockPanel,
  authSetAutoLock,
  state,
  toast,
} from "../../lib/store";

const username = ref(state.auth.username ?? "");
const password = ref("");
const confirm = ref("");
const currentPassword = ref("");
const disablePassword = ref("");
const busy = ref(false);

const showChange = ref(false);
const issuedCode = ref<string | null>(null);
const showDisable = ref(false);

const lockOptions = [
  { value: 0, label: "关闭自动锁定" },
  { value: 5, label: "5 分钟" },
  { value: 15, label: "15 分钟" },
  { value: 30, label: "30 分钟" },
  { value: 60, label: "60 分钟" },
];

const mode = computed(() => {
  if (state.auth.enabled) return "enabled";
  return state.auth.registered ? "disabled" : "fresh";
});

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

async function submitEnable() {
  if (!username.value.trim()) {
    toast("请填写管理员账号", "error");
    return;
  }
  if (Array.from(password.value).length < 8) {
    toast("密码至少 8 个字符", "error");
    return;
  }
  if (password.value !== confirm.value) {
    toast("两次输入的密码不一致", "error");
    return;
  }
  const name = username.value.trim();
  const pass = password.value;
  await run(async () => {
    issuedCode.value = await authInitialize(name, pass, pass);
    password.value = "";
    confirm.value = "";
  }, "面板登录保护已启用");
}

async function submitReEnable() {
  if (!password.value) {
    toast("请输入管理员密码", "error");
    return;
  }
  const pass = password.value;
  await run(async () => {
    await authEnableExisting(pass);
    password.value = "";
  }, "面板登录保护已重新启用");
}

async function submitChange() {
  await run(async () => {
    await authChangePanelPassword(currentPassword.value, password.value, confirm.value);
    currentPassword.value = "";
    password.value = "";
    confirm.value = "";
    showChange.value = false;
  }, "管理员密码已更新");
}

async function submitDisable() {
  await run(async () => {
    await authDisablePanel(disablePassword.value);
    disablePassword.value = "";
    showDisable.value = false;
  }, "已关闭面板登录保护");
}

async function copyCode() {
  if (!issuedCode.value) return;
  try {
    await navigator.clipboard.writeText(issuedCode.value);
    toast("恢复码已复制", "success");
  } catch {
    toast("复制失败，请手动抄录", "error");
  }
}

async function setAutoLock(value: number) {
  await run(() => authSetAutoLock(value), "自动锁定设置已保存");
}
</script>

<template>
  <section>
    <div class="flex items-start justify-between gap-4">
      <div>
        <h2 class="text-[13px] font-semibold">安全</h2>
        <p class="mt-0.5 text-[11.5px] leading-relaxed text-muted">
          面板登录保护会在启动和锁定时要求验证本机管理员密码。它与凭据保险箱的二级密码相互独立。
        </p>
      </div>
      <span
        class="shrink-0 rounded-full px-2 py-0.5 text-[10.5px]"
        :class="state.auth.enabled ? 'bg-emerald-500/15 text-emerald-300' : 'bg-panel-2 text-muted'"
      >
        {{ state.auth.enabled ? "已启用" : "未启用" }}
      </span>
    </div>

    <div
      v-if="issuedCode"
      class="mt-3 rounded-lg border border-amber-500/30 bg-amber-500/[0.07] px-4 py-3"
    >
      <div class="text-[12px] font-medium text-amber-300">
        请立即保存恢复码（只显示这一次）
      </div>
      <div class="mt-2 flex items-center gap-2">
        <code
          class="flex-1 select-all rounded bg-black/30 px-2 py-1.5 font-mono text-[13px] tracking-wider"
        >{{ issuedCode }}</code>
        <button class="btn h-7 px-2" @click="copyCode">复制</button>
        <button class="btn h-7 px-2" @click="issuedCode = null">我已保存</button>
      </div>
      <p class="mt-1.5 text-[10.5px] leading-relaxed text-muted">
        忘记密码时可在锁屏用「忘记密码」输入它：恢复=直接登录并保留账号；重置=关闭登录保护并清除各服务的登录状态。
        恢复码不会再次显示，丢失后只能重置。
      </p>
    </div>

    <div class="card mt-3 divide-y divide-line/60">
      <!-- 首次启用 -->
      <div v-if="mode === 'fresh'" class="space-y-3 px-4 py-3">
        <div class="flex items-center gap-2 text-[12px]">
          <ShieldCheck class="size-4 text-accent" />
          启用登录保护（创建本机管理员账号）
        </div>
        <div class="grid grid-cols-3 gap-2">
          <input v-model="username" class="field" placeholder="管理员账号" />
          <input v-model="password" type="password" class="field" placeholder="密码（≥8 字符）" />
          <input v-model="confirm" type="password" class="field" placeholder="再次输入" />
        </div>
        <p class="text-[10.5px] text-muted">
          密码以 Argon2id 哈希保存在本机，不写入日志，也无法找回；忘记后只能重装或恢复备份。
        </p>
        <button class="btn btn-primary h-8" :disabled="busy" @click="submitEnable">
          启用登录保护
        </button>
      </div>

      <!-- 曾启用但已关闭 -->
      <div v-else-if="mode === 'disabled'" class="space-y-3 px-4 py-3">
        <div class="flex items-center gap-2 text-[12px]">
          <ShieldOff class="size-4 text-muted" />
          登录保护已关闭，重新启用需要验证原管理员密码
        </div>
        <div class="flex gap-2">
          <input
            v-model="password"
            type="password"
            class="field max-w-xs"
            placeholder="管理员密码"
            @keydown.enter="submitReEnable"
          />
          <button class="btn btn-primary h-8" :disabled="busy" @click="submitReEnable">
            重新启用
          </button>
        </div>
      </div>

      <!-- 已启用 -->
      <template v-else>
        <div class="flex items-center justify-between px-4 py-3">
          <div>
            <div class="text-[12.5px]">当前管理员</div>
            <div class="text-[11px] text-muted">
              {{ state.auth.username || "—" }} · 角色 administrator
            </div>
          </div>
          <button class="btn h-7 px-2.5" :disabled="busy" @click="run(authLockPanel, '已锁定')">
            <Lock class="size-3.5" /> 立即锁定
          </button>
        </div>

        <div class="flex items-center justify-between gap-4 px-4 py-3">
          <div>
            <div class="text-[12.5px]">无操作自动锁定</div>
            <div class="text-[11px] text-muted">
              只有本窗口的操作用来续期，远程页面无法延长解锁时间
            </div>
          </div>
          <select
            class="field w-40"
            :value="state.auth.autoLockMinutes"
            @change="setAutoLock(Number(($event.target as HTMLSelectElement).value))"
          >
            <option v-for="option in lockOptions" :key="option.value" :value="option.value">
              {{ option.label }}
            </option>
          </select>
        </div>

        <div class="px-4 py-3">
          <button class="text-[12px] text-accent hover:underline" @click="showChange = !showChange">
            修改管理员密码
          </button>
          <div v-if="showChange" class="mt-2 grid grid-cols-3 gap-2">
            <input v-model="currentPassword" type="password" class="field" placeholder="当前密码" />
            <input v-model="password" type="password" class="field" placeholder="新密码（≥8 字符）" />
            <input v-model="confirm" type="password" class="field" placeholder="再次输入新密码" />
            <div class="col-span-3 flex justify-end gap-2">
              <button class="btn h-7 px-2.5" @click="showChange = false">取消</button>
              <button class="btn btn-primary h-7 px-2.5" :disabled="busy" @click="submitChange">
                保存
              </button>
            </div>
          </div>
        </div>

        <div class="px-4 py-3">
          <button
            class="text-[11.5px] text-rose-400/80 hover:underline"
            @click="showDisable = !showDisable"
          >
            关闭面板登录保护
          </button>
          <div v-if="showDisable" class="mt-2 space-y-2">
            <p class="text-[11px] leading-relaxed text-rose-200/80">
              关闭后启动将不再要求登录。不会删除保险箱、服务配置或已有 Cookie；
              重新启用时仍需验证当前管理员密码。
            </p>
            <div class="flex gap-2">
              <input
                v-model="disablePassword"
                type="password"
                class="field max-w-xs"
                placeholder="管理员密码"
              />
              <button class="btn btn-danger h-8" :disabled="busy" @click="submitDisable">
                确认关闭
              </button>
            </div>
          </div>
        </div>
      </template>
    </div>
  </section>
</template>
