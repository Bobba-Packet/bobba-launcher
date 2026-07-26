<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import type {
  ClientId,
  ClientStatus,
  LauncherUpdate,
  LoginTicket,
  ProgressEvent,
} from "./types";

const clients = ref<ClientStatus[]>([]);
const selected = ref<ClientId>("airPlus");
const ticketRaw = ref("");
const ticket = ref<LoginTicket | null>(null);
const busy = ref(false);
const error = ref<string | null>(null);
const progress = ref<string | null>(null);
const autoDownloadUpdates = ref(true);
const launcherVersion = ref("");
const pendingUpdate = ref<LauncherUpdate | null>(null);
const updatingLauncher = ref(false);
/** Seconds left before auto-launch; null when idle. */
const launchCountdown = ref<number | null>(null);

const LAUNCH_DELAY_SEC = 5;

let unlisten: UnlistenFn | null = null;
let unlistenUpdate: UnlistenFn | null = null;
let unlistenDeepLink: UnlistenFn | null = null;
let clipboardTimer: ReturnType<typeof setInterval> | null = null;
let countdownTimer: ReturnType<typeof setInterval> | null = null;
let lastClipboard = "";
let lastDeepLinkUrl = "";
let lastDeepLinkAt = 0;

const active = computed(
  () => clients.value.find((c) => c.id === selected.value) ?? null,
);

const ticketLabel = computed(() => {
  if (!ticket.value) {
    return "Waiting for login ticket from clipboard…";
  }
  const hotel = ticket.value.serverId.replace(/^hh/i, "").toUpperCase();
  const user = ticket.value.username ? ` · ${ticket.value.username}` : "";
  return `Login ticket detected [${hotel}]${user}`;
});

const statusLabel = computed(() => {
  if (!active.value) return "";
  if (launchCountdown.value != null) {
    return `Launching ${active.value.label} in ${launchCountdown.value}s — pick a version`;
  }
  if (progress.value) return progress.value;
  if (active.value.ready) {
    return active.value.version
      ? `Ready · ${active.value.version}`
      : "Ready";
  }
  return "Not installed";
});

const canPlay = computed(
  () =>
    !!ticket.value &&
    !!active.value?.supported &&
    !busy.value &&
    !updatingLauncher.value,
);

function clearLaunchCountdown() {
  if (countdownTimer) {
    clearInterval(countdownTimer);
    countdownTimer = null;
  }
  launchCountdown.value = null;
}

function startLaunchCountdown() {
  clearLaunchCountdown();
  if (!ticket.value || !active.value?.supported) return;

  launchCountdown.value = LAUNCH_DELAY_SEC;
  countdownTimer = setInterval(() => {
    const next = (launchCountdown.value ?? 1) - 1;
    if (next <= 0) {
      clearLaunchCountdown();
      void play();
      return;
    }
    launchCountdown.value = next;
  }, 1000);
}

async function refresh() {
  clients.value = await invoke<ClientStatus[]>("list_clients");
  selected.value = await invoke<ClientId>("get_selected");
  autoDownloadUpdates.value = await invoke<boolean>("get_auto_download_updates");
  launcherVersion.value = await invoke<string>("get_launcher_version");
}

async function selectClient(id: ClientId) {
  const client = clients.value.find((c) => c.id === id);
  if (client && !client.supported) return;
  error.value = null;
  selected.value = id;
  await invoke("set_selected", { id });
}

async function toggleAutoDownload(enabled: boolean) {
  autoDownloadUpdates.value = enabled;
  await invoke("set_auto_download_updates", { enabled });
  if (enabled && pendingUpdate.value && !updatingLauncher.value) {
    await applyLauncherUpdate(pendingUpdate.value);
  }
}

async function applyTicketRaw(raw: string, force = false): Promise<boolean> {
  if (!force && raw === lastClipboard) return false;
  lastClipboard = raw;
  ticketRaw.value = raw;

  const parsed = await invoke<LoginTicket | null>("parse_login_ticket", {
    raw,
  });
  ticket.value = parsed;
  if (parsed) {
    error.value = null;
    // Keep hotel host in sync for Classic installs (silent)
    await invoke("set_default_hotel", { host: parsed.serverHost });
    return true;
  }
  return false;
}

async function onNewTicket(raw: string) {
  const ok = await applyTicketRaw(raw, true);
  if (!ok) return;
  await invoke("show_launcher");
  startLaunchCountdown();
}

async function pollClipboard() {
  if (busy.value || updatingLauncher.value) return;
  try {
    const text = (await readText()) ?? "";
    if (!text.trim()) {
      // Keep ticket during countdown; only clear when idle
      if (ticket.value && launchCountdown.value == null) {
        ticket.value = null;
        ticketRaw.value = "";
        lastClipboard = "";
      }
      return;
    }
    if (text === lastClipboard) return;

    const parsed = await invoke<LoginTicket | null>("parse_login_ticket", {
      raw: text,
    });
    if (parsed) {
      await onNewTicket(text);
    } else {
      // Remember non-ticket clipboard so we don't re-parse every poll
      lastClipboard = text;
    }
  } catch {
    // Clipboard can fail briefly while another app locks it
  }
}

async function install() {
  if (!active.value?.supported) return;
  busy.value = true;
  error.value = null;
  progress.value = "Starting install…";
  try {
    const hotelHost = ticket.value?.serverHost
      ?? (await invoke<string>("get_default_hotel"));
    const updated = await invoke<ClientStatus>("install_client", {
      id: selected.value,
      hotelHost,
    });
    clients.value = clients.value.map((c) =>
      c.id === updated.id ? updated : c,
    );
    progress.value = null;
  } catch (e) {
    error.value = String(e);
    progress.value = null;
  } finally {
    busy.value = false;
  }
}

async function play() {
  clearLaunchCountdown();
  if (!active.value?.supported) return;
  if (!ticketRaw.value.trim() || !ticket.value) {
    error.value = "Copy your Habbo login ticket first (habbo:// link).";
    return;
  }
  busy.value = true;
  error.value = null;
  progress.value = "Launching…";
  try {
    await invoke("launch_client", {
      id: selected.value,
      ticketRaw: ticketRaw.value,
    });
    progress.value = null;
  } catch (e) {
    error.value = String(e);
    progress.value = null;
  } finally {
    busy.value = false;
  }
}

async function applyLauncherUpdate(update: LauncherUpdate) {
  updatingLauncher.value = true;
  error.value = null;
  progress.value = `Downloading launcher v${update.version}…`;
  try {
    await invoke("download_launcher_update", { update });
  } catch (e) {
    error.value = String(e);
    progress.value = null;
    updatingLauncher.value = false;
  }
}

async function checkLauncherUpdates() {
  try {
    const update = await invoke<LauncherUpdate | null>("check_launcher_update");
    pendingUpdate.value = update;
    if (!update) return;
    if (autoDownloadUpdates.value) {
      await applyLauncherUpdate(update);
    } else {
      progress.value = `Launcher v${update.version} available — enable auto-update to install`;
    }
  } catch {
    // Offline / rate-limited GitHub — ignore quietly
  }
}

async function handleHabboUrl(raw: string): Promise<boolean> {
  const now = Date.now();
  // single-instance + deep-link can both fire for the same click
  if (raw === lastDeepLinkUrl && now - lastDeepLinkAt < 1500) return false;
  lastDeepLinkUrl = raw;
  lastDeepLinkAt = now;

  await onNewTicket(raw);
  return !!ticket.value;
}

async function handleStartupTicket(): Promise<boolean> {
  const raw = await invoke<string | null>("get_startup_ticket");
  if (!raw) return false;
  return handleHabboUrl(raw);
}

onMounted(async () => {
  try {
    await refresh();
    unlisten = await listen<ProgressEvent>("client-progress", (event) => {
      const pct =
        event.payload.percent != null ? ` ${event.payload.percent}%` : "";
      progress.value = `${event.payload.message}${pct}`;
    });
    unlistenUpdate = await listen<ProgressEvent>(
      "launcher-update-progress",
      (event) => {
        const pct =
          event.payload.percent != null ? ` ${event.payload.percent}%` : "";
        progress.value = `${event.payload.message}${pct}`;
      },
    );
    // Second-instance / deep-link while already running
    unlistenDeepLink = await listen<string>("habbo-deep-link", (event) => {
      void handleHabboUrl(event.payload);
    });
    const launchedFromUrl = await handleStartupTicket();
    if (!launchedFromUrl) await pollClipboard();
    clipboardTimer = setInterval(() => {
      void pollClipboard();
    }, 800);
    void checkLauncherUpdates();
  } catch (e) {
    error.value = String(e);
  }
});

onUnmounted(() => {
  unlisten?.();
  unlistenUpdate?.();
  unlistenDeepLink?.();
  if (clipboardTimer) clearInterval(clipboardTimer);
  clearLaunchCountdown();
});
</script>

<template>
  <div class="flex h-full flex-col items-center overflow-hidden bg-bp-bg px-7 py-6">
    <header class="relative mb-5 flex w-full shrink-0 items-center justify-center">
      <img
        src="/brand/logo-bobba-launcher-full.svg"
        alt="Bobba Launcher"
        class="h-9 w-auto"
        draggable="false"
      >
      <span
        v-if="launcherVersion"
        class="absolute right-0 text-[11px] uppercase tracking-wider text-bp-muted/70"
      >
        v{{ launcherVersion }}
      </span>
    </header>

    <main class="flex min-h-0 w-full flex-1 flex-col items-center justify-center">
      <p class="mb-3 text-xs uppercase tracking-widest text-bp-muted">
        Client version
      </p>

      <div class="flex w-full gap-2">
        <button
          v-for="client in clients"
          :key="client.id"
          type="button"
          class="min-w-0 flex-1 rounded-md border px-3 py-4 text-center transition-colors"
          :class="[
            !client.supported ? 'cursor-not-allowed opacity-35' : '',
            client.id === selected
              ? 'border-bp-accent bg-bp-surface text-bp-fg'
              : 'border-bp-border text-bp-muted hover:border-bp-accent/50 hover:text-bp-fg',
          ]"
          :disabled="!client.supported || updatingLauncher"
          @click="selectClient(client.id)"
        >
          <span class="font-display block text-xl leading-none">{{ client.label }}</span>
          <span
            class="mt-2 block text-[11px] uppercase tracking-wider"
            :class="client.ready ? 'text-bp-accent' : 'text-bp-muted/70'"
          >
            {{ client.ready ? "Installed" : "Not installed" }}
          </span>
        </button>
      </div>

      <div class="mt-5 w-full space-y-2 text-center">
        <p
          class="text-sm"
          :class="ticket ? 'text-bp-accent' : 'text-bp-muted'"
        >
          {{ ticketLabel }}
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
        <p
          v-if="error"
          class="truncate text-sm text-[#EC0B43]"
          :title="error"
        >
          {{ error }}
        </p>
      </div>

      <label
        class="mt-4 flex cursor-pointer items-center gap-2.5 text-sm text-bp-muted transition-colors hover:text-bp-fg"
      >
        <input
          type="checkbox"
          class="accent-bp-accent size-3.5 rounded border-bp-border"
          :checked="autoDownloadUpdates"
          :disabled="updatingLauncher"
          @change="toggleAutoDownload(($event.target as HTMLInputElement).checked)"
        >
        <span>Auto-download launcher updates</span>
      </label>

      <div
        v-if="pendingUpdate && !autoDownloadUpdates && !updatingLauncher"
        class="mt-2"
      >
        <button
          type="button"
          class="text-xs text-bp-accent underline-offset-2 hover:underline"
          @click="applyLauncherUpdate(pendingUpdate)"
        >
          Download v{{ pendingUpdate.version }} now
        </button>
      </div>

      <div class="mt-2 flex items-center justify-center gap-3 pt-6">
        <button
          type="button"
          class="rounded-md border border-bp-border px-4 py-2.5 text-sm text-bp-fg transition-colors hover:border-bp-accent hover:text-bp-accent disabled:opacity-40"
          :disabled="busy || updatingLauncher || !active?.supported"
          @click="install"
        >
          {{ active?.ready ? "Update" : "Install" }}
        </button>
        <button
          type="button"
          class="rounded-md bg-bp-accent px-8 py-2.5 text-sm font-medium text-white transition-colors hover:bg-bp-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
          :disabled="!canPlay"
          @click="play"
        >
          {{
            launchCountdown != null
              ? `Play now · ${active?.label ?? ""}`
              : `Play${active ? ` ${active.label}` : ""}`
          }}
        </button>
      </div>
    </main>
  </div>
</template>
