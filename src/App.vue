<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { t } from "./i18n";
import { useLauncher, type View } from "./composables/useLauncher";
import PlayView from "./components/play/PlayView.vue";
import SettingsView from "./components/settings/SettingsView.vue";
import ContextMenu from "./components/ui/ContextMenu.vue";

const { view, launcherVersion, error, closeCtx, boot, shutdown } = useLauncher();

onMounted(() => {
  void boot();
});

onUnmounted(() => {
  shutdown();
});
</script>

<template>
  <div class="app-shell flex h-full flex-col overflow-hidden bg-bp-bg px-6 py-4" @click="closeCtx">
    <header class="relative mb-3 flex w-full shrink-0 items-center justify-center">
      <img
        src="/brand/logo-bobba-launcher-full.svg"
        alt="Bobba Launcher"
        class="h-8 w-auto"
        draggable="false"
      >
      <span
        v-if="launcherVersion"
        class="absolute right-0 text-[11px] uppercase tracking-wider text-bp-muted/70"
      >
        v{{ launcherVersion }}
      </span>
    </header>

    <nav class="mb-3 flex shrink-0 gap-1 rounded-md border border-bp-border p-1">
      <button
        v-for="tab in (['play', 'settings'] as View[])"
        :key="tab"
        type="button"
        class="flex-1 cursor-pointer rounded px-3 py-1.5 text-xs uppercase tracking-wider transition-colors"
        :class="
          view === tab
            ? 'bg-bp-surface text-bp-fg'
            : 'text-bp-muted hover:text-bp-fg'
        "
        @click="view = tab"
      >
        {{ t(`nav.${tab}`) }}
      </button>
    </nav>

    <main
      class="flex min-h-0 w-full flex-1 flex-col"
      :class="view === 'settings' ? 'settings-scroll overflow-y-auto' : 'overflow-hidden'"
    >
      <PlayView v-if="view === 'play'" />
      <SettingsView v-else />

      <p
        v-if="error"
        class="mt-3 shrink-0 break-words text-sm text-[#EC0B43]"
        :title="error"
      >
        {{ error }}
      </p>
    </main>

    <ContextMenu />
  </div>
</template>
