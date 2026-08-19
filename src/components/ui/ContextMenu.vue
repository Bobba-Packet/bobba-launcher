<script setup lang="ts">
import { t } from "../../i18n";
import type { ClientId, OriginsServerId, Platform } from "../../types";
import { useLauncher } from "../../composables/useLauncher";

const {
  ctxMenu,
  closeCtx,
  hideCtxItem,
  hiddenPlatforms,
  hiddenOriginsServers,
  hiddenClients,
  platforms,
  originsServers,
  clients,
  unhidePlatform,
  unhideOriginsServer,
  unhideClient,
} = useLauncher();
</script>

<template>
  <Teleport to="body">
    <div
      v-if="ctxMenu"
      class="fixed z-50 min-w-[140px] rounded-md border border-bp-border bg-bp-surface py-1 shadow-lg"
      :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }"
      @mouseleave="closeCtx"
    >
      <button
        class="w-full px-3 py-1.5 text-left text-xs text-bp-fg hover:bg-bp-accent/20"
        @click="hideCtxItem"
      >
        {{ t("menu.hide") }}
      </button>
      <template v-if="ctxMenu.type === 'platform' && hiddenPlatforms.length">
        <div class="mx-2 my-1 border-t border-bp-border" />
        <p class="px-3 py-0.5 text-[10px] uppercase text-bp-muted">{{ t("menu.hidden") }}</p>
        <button
          v-for="hp in hiddenPlatforms"
          :key="hp"
          class="w-full px-3 py-1.5 text-left text-xs text-bp-muted hover:bg-bp-accent/20 hover:text-bp-fg"
          @click="unhidePlatform(hp as Platform)"
        >
          {{ t("menu.show", { label: platforms.find(p => p.id === hp)?.label ?? hp }) }}
        </button>
      </template>
      <template v-if="ctxMenu.type === 'server' && hiddenOriginsServers.length">
        <div class="mx-2 my-1 border-t border-bp-border" />
        <p class="px-3 py-0.5 text-[10px] uppercase text-bp-muted">{{ t("menu.hidden") }}</p>
        <button
          v-for="hs in hiddenOriginsServers"
          :key="hs"
          class="w-full px-3 py-1.5 text-left text-xs text-bp-muted hover:bg-bp-accent/20 hover:text-bp-fg"
          @click="unhideOriginsServer(hs as OriginsServerId)"
        >
          {{ t("menu.show", { label: originsServers.find(s => s.id === hs)?.label ?? hs }) }}
        </button>
      </template>
      <template v-if="ctxMenu.type === 'client' && hiddenClients.length">
        <div class="mx-2 my-1 border-t border-bp-border" />
        <p class="px-3 py-0.5 text-[10px] uppercase text-bp-muted">{{ t("menu.hidden") }}</p>
        <button
          v-for="hc in hiddenClients"
          :key="hc"
          class="w-full px-3 py-1.5 text-left text-xs text-bp-muted hover:bg-bp-accent/20 hover:text-bp-fg"
          @click="unhideClient(hc as ClientId)"
        >
          {{ t("menu.show", { label: clients.find(c => c.id === hc)?.label ?? hc }) }}
        </button>
      </template>
    </div>
  </Teleport>
</template>
