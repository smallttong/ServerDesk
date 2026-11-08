<script setup lang="ts">
import { computed } from "vue";
import type { Status } from "../lib/store";

const props = defineProps<{ status: Status; showLabel?: boolean }>();

const tone = computed(
  () =>
    ({
      online: "bg-emerald-400 shadow-[0_0_6px_rgba(52,211,153,0.65)]",
      offline: "bg-rose-400",
      unknown: "bg-slate-500",
      disabled: "bg-slate-600",
    })[props.status],
);

const label = computed(
  () =>
    ({ online: "在线", offline: "离线", unknown: "未知", disabled: "已停用" })[
      props.status
    ],
);
</script>

<template>
  <span class="inline-flex items-center gap-1.5">
    <span class="size-1.5 shrink-0 rounded-full" :class="tone" />
    <span v-if="showLabel" class="text-[11px] text-muted">{{ label }}</span>
  </span>
</template>
