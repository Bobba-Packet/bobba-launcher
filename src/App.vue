<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type {
  ClientId,
  ClientStatus,
  CustomSwfSettings,
  GEarthSettings,
  Hotel,
  LauncherUpdate,
  LoginTicket,
  OriginsServerId,
  OriginsServerInfo,
  Platform,
  PlatformInfo,
  ProgressEvent,
} from "./types";

type View = "play" | "settings";

const view = ref<View>("play");

const clients = ref<ClientStatus[]>([]);
const selected = ref<ClientId>("airPlus");

// --- platforms ---
const platforms = ref<PlatformInfo[]>([]);
const platform = ref<Platform>("habboHotel");
const originsServers = ref<OriginsServerInfo[]>([]);
const originsServer = ref<OriginsServerId>("com");
const originsXl = ref(false);
/** Set when a pasted ticket moved us to a different platform. */
const platformSwitchNote = ref<string | null>(null);
const hiddenPlatforms = ref<Platform[]>([]);
const hiddenOriginsServers = ref<OriginsServerId[]>([]);
const hiddenClients = ref<ClientId[]>([]);
const skipUpdateClients = ref<ClientId[]>([]);

// Context menu state
const ctxMenu = ref<{ x: number; y: number; type: string; id: string } | null>(null);

function showCtx(e: MouseEvent, type: string, id: string) {
  ctxMenu.value = { x: e.clientX, y: e.clientY, type, id };
}

function closeCtx() {
  ctxMenu.value = null;
}
const ticketRaw = ref("");
const ticket = ref<LoginTicket | null>(null);
const busy = ref(false);
const error = ref<string | null>(null);
const progress = ref<string | null>(null);
const autoDownloadUpdates = ref(true);
const minimizeToTray = ref(true);
const launcherVersion = ref("");
const pendingUpdate = ref<LauncherUpdate | null>(null);
const updatingLauncher = ref(false);
/** Seconds left before auto-launch; null when idle. */
const launchCountdown = ref<number | null>(null);

// --- integrations ---
const hotels = ref<Hotel[]>([]);
const gearth = ref<GEarthSettings>({ enabled: false, path: "", originsPath: "" });
const customSwf = ref<CustomSwfSettings>({ enabled: false, link: "" });
const machineIdIsolation = ref(true);
const autoLaunchDelay = ref(5);

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

const activePlatform = computed(
  () => platforms.value.find((p) => p.id === platform.value) ?? null,
);

/** Only the clients belonging to the selected platform, minus hidden ones. */
const platformClients = computed(() => {
  const allowed = activePlatform.value?.clients ?? [];
  return allowed
    .map((id) => clients.value.find((c) => c.id === id))
    .filter((c): c is ClientStatus => !!c && !hiddenClients.value.includes(c.id));
});

const needsTicket = computed(() => activePlatform.value?.needsTicket ?? true);

const visiblePlatforms = computed(() =>
  platforms.value.filter((p) => !hiddenPlatforms.value.includes(p.id)),
);

const visibleOriginsServers = computed(() =>
  originsServers.value.filter((s) => !hiddenOriginsServers.value.includes(s.id)),
);

/** Custom SWF and machine-id isolation only apply to the AIR-based clients. */
const isAirClient = computed(() =>
  (["classic", "airPlus", "airBobba"] as ClientId[]).includes(selected.value),
);

/** What the Play button launches, spelled out. */
const playTarget = computed(() => {
  if (needsServerChoice.value) {
    const srv = originsServers.value.find((s) => s.id === originsServer.value);
    return `Origins ${srv?.label ?? ""}`.trim();
  }
  return active.value?.label ?? "";
});
const needsServerChoice = computed(
  () => activePlatform.value?.needsServerChoice ?? false,
);

/** True when a pasted ticket belongs to a platform other than the active one. */
const ticketMismatch = computed(
  () => !!ticket.value && !!activePlatform.value && ticket.value.platform !== platform.value,
);

const ticketLabel = computed(() => {
  if (!needsTicket.value) {
    return ``;
  }
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
    return active.value.version ? `Ready · ${active.value.version}` : "Ready";
  }
  return "Not installed";
});

const canPlay = computed(() => {
  if (!active.value?.supported || busy.value || updatingLauncher.value) return false;
  // Origins launches with nothing pasted; everything else needs a matching ticket.
  if (!needsTicket.value) return true;
  return !!ticket.value && !ticketMismatch.value;
});

function clearLaunchCountdown() {
  if (countdownTimer) {
    clearInterval(countdownTimer);
    countdownTimer = null;
  }
  launchCountdown.value = null;
}

function startLaunchCountdown() {
  clearLaunchCountdown();
  if (!canPlay.value) return;
  if (autoLaunchDelay.value === 0) return; // disabled
  launchCountdown.value = autoLaunchDelay.value;
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
  platforms.value = await invoke<PlatformInfo[]>("list_platforms");
  originsServers.value = await invoke<OriginsServerInfo[]>("list_origins_servers");
  platform.value = await invoke<Platform>("get_platform");
  originsServer.value = await invoke<OriginsServerId>("get_origins_server");
  originsXl.value = await invoke<boolean>("get_origins_xl");
  hiddenPlatforms.value = await invoke<Platform[]>("get_hidden_platforms");
  hiddenOriginsServers.value = await invoke<OriginsServerId[]>("get_hidden_origins_servers");
  hiddenClients.value = JSON.parse(localStorage.getItem("hiddenClients") || "[]");
  skipUpdateClients.value = await invoke<ClientId[]>("get_skip_update_clients");
  selected.value = await invoke<ClientId>("get_selected");
  autoDownloadUpdates.value = await invoke<boolean>("get_auto_download_updates");
  minimizeToTray.value = await invoke<boolean>("get_minimize_to_tray");
  launcherVersion.value = await invoke<string>("get_launcher_version");
}

async function toggleMinimizeToTray(enabled: boolean) {
  minimizeToTray.value = enabled;
  await invoke("set_minimize_to_tray", { enabled });
}

async function setMachineId(enabled: boolean) {
  machineIdIsolation.value = enabled;
  await invoke("set_machine_id_isolation", { enabled });
}

async function setAutoDelay(seconds: number) {
  autoLaunchDelay.value = seconds;
  await invoke("set_auto_launch_delay", { seconds });
}

async function refreshExtras() {
  hotels.value = await invoke<Hotel[]>("list_hotels");
  gearth.value = await invoke<GEarthSettings>("get_gearth_settings");
  customSwf.value = await invoke<CustomSwfSettings>("get_custom_swf_settings");
  machineIdIsolation.value = await invoke<boolean>("get_machine_id_isolation");
  autoLaunchDelay.value = await invoke<number>("get_auto_launch_delay");
}

async function selectClient(id: ClientId) {
  const client = clients.value.find((c) => c.id === id);
  if (client && !client.supported) return;
  error.value = null;
  selected.value = id;
  await invoke("set_selected", { id });
}

/**
 * Switching platform. The backend moves `selected` to that platform's default
 * client when the current one belongs elsewhere, and returns it, so the two
 * never drift apart.
 */
async function selectPlatform(id: Platform, manual = true) {
  if (platform.value === id) return;
  error.value = null;
  if (manual) platformSwitchNote.value = null;
  platform.value = id;
  clearLaunchCountdown();
  selected.value = await invoke<ClientId>("set_platform", { platform: id });
}

async function setOriginsServer(id: OriginsServerId) {
  originsServer.value = id;
  await invoke("set_origins_server", { server: id });
}

async function toggleOriginsXl(enabled: boolean) {
  originsXl.value = enabled;
  await invoke("set_origins_xl", { enabled });
}

async function toggleGearthQuick() {
  if (!gearth.value.enabled && !gearth.value.path) {
    // Can't enable without a path — prompt them to pick one
    const picked = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "G-Earth", extensions: ["exe", "jar"] }],
    });
    if (typeof picked === "string" && picked) {
      gearth.value.path = picked;
      gearth.value.enabled = true;
      await saveGearth();
    }
    return;
  }
  gearth.value.enabled = await invoke<boolean>("toggle_gearth");
}

async function hidePlatform(id: Platform) {
  if (visiblePlatforms.value.length <= 1) return;
  hiddenPlatforms.value = [...hiddenPlatforms.value, id];
  await invoke("set_hidden_platforms", { hidden: hiddenPlatforms.value });
  if (platform.value === id) {
    const first = visiblePlatforms.value[0];
    if (first) await selectPlatform(first.id);
  }
}

async function hideOriginsServer(id: OriginsServerId) {
  if (visibleOriginsServers.value.length <= 1) return;
  hiddenOriginsServers.value = [...hiddenOriginsServers.value, id];
  await invoke("set_hidden_origins_servers", { hidden: hiddenOriginsServers.value });
}

async function hideClient(id: ClientId) {
  if (platformClients.value.length <= 1) return;
  hiddenClients.value = [...hiddenClients.value, id];
  localStorage.setItem("hiddenClients", JSON.stringify(hiddenClients.value));
  closeCtx();
}

function unhideClient(id: ClientId) {
  hiddenClients.value = hiddenClients.value.filter((c) => c !== id);
  localStorage.setItem("hiddenClients", JSON.stringify(hiddenClients.value));
  closeCtx();
}

async function resetHidden() {
  hiddenPlatforms.value = [];
  hiddenOriginsServers.value = [];
  hiddenClients.value = [];
  await invoke("set_hidden_platforms", { hidden: [] });
  await invoke("set_hidden_origins_servers", { hidden: [] });
  localStorage.removeItem("hiddenClients");
}

async function unhidePlatform(id: Platform) {
  hiddenPlatforms.value = hiddenPlatforms.value.filter((p) => p !== id);
  await invoke("set_hidden_platforms", { hidden: hiddenPlatforms.value });
  closeCtx();
}

async function unhideOriginsServer(id: OriginsServerId) {
  hiddenOriginsServers.value = hiddenOriginsServers.value.filter((s) => s !== id);
  await invoke("set_hidden_origins_servers", { hidden: hiddenOriginsServers.value });
  closeCtx();
}

async function toggleSkipUpdate(id: ClientId) {
  if (skipUpdateClients.value.includes(id)) {
    skipUpdateClients.value = skipUpdateClients.value.filter((c) => c !== id);
  } else {
    skipUpdateClients.value = [...skipUpdateClients.value, id];
  }
  await invoke("set_skip_update_clients", { clients: skipUpdateClients.value });
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

  const parsed = await invoke<LoginTicket | null>("parse_login_ticket", { raw });
  ticket.value = parsed;
  if (parsed) {
    error.value = null;
    await invoke("set_default_hotel", { host: parsed.serverHost });
    return true;
  }
  return false;
}

async function onNewTicket(raw: string) {
  const ok = await applyTicketRaw(raw, true);
  if (!ok) return;
  await invoke("show_launcher");
  view.value = "play";

  // Auto-detection: a Habbo X ticket selects Habbo X, a hotel ticket selects
  // Habbo Hotel. Without this the user could sit on the wrong platform holding a
  // perfectly valid ticket and get a mismatch error instead of a launch.
  const detected = ticket.value?.platform;
  if (detected && detected !== platform.value) {
    const from = activePlatform.value?.label ?? platform.value;
    await selectPlatform(detected, false);
    const to = activePlatform.value?.label ?? detected;
    platformSwitchNote.value = `Ticket is for ${to} — switched from ${from}`;
  } else {
    platformSwitchNote.value = null;
  }

  startLaunchCountdown();
}

async function pollClipboard() {
  if (busy.value || updatingLauncher.value) return;
  try {
    const text = (await readText()) ?? "";
    if (!text.trim()) {
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
    const hotelHost =
      ticket.value?.serverHost ?? (await invoke<string>("get_default_hotel"));
    const updated = await invoke<ClientStatus>("install_client", {
      id: selected.value,
      hotelHost,
    });
    clients.value = clients.value.map((c) => (c.id === updated.id ? updated : c));
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
  if (needsTicket.value && (!ticketRaw.value.trim() || !ticket.value)) {
    error.value = "Copy your Habbo login ticket first (habbo:// link).";
    return;
  }
  if (ticketMismatch.value) {
    error.value = `That ticket is for a different platform. Switch platform or copy a ${activePlatform.value?.label} ticket.`;
    return;
  }
  busy.value = true;
  error.value = null;
  progress.value = "Checking for updates…";
  try {
    await invoke("launch_client", {
      id: selected.value,
      // Origins ignores this; the backend knows it takes no ticket.
      ticketRaw: needsTicket.value ? ticketRaw.value : "",
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

// ============ G-Earth / custom SWF ============

async function pickGearthPath() {
  const picked = await openDialog({
    multiple: false,
    directory: false,
    filters: [{ name: "G-Earth", extensions: ["exe", "jar"] }],
  });
  if (typeof picked === "string") {
    gearth.value.path = picked;
    await saveGearth();
  }
}

async function saveGearth() {
  await invoke("set_gearth_settings", { value: gearth.value });
}

async function onGearthCheckbox() {
  if (gearth.value.enabled && !gearth.value.path) {
    const picked = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "G-Earth", extensions: ["exe", "jar"] }],
    });
    if (typeof picked === "string" && picked) {
      gearth.value.path = picked;
    } else {
      gearth.value.enabled = false;
      return;
    }
  }
  await saveGearth();
}

async function saveCustomSwf() {
  await invoke("set_custom_swf_settings", { value: customSwf.value });
}

async function downloadCustomSwf() {
  error.value = null;
  progress.value = "Downloading custom SWF…";
  try {
    await saveCustomSwf();
    await invoke("download_custom_swf");
    progress.value = "Custom SWF downloaded";
  } catch (e) {
    error.value = String(e);
    progress.value = null;
  }
}

function platformIcon(id: Platform): string {
  if (id === "habboX") return "/icons/habbox.png";
  if (id === "origins") return "/icons/origin.png";
  return "/icons/air.png";
}

function clientIcon(id: ClientId): string {
  if (id === "unity") return "/icons/unity.png";
  if (id === "habbox") return "/icons/habbox.png";
  if (id === "airPlus") return "/icons/airplus.png";
  if (id === "airBobba") return "/icons/bobba.png";
  // Classic uses the standard AIR icon
  return "/icons/air.png";
}

function originsServerIcon(id: OriginsServerId): string {
  if (id === "es") return "/icons/spain.png";
  if (id === "br") return "/icons/brazil.png";
  return "/icons/us_flag.png";
}

onMounted(async () => {
  // Disable the default browser context menu globally — we use our own.
  document.addEventListener("contextmenu", (e) => {
    e.preventDefault();
  });

  try {
    await refresh();
    await refreshExtras();
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
  <div class="flex h-full flex-col overflow-hidden bg-bp-bg px-6 py-5" @click="closeCtx">
    <header class="relative mb-4 flex w-full shrink-0 items-center justify-center">
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

    <!-- View switcher -->
    <nav class="mb-4 flex shrink-0 gap-1 rounded-md border border-bp-border p-1">
      <button
        v-for="tab in (['play', 'settings'] as View[])"
        :key="tab"
        type="button"
        class="flex-1 rounded px-3 py-1.5 text-xs uppercase tracking-wider transition-colors"
        :class="
          view === tab
            ? 'bg-bp-surface text-bp-fg'
            : 'text-bp-muted hover:text-bp-fg'
        "
        @click="view = tab"
      >
        {{ tab }}
      </button>
    </nav>

    <main class="flex min-h-0 w-full flex-1 flex-col overflow-y-auto">
      <!-- ================= PLAY ================= -->
      <template v-if="view === 'play'">
        <!-- Platform selector -->
        <p class="mb-2 text-center text-xs uppercase tracking-widest text-bp-muted">
          Game
        </p>
        <div class="flex w-full gap-2">
          <button
            v-for="p in visiblePlatforms"
            :key="p.id"
            type="button"
            class="min-w-0 flex-1 rounded-md border px-3 py-2.5 transition-colors"
            :class="
              p.id === platform
                ? 'border-bp-accent bg-bp-surface text-bp-fg'
                : 'border-bp-border text-bp-muted hover:border-bp-accent/50 hover:text-bp-fg'
            "
            :disabled="updatingLauncher"
            @click="selectPlatform(p.id)"
            @contextmenu.prevent="showCtx($event, 'platform', p.id)"
          >
            <div class="flex items-center justify-between gap-2">
              <span class="font-display text-sm leading-tight">{{ p.label }}</span>
              <img
                :src="platformIcon(p.id)"
                class="h-5 w-5 shrink-0 object-contain opacity-80"
                draggable="false"
              >
            </div>
          </button>
        </div>

        <!-- Origins: pick a server instead of a client -->
        <template v-if="needsServerChoice">
          <p class="mt-4 mb-2 text-center text-xs uppercase tracking-widest text-bp-muted">
            Server
          </p>
          <div class="flex w-full gap-2">
            <button
              v-for="s in visibleOriginsServers"
              :key="s.id"
              type="button"
              class="min-w-0 flex-1 rounded-md border px-3 py-2.5 transition-colors"
              :class="
                s.id === originsServer
                  ? 'border-bp-accent bg-bp-surface text-bp-fg'
                  : 'border-bp-border text-bp-muted hover:border-bp-accent/50 hover:text-bp-fg'
              "
              @click="setOriginsServer(s.id)"
              @contextmenu.prevent="showCtx($event, 'server', s.id)"
            >
              <div class="flex items-center justify-between gap-2">
                <span class="font-display text-sm leading-none">{{ s.label }}</span>
                <img
                  :src="originsServerIcon(s.id)"
                  class="h-4 w-4 shrink-0 rounded-[2px] object-cover opacity-90"
                  draggable="false"
                >
              </div>
            </button>
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
            <span>Widescreen (XL)</span>
          </label>
        </template>

        <!-- Habbo Hotel / Habbo X: pick a client -->
        <template v-else>
          <p class="mt-4 mb-2 text-center text-xs uppercase tracking-widest text-bp-muted">
            Client
          </p>
          <div class="flex w-full gap-2">
            <button
              v-for="client in platformClients"
              :key="client.id"
              type="button"
              class="min-w-0 flex-1 rounded-md border px-3 py-2.5 transition-colors"
              :class="[
                !client.supported ? 'cursor-not-allowed opacity-35' : '',
                client.id === selected
                  ? 'border-bp-accent bg-bp-surface text-bp-fg'
                  : 'border-bp-border text-bp-muted hover:border-bp-accent/50 hover:text-bp-fg',
              ]"
              :disabled="!client.supported || updatingLauncher"
              @click="selectClient(client.id)"
              @contextmenu.prevent="showCtx($event, 'client', client.id)"
            >
              <div class="flex items-center justify-between gap-2">
                <div class="min-w-0 text-left">
                  <span class="font-display block text-sm leading-tight">{{ client.label }}</span>
                  <span
                    class="block text-[10px] uppercase tracking-wide"
                    :class="client.ready ? 'text-bp-accent' : 'text-bp-muted/50'"
                  >
                    {{ client.ready ? "Installed" : "Not installed" }}
                  </span>
                </div>
                <img
                  :src="clientIcon(client.id)"
                  class="h-5 w-5 shrink-0 object-contain opacity-80"
                  draggable="false"
                  aria-hidden="true"
                >
              </div>
            </button>
          </div>
        </template>

        <div class="mt-5 w-full space-y-2 text-center">
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
            {{ platformSwitchNote }}
          </p>
          <p v-if="ticketMismatch" class="text-xs text-[#EC0B43]">
            This ticket belongs to another game — switch above or copy a
            {{ activePlatform?.label }} ticket.
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

        <!-- Live summary of what a launch will actually do -->
        <!-- Status strip -->
        <div class="mt-4 flex justify-center gap-6 rounded-md border border-bp-border p-2 text-center text-[11px]">
          <div
            class="cursor-pointer px-4"
            @click="toggleGearthQuick"
            title="Click to toggle G-Earth"
          >
            <span class="block uppercase tracking-wider text-bp-muted/70">G-Earth</span>
            <span :class="gearth.enabled ? 'text-bp-accent' : 'text-bp-muted'">
              {{ gearth.enabled ? "On" : "Off" }}
            </span>
          </div>
          <div v-if="isAirClient && selected === 'classic'" class="flex items-center gap-6">
            <div class="h-6 w-px bg-bp-border" />
            <div class="px-4">
              <span class="block uppercase tracking-wider text-bp-muted/70">Custom SWF</span>
              <span :class="customSwf.enabled ? 'text-bp-accent' : 'text-bp-muted'">
                {{ customSwf.enabled ? "On" : "Off" }}
              </span>
            </div>
          </div>
        </div>

        <label
          class="mt-4 flex cursor-pointer items-center justify-center gap-2.5 text-sm text-bp-muted transition-colors hover:text-bp-fg"
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
          class="mt-2 text-center"
        >
          <button
            type="button"
            class="text-xs text-bp-accent underline-offset-2 hover:underline"
            @click="applyLauncherUpdate(pendingUpdate)"
          >
            Download v{{ pendingUpdate.version }} now
          </button>
        </div>

        <div class="mt-auto flex items-center justify-center gap-3 pt-6">
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
                ? `Play now · ${playTarget}`
                : `Play${playTarget ? ` ${playTarget}` : ""}`
            }}
          </button>
        </div>
      </template>

      <!-- ================= SETTINGS ================= -->
      <template v-else>
        <!-- Behaviour -->
        <section class="rounded-md border border-bp-border p-3 space-y-3">
          <label class="flex cursor-pointer items-center gap-2.5 text-sm text-bp-fg">
            <input
              type="checkbox"
              class="accent-bp-accent size-3.5 rounded border-bp-border"
              :checked="minimizeToTray"
              @change="toggleMinimizeToTray(($event.target as HTMLInputElement).checked)"
            >
            <span>Keep running in the system tray when closed</span>
          </label>

          <label class="flex cursor-pointer items-center gap-2.5 text-sm text-bp-fg">
            <input
              type="checkbox"
              class="accent-bp-accent size-3.5 rounded border-bp-border"
              :checked="machineIdIsolation"
              @change="setMachineId(($event.target as HTMLInputElement).checked)"
            >
            <span>Per-account machine ID isolation</span>
          </label>
          <p class="ml-6 -mt-2 text-[11px] text-bp-muted/70">
            Gives each account its own device identity. Disable if you experience launch issues.
          </p>

          <div class="flex items-center gap-3">
            <span class="text-sm text-bp-fg whitespace-nowrap">Auto-launch delay</span>
            <input
              type="range"
              min="0"
              max="15"
              :value="autoLaunchDelay"
              class="flex-1 accent-bp-accent"
              @input="setAutoDelay(Number(($event.target as HTMLInputElement).value))"
            >
            <span class="w-8 text-center text-xs text-bp-muted tabular-nums">
              {{ autoLaunchDelay === 0 ? 'Off' : `${autoLaunchDelay}s` }}
            </span>
          </div>
        </section>

        <!-- G-Earth -->
        <section class="mt-3 rounded-md border border-bp-border p-3">
          <label class="flex cursor-pointer items-center gap-2.5 text-sm text-bp-fg">
            <input
              v-model="gearth.enabled"
              type="checkbox"
              class="accent-bp-accent size-3.5 rounded border-bp-border"
              @change="onGearthCheckbox"
            >
            <span>Launch G-Earth with the client</span>
          </label>
          <div v-if="gearth.enabled" class="mt-2 flex gap-2">
            <input
              v-model="gearth.path"
              placeholder="Path to G-Earth executable"
              class="min-w-0 flex-1 rounded border border-bp-border bg-bp-surface px-2 py-1.5 text-xs text-bp-fg"
              @change="saveGearth"
            >
            <button
              type="button"
              class="rounded border border-bp-border px-3 py-1.5 text-xs text-bp-fg hover:border-bp-accent hover:text-bp-accent"
              @click="pickGearthPath"
            >
              Browse
            </button>
          </div>
        </section>

        <!-- Custom SWF — only relevant for Classic client -->
        <section v-if="selected === 'classic'" class="mt-3 rounded-md border border-bp-border p-3">
          <label class="flex cursor-pointer items-center gap-2.5 text-sm text-bp-fg">
            <input
              v-model="customSwf.enabled"
              type="checkbox"
              class="accent-bp-accent size-3.5 rounded border-bp-border"
              @change="saveCustomSwf"
            >
            <span>Use a custom HabboAir.swf</span>
          </label>
          <p class="mt-1 mb-2 text-[11px] leading-relaxed text-bp-muted/70">
            Only use SWFs you trust — they run code on your machine. Custom
            builds keep the stock AIR identity, so multi-client isolation is
            disabled while this is on.
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
              class="rounded border border-bp-border px-3 py-1.5 text-xs text-bp-fg hover:border-bp-accent hover:text-bp-accent"
              @click="downloadCustomSwf"
            >
              Download
            </button>
          </div>
        </section>

        <!-- Visibility -->
        <section class="mt-3 rounded-md border border-bp-border p-3">
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm text-bp-fg">Hidden items</p>
              <p class="text-[11px] text-bp-muted/70">
                Right-click any game or server to hide it. Reset here to bring them all back.
              </p>
            </div>
            <button
              type="button"
              class="rounded border border-bp-border px-3 py-1.5 text-xs text-bp-fg hover:border-bp-accent hover:text-bp-accent disabled:opacity-40"
              :disabled="!hiddenPlatforms.length && !hiddenOriginsServers.length"
              @click="resetHidden"
            >
              Reset all
            </button>
          </div>
          <p
            v-if="hiddenPlatforms.length || hiddenOriginsServers.length"
            class="mt-2 text-[11px] text-bp-muted"
          >
            {{ hiddenPlatforms.length }} game(s) and {{ hiddenOriginsServers.length }} server(s) hidden
          </p>
        </section>

        <!-- Per-client update control -->
        <section class="mt-3 rounded-md border border-bp-border p-3">
          <p class="text-sm text-bp-fg">Auto-update per client</p>
          <p class="mt-1 mb-2 text-[11px] text-bp-muted/70">
            Disable auto-update for specific clients to keep a pinned version.
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
              <span v-if="skipUpdateClients.includes(c.id)" class="text-[10px] text-bp-muted">(pinned)</span>
            </label>
          </div>
        </section>
      </template>

      <p
        v-if="error"
        class="mt-3 shrink-0 break-words text-sm text-[#EC0B43]"
        :title="error"
      >
        {{ error }}
      </p>
    </main>

    <!-- Context menu -->
    <Teleport to="body">
      <div
        v-if="ctxMenu"
        class="fixed z-50 min-w-[140px] rounded-md border border-bp-border bg-bp-surface py-1 shadow-lg"
        :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }"
        @mouseleave="closeCtx"
      >
        <!-- Hide this item -->
        <button
          class="w-full px-3 py-1.5 text-left text-xs text-bp-fg hover:bg-bp-accent/20"
          @click="ctxMenu.type === 'platform' ? hidePlatform(ctxMenu.id as Platform) : ctxMenu.type === 'server' ? hideOriginsServer(ctxMenu.id as OriginsServerId) : hideClient(ctxMenu.id as ClientId)"
        >
          Hide
        </button>
        <!-- Unhide: show hidden items of the same type -->
        <template v-if="ctxMenu.type === 'platform' && hiddenPlatforms.length">
          <div class="mx-2 my-1 border-t border-bp-border" />
          <p class="px-3 py-0.5 text-[10px] uppercase text-bp-muted">Hidden</p>
          <button
            v-for="hp in hiddenPlatforms"
            :key="hp"
            class="w-full px-3 py-1.5 text-left text-xs text-bp-muted hover:bg-bp-accent/20 hover:text-bp-fg"
            @click="unhidePlatform(hp)"
          >
            Show {{ platforms.find(p => p.id === hp)?.label ?? hp }}
          </button>
        </template>
        <template v-if="ctxMenu.type === 'server' && hiddenOriginsServers.length">
          <div class="mx-2 my-1 border-t border-bp-border" />
          <p class="px-3 py-0.5 text-[10px] uppercase text-bp-muted">Hidden</p>
          <button
            v-for="hs in hiddenOriginsServers"
            :key="hs"
            class="w-full px-3 py-1.5 text-left text-xs text-bp-muted hover:bg-bp-accent/20 hover:text-bp-fg"
            @click="unhideOriginsServer(hs)"
          >
            Show {{ originsServers.find(s => s.id === hs)?.label ?? hs }}
          </button>
        </template>
        <template v-if="ctxMenu.type === 'client' && hiddenClients.length">
          <div class="mx-2 my-1 border-t border-bp-border" />
          <p class="px-3 py-0.5 text-[10px] uppercase text-bp-muted">Hidden</p>
          <button
            v-for="hc in hiddenClients"
            :key="hc"
            class="w-full px-3 py-1.5 text-left text-xs text-bp-muted hover:bg-bp-accent/20 hover:text-bp-fg"
            @click="unhideClient(hc)"
          >
            Show {{ clients.find(c => c.id === hc)?.label ?? hc }}
          </button>
        </template>
      </div>
    </Teleport>
  </div>
</template>>
