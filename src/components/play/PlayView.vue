<script setup lang="ts">
import { t } from "../../i18n";
import { clientIcon, originsServerIcon, platformIcon } from "../../icons";
import { useLauncher } from "../../composables/useLauncher";
import OptionButton from "../ui/OptionButton.vue";

const {
  visiblePlatforms,
  platform,
  updatingLauncher,
  selectPlatform,
  showCtx,
  needsServerChoice,
  visibleOriginsServers,
  originsServer,
  setOriginsServer,
  originsXl,
  toggleOriginsXl,
  platformClients,
  selected,
  selectClient,
  ticketMismatch,
  ticket,
  needsTicket,
  ticketLabel,
  platformSwitchNote,
  activePlatform,
  launchCountdown,
  active,
  statusLabel,
  toggleGearthQuick,
  gearth,
  isAirClient,
  customSwf,
  autoDownloadUpdates,
  toggleAutoDownload,
  pendingUpdate,
  applyLauncherUpdate,
  busy,
  install,
  canPlay,
  play,
  playTarget,
} = useLauncher();
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col">
    <p class="mb-2 text-center text-xs uppercase tracking-widest text-bp-muted">
      {{ t("play.game") }}
    </p>
    <div class="flex w-full gap-2">
      <OptionButton
        v-for="p in visiblePlatforms"
        :key="p.id"
        :active="p.id === platform"
        :disabled="updatingLauncher"
        @click="selectPlatform(p.id)"
        @contextmenu="showCtx($event, 'platform', p.id)"
      >
        <div class="flex items-center justify-between gap-2">
          <span class="font-display text-base leading-tight">{{ p.label }}</span>
          <img
            :src="platformIcon(p.id)"
            class="h-5 w-5 shrink-0 object-contain opacity-80"
            draggable="false"
          >
        </div>
      </OptionButton>
    </div>

    <template v-if="needsServerChoice">
      <p class="mt-3 mb-1.5 text-center text-xs uppercase tracking-widest text-bp-muted">
        {{ t("play.server") }}
      </p>
      <div class="flex w-full gap-2">
        <OptionButton
          v-for="s in visibleOriginsServers"
          :key="s.id"
          :active="s.id === originsServer"
          @click="setOriginsServer(s.id)"
          @contextmenu="showCtx($event, 'server', s.id)"
        >
          <div class="flex items-center justify-between gap-2">
            <span class="font-display text-base leading-none">{{ s.label }}</span>
            <img
              :src="originsServerIcon(s.id)"
              class="h-4 w-4 shrink-0 rounded-[2px] object-cover opacity-90"
              draggable="false"
            >
          </div>
        </OptionButton>
      </div>
      <label
        class="mt-3 flex cursor-pointer items-center justify-center gap-2.5 text-sm text-bp-muted transition-colors hover:text-bp-fg"
      >
        <input
          type="checkbox"
          class="accent-bp-accent size-3.5 rounded border-bp-border"
          :checked="originsXl"
          @change="toggleOriginsXl(($event.target as HTMLInputElement).checked)"
        >
        <span>{{ t("play.widescreen") }}</span>
      </label>
    </template>

    <template v-else>
      <p class="mt-3 mb-1.5 text-center text-xs uppercase tracking-widest text-bp-muted">
        {{ t("play.client") }}
      </p>
      <div class="flex w-full gap-2">
        <OptionButton
          v-for="client in platformClients"
          :key="client.id"
          :active="client.id === selected"
          :disabled="!client.supported || updatingLauncher"
          :dimmed="!client.supported"
          @click="selectClient(client.id)"
          @contextmenu="showCtx($event, 'client', client.id)"
        >
          <div class="flex items-center justify-between gap-2">
            <div class="min-w-0 text-left">
              <span class="font-display block text-base leading-tight">{{ client.label }}</span>
              <span
                class="block text-[10px] uppercase tracking-wide"
                :class="client.ready ? 'text-bp-accent' : 'text-bp-muted/50'"
              >
                {{ client.ready ? t("play.installed") : t("play.notInstalled") }}
              </span>
            </div>
            <img
              :src="clientIcon(client.id)"
              class="h-5 w-5 shrink-0 object-contain opacity-80"
              draggable="false"
              aria-hidden="true"
            >
          </div>
        </OptionButton>
      </div>
    </template>

    <div class="mt-3 w-full space-y-1.5 text-center">
      <p
        class="text-sm"
        :class="
          ticketMismatch
            ? 'text-[#EC0B43]'
            : ticket || !needsTicket
              ? 'text-bp-accent'
              : 'text-bp-muted'
        "
      >
        {{ ticketLabel }}
      </p>
      <p v-if="platformSwitchNote" class="text-xs text-bp-accent">
        {{ t("play.switchedPlatform", platformSwitchNote) }}
      </p>
      <p v-if="ticketMismatch" class="text-xs text-[#EC0B43]">
        {{ t("play.ticketMismatch", { platform: activePlatform?.label ?? "" }) }}
      </p>
      <p
        v-if="launchCountdown != null"
        class="font-display text-2xl tabular-nums text-bp-accent"
      >
        {{ launchCountdown }}
      </p>
      <p v-if="active" class="text-xs text-bp-muted/80">
        {{ statusLabel }}
      </p>
    </div>

    <div class="mt-3 flex justify-center gap-6 rounded-md border border-bp-border p-2 text-center text-[11px]">
      <div
        class="cursor-pointer px-4"
        @click="toggleGearthQuick"
        :title="t('play.toggleGearth')"
      >
        <span class="block uppercase tracking-wider text-bp-muted/70">{{ t("play.gearth") }}</span>
        <span :class="gearth.enabled ? 'text-bp-accent' : 'text-bp-muted'">
          {{ gearth.enabled ? t("play.on") : t("play.off") }}
        </span>
      </div>
      <div v-if="isAirClient && selected === 'classic'" class="flex items-center gap-6">
        <div class="h-6 w-px bg-bp-border" />
        <div class="px-4">
          <span class="block uppercase tracking-wider text-bp-muted/70">{{ t("play.customSwf") }}</span>
          <span :class="customSwf.enabled ? 'text-bp-accent' : 'text-bp-muted'">
            {{ customSwf.enabled ? t("play.on") : t("play.off") }}
          </span>
        </div>
      </div>
    </div>

    <label
      class="mt-3 flex cursor-pointer items-center justify-center gap-2.5 text-sm text-bp-muted transition-colors hover:text-bp-fg"
    >
      <input
        type="checkbox"
        class="accent-bp-accent size-3.5 rounded border-bp-border"
        :checked="autoDownloadUpdates"
        :disabled="updatingLauncher"
        @change="toggleAutoDownload(($event.target as HTMLInputElement).checked)"
      >
      <span>{{ t("play.autoDownload") }}</span>
    </label>

    <div
      v-if="pendingUpdate && !autoDownloadUpdates && !updatingLauncher"
      class="mt-2 text-center"
    >
      <button
        type="button"
        class="text-xs text-bp-accent underline-offset-2 hover:underline"
        @click="applyLauncherUpdate(pendingUpdate)"
      >
        {{ t("play.downloadNow", { version: pendingUpdate.version }) }}
      </button>
    </div>

    <div class="mt-auto flex items-center justify-center gap-3 pt-4">
      <button
        type="button"
        class="cursor-pointer rounded-md border border-bp-border px-4 py-2.5 text-sm text-bp-fg transition-colors hover:border-bp-accent hover:text-bp-accent disabled:cursor-not-allowed disabled:opacity-40"
        :disabled="busy || updatingLauncher || !active?.supported"
        @click="install"
      >
        {{ active?.ready ? t("play.update") : t("play.install") }}
      </button>
      <button
        type="button"
        class="cursor-pointer rounded-md bg-bp-accent px-8 py-2.5 text-sm font-medium text-white transition-colors hover:bg-bp-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
        :disabled="!canPlay"
        @click="play"
      >
        {{
          launchCountdown != null
            ? t("play.playNow", { target: playTarget })
            : playTarget
              ? t("play.playTarget", { target: playTarget })
              : t("play.play")
        }}
      </button>
    </div>
  </div>
</template>
