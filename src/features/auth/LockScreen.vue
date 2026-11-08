<script setup lang="ts">
import { ref } from "vue";
import { KeyRound, Layers, LoaderCircle, ShieldAlert } from "lucide-vue-next";
import {
  authLogin,
  authRecoverWithCode,
  authResetWithRecovery,
  authFactoryReset,
  state,
} from "../../lib/store";

const mode = ref<"login" | "forgot">("login");

const username = ref(state.auth.username ?? "");
const password = ref("");
const code = ref("");
const error = ref("");
const busy = ref(false);
const confirmReset = ref(false);
const confirmFullReset = ref(false);

async function submitLogin() {
  if (busy.value) return;
  if (!username.value.trim() || !password.value) {
    error.value = "请输入管理员账号和密码";
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    await authLogin(username.value.trim(), password.value);
    password.value = "";
  } catch (e) {
    error.value = String(e);
    password.value = "";
  } finally {
    busy.value = false;
  }
}

async function doRecover() {
  if (busy.value) return;
  if (!code.value.trim()) {
    error.value = "请输入恢复码";
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    await authRecoverWithCode(code.value.trim());
    code.value = "";
    confirmReset.value = false;
    mode.value = "login";
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function doReset() {
  if (busy.value) return;
  if (!code.value.trim()) {
    error.value = "请输入恢复码";
    return;
  }
  if (!confirmReset.value) {
    confirmReset.value = true;
    error.value = "";
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    // Disables the panel lock and drops the account; vault and services stay.
    await authResetWithRecovery(code.value.trim());
    code.value = "";
    confirmReset.value = false;
    mode.value = "login";
  } catch (e) {
    error.value = String(e);
    confirmReset.value = false;
  } finally {
    busy.value = false;
  }
}

async function doFullReset() {
  if (busy.value || !state.auth.error) return;
  if (!confirmFullReset.value) {
    confirmFullReset.value = true;
    error.value = "";
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    await authFactoryReset();
    confirmFullReset.value = false;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

function backToLogin() {
  mode.value = "login";
  code.value = "";
  error.value = "";
  confirmReset.value = false;
  confirmFullReset.value = false;
}
</script>

<template>
  <div class="flex h-full w-full items-center justify-center bg-app text-ink">
    <div class="card w-[368px] px-6 py-7 shadow-2xl">
      <div class="flex items-center gap-3">
        <div
          class="grid size-9 place-items-center rounded-lg bg-gradient-to-br from-accent to-indigo-500 shadow-lg shadow-accent/25"
        >
          <Layers class="size-5 text-white" />
        </div>
        <div class="leading-tight">
          <div class="text-[14px] font-semibold">服务器控制中心</div>
          <div class="text-[10px] tracking-wide text-muted">SERVER DESK</div>
        </div>
      </div>

      <!-- 登录 -->
      <template v-if="mode === 'login'">
        <p class="mt-5 text-[11.5px] text-muted">
          面板登录保护已启用，请验证管理员身份后继续。
        </p>

        <div class="mt-4 space-y-3">
          <div>
            <label class="label">管理员账号</label>
            <input
              v-model="username"
              class="field"
              :disabled="busy"
              autocomplete="username"
              @keydown.enter="submitLogin"
            />
          </div>
          <div>
            <label class="label">密码</label>
            <input
              v-model="password"
              type="password"
              class="field"
              :disabled="busy"
              autocomplete="current-password"
              @keydown.enter="submitLogin"
            />
          </div>
        </div>

        <p v-if="error || state.auth.error" class="mt-3 text-[11.5px] text-rose-400">
          {{ error || state.auth.error }}
        </p>

        <button class="btn btn-primary mt-4 h-9 w-full" :disabled="busy" @click="submitLogin">
          <LoaderCircle v-if="busy" class="size-4 animate-spin" />
          登录
        </button>

        <div class="mt-3 text-center">
          <button
            class="text-[11px] text-muted hover:text-accent hover:underline"
            @click="mode = 'forgot'"
          >
            忘记密码？
          </button>
        </div>

        <p class="mt-4 text-[10.5px] leading-relaxed text-muted/80">
          密码只在本机校验，不会发送到任何服务器。连续输错会暂时限制登录。
        </p>
      </template>

      <!-- 忘记密码 -->
      <template v-else>
        <p v-if="state.auth.error" class="mt-4 text-[11px] leading-relaxed text-rose-400">{{ state.auth.error }}</p>
        <div class="mt-5 flex items-center gap-2 text-[12px]">
          <KeyRound class="size-4 text-accent" />
          使用恢复码
        </div>
        <p class="mt-1.5 text-[11px] leading-relaxed text-muted">
          恢复码在启用登录保护时生成并只显示一次。输入后可以选择「恢复」直接登录（保留账号与密码），
          或「重置」关闭登录保护、清空管理员账号，并清除所有服务在本机的登录状态。
        </p>

        <div class="mt-4">
          <label class="label">恢复码</label>
          <input
            v-model="code"
            class="field font-mono tracking-wider"
            placeholder="XXXX-XXXX-XXXX-XXXX"
            spellcheck="false"
            :disabled="busy"
            @keydown.enter="doRecover"
          />
        </div>

        <p v-if="error" class="mt-3 text-[11.5px] text-rose-400">{{ error }}</p>

        <div class="mt-4 grid grid-cols-2 gap-2">
          <button class="btn btn-danger h-9" :disabled="busy" @click="doReset">
            {{ confirmReset ? "再点一次确认" : "重置" }}
          </button>
          <button class="btn btn-primary h-9" :disabled="busy" @click="doRecover">
            <LoaderCircle v-if="busy" class="size-4 animate-spin" />
            恢复
          </button>
        </div>

        <p v-if="confirmReset" class="mt-2 flex items-start gap-1.5 text-[10.5px] leading-relaxed text-rose-300/90">
          <ShieldAlert class="mt-0.5 size-3.5 shrink-0" />
          重置会关闭登录保护、删除管理员账号，并清除所有服务在本机的登录状态（Cookie）。服务配置、Tailscale 身份和凭据保险箱不受影响。
        </p>

        <div v-if="state.auth.error" class="mt-4 border-t border-line pt-4">
          <p class="text-[10.5px] leading-relaxed text-muted">认证文件及备份都无法恢复时，可执行本机重置。服务列表和加密保险箱保留；服务登录会话、免密授权和 Tailscale 节点身份将被清除。</p>
          <button class="btn btn-danger mt-3 h-9 w-full" :disabled="busy" @click="doFullReset">
            {{ confirmFullReset ? "再次点击，清除本机登录状态" : "无法恢复？执行本机重置" }}
          </button>
        </div>

        <div class="mt-3 text-center">
          <button
            class="text-[11px] text-muted hover:text-accent hover:underline"
            @click="backToLogin"
          >
            返回登录
          </button>
        </div>
      </template>
    </div>
  </div>
</template>
