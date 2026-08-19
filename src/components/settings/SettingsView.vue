<script setup lang="ts">
import { LOCALE_OPTIONS, locale, setLocale, t } from "../../i18n";
import { useLauncher } from "../../composables/useLauncher";
import OptionButton from "../ui/OptionButton.vue";

const {
  minimizeToTray,
  toggleMinimizeToTray,
  machineIdIsolation,
  setMachineId,
  autoLaunchDelay,
  setAutoDelay,
  gearth,
  onGearthCheckbox,
  saveGearth,
  pickGearthPath,
  selected,
  customSwf,
  saveCustomSwf,
  downloadCustomSwf,
  hiddenPlatforms,
  hiddenOriginsServers,
  resetHidden,
  clients,
  skipUpdateClients,
  toggleSkipUpdate,
} = useLauncher();
</script>

<template>
  <section class="rounded-md border border-bp-border p-3">
    <p class="mb-2 text-sm text-bp-fg">{{ t("settings.language") }}</p>
    <div class="flex w-full gap-2">
      <OptionButton
        v-for="opt in LOCALE_OPTIONS"
        :key="opt.id"
        compact
        :active="locale === opt.id"
        @click="setLocale(opt.id)"
      >
        {{ opt.label }}
      </OptionButton>
    </div>
  </section>

  <section class="mt-3 rounded-md border border-bp-border p-3 space-y-3">
    <label class="flex cursor-pointer items-center gap-2.5 text-sm text-bp-fg">
      <input
        type="checkbox"
        class="accent-bp-accent size-3.5 rounded border-bp-border"
        :checked="minimizeToTray"
        @change="toggleMinimizeToTray(($event.target as HTMLInputElement).checked)"
      >
      <span>{{ t("settings.tray") }}</span>
    </label>

    <label class="flex cursor-pointer items-center gap-2.5 text-sm text-bp-fg">
      <input
        type="checkbox"
        class="accent-bp-accent size-3.5 rounded border-bp-border"
        :checked="machineIdIsolation"
        @change="setMachineId(($event.target as HTMLInputElement).checked)"
      >
      <span>{{ t("settings.machineId") }}</span>
    </label>
    <p class="ml-6 -mt-2 text-[11px] text-bp-muted/70">
      {{ t("settings.machineIdHint") }}
    </p>

    <div class="space-y-2">
      <div class="flex items-center justify-between gap-3">
        <span class="text-sm text-bp-fg">{{ t("settings.autoLaunch") }}</span>
        <span
          class="min-w-10 rounded border px-1.5 py-0.5 text-center text-[11px] tabular-nums"
          :class="
            autoLaunchDelay === 0
              ? 'border-bp-border text-bp-muted'
              : 'border-bp-accent/40 text-bp-accent'
          "
        >
          {{ autoLaunchDelay === 0 ? t("settings.off") : t("settings.seconds", { n: autoLaunchDelay }) }}
        </span>
      </div>
      <input
        type="range"
        min="0"
        max="15"
        :value="autoLaunchDelay"
        class="delay-slider"
        :style="{
          '--delay-pct': `${(autoLaunchDelay / 15) * 100}%`,
        }"
        @input="setAutoDelay(Number(($event.target as HTMLInputElement).value))"
      >
      <div class="flex justify-between text-[10px] uppercase tracking-wider text-bp-muted/50">
        <span>{{ t("settings.off") }}</span>
        <span>{{ t("settings.seconds", { n: 15 }) }}</span>
      </div>
    </div>
  </section>

  <section class="mt-3 rounded-md border border-bp-border p-3">
    <label class="flex cursor-pointer items-center gap-2.5 text-sm text-bp-fg">
      <input
        v-model="gearth.enabled"
        type="checkbox"
        class="accent-bp-accent size-3.5 rounded border-bp-border"
        @change="onGearthCheckbox"
      >
      <span>{{ t("settings.gearth") }}</span>
    </label>
    <div v-if="gearth.enabled" class="mt-2 flex gap-2">
      <input
        v-model="gearth.path"
        :placeholder="t('settings.gearthPath')"
        class="min-w-0 flex-1 rounded border border-bp-border bg-bp-surface px-2 py-1.5 text-xs text-bp-fg"
        @change="saveGearth"
      >
      <button
        type="button"
        class="btn-skull rounded border border-bp-border px-3 py-1.5 text-xs text-bp-fg hover:border-bp-accent hover:text-bp-accent"
        @click="pickGearthPath"
      >
        {{ t("settings.browse") }}
      </button>
    </div>
  </section>

  <section v-if="selected === 'classic'" class="mt-3 rounded-md border border-bp-border p-3">
    <label class="flex cursor-pointer items-center gap-2.5 text-sm text-bp-fg">
      <input
        v-model="customSwf.enabled"
        type="checkbox"
        class="accent-bp-accent size-3.5 rounded border-bp-border"
        @change="saveCustomSwf"
      >
      <span>{{ t("settings.customSwf") }}</span>
    </label>
    <p class="mt-1 mb-2 text-[11px] leading-relaxed text-bp-muted/70">
      {{ t("settings.customSwfHint") }}
    </p>
    <div class="flex gap-2">
      <input
        v-model="customSwf.link"
        placeholder="https://…/HabboAir.swf"
        class="min-w-0 flex-1 rounded border border-bp-border bg-bp-surface px-2 py-1.5 text-xs text-bp-fg"
        @change="saveCustomSwf"
      >
      <button
        type="button"
        class="btn-skull rounded border border-bp-border px-3 py-1.5 text-xs text-bp-fg hover:border-bp-accent hover:text-bp-accent"
        @click="downloadCustomSwf"
      >
        {{ t("settings.download") }}
      </button>
    </div>
  </section>

  <section class="mt-3 rounded-md border border-bp-border p-3">
    <div class="flex items-center justify-between gap-3">
      <div>
        <p class="text-sm text-bp-fg">{{ t("settings.hiddenItems") }}</p>
        <p class="text-[11px] text-bp-muted/70">
          {{ t("settings.hiddenHint") }}
        </p>
      </div>
      <button
        type="button"
        class="btn-skull shrink-0 rounded border border-bp-border px-3 py-1.5 text-xs text-bp-fg hover:border-bp-accent hover:text-bp-accent disabled:opacity-40"
        :disabled="!hiddenPlatforms.length && !hiddenOriginsServers.length"
        @click="resetHidden"
      >
        {{ t("settings.resetAll") }}
      </button>
    </div>
    <p
      v-if="hiddenPlatforms.length || hiddenOriginsServers.length"
      class="mt-2 text-[11px] text-bp-muted"
    >
      {{ t("settings.hiddenCount", { games: hiddenPlatforms.length, servers: hiddenOriginsServers.length }) }}
    </p>
  </section>

  <section class="mt-3 rounded-md border border-bp-border p-3">
    <p class="text-sm text-bp-fg">{{ t("settings.autoUpdatePerClient") }}</p>
    <p class="mt-1 mb-2 text-[11px] text-bp-muted/70">
      {{ t("settings.autoUpdateHint") }}
    </p>
    <div class="space-y-1.5">
      <label
        v-for="c in clients"
        :key="c.id"
        class="flex cursor-pointer items-center gap-2.5 text-xs text-bp-fg"
      >
        <input
          type="checkbox"
          class="accent-bp-accent size-3.5 rounded border-bp-border"
          :checked="!skipUpdateClients.includes(c.id)"
          @change="toggleSkipUpdate(c.id)"
        >
        <span :class="skipUpdateClients.includes(c.id) ? 'text-bp-muted line-through' : ''">
          {{ c.label }}
        </span>
        <span v-if="skipUpdateClients.includes(c.id)" class="text-[10px] text-bp-muted">{{ t("settings.pinned") }}</span>
      </label>
    </div>
  </section>
</template>
