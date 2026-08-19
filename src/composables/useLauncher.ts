import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { t } from "../i18n";
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
} from "../types";

export type View = "play" | "settings";
export type CtxMenu = { x: number; y: number; type: string; id: string };

const HIDDEN_CLIENTS_KEY = "hiddenClients";
const GEARTH_FILTERS = [{ name: "G-Earth", extensions: ["exe", "jar"] }];

export const view = ref<View>("play");
export const clients = ref<ClientStatus[]>([]);
export const selected = ref<ClientId>("airBobba");
export const platforms = ref<PlatformInfo[]>([]);
export const platform = ref<Platform>("habboHotel");
export const originsServers = ref<OriginsServerInfo[]>([]);
export const originsServer = ref<OriginsServerId>("com");
export const originsXl = ref(false);
export const platformSwitchNote = ref<{ from: string; to: string } | null>(null);
export const hiddenPlatforms = ref<Platform[]>([]);
export const hiddenOriginsServers = ref<OriginsServerId[]>([]);
export const hiddenClients = ref<ClientId[]>([]);
export const skipUpdateClients = ref<ClientId[]>([]);
export const ctxMenu = ref<CtxMenu | null>(null);
export const ticketRaw = ref("");
export const ticket = ref<LoginTicket | null>(null);
export const busy = ref(false);
export const error = ref<string | null>(null);
export const progress = ref<string | null>(null);
export const autoDownloadUpdates = ref(true);
export const minimizeToTray = ref(true);
export const launcherVersion = ref("");
export const pendingUpdate = ref<LauncherUpdate | null>(null);
export const updatingLauncher = ref(false);
export const launchCountdown = ref<number | null>(null);
export const hotels = ref<Hotel[]>([]);
export const gearth = ref<GEarthSettings>({
  enabled: false,
  path: "",
  originsPath: "",
});
export const customSwf = ref<CustomSwfSettings>({ enabled: false, link: "" });
export const machineIdIsolation = ref(true);
export const autoLaunchDelay = ref(5);

let unlisten: UnlistenFn | null = null;
let unlistenUpdate: UnlistenFn | null = null;
let unlistenDeepLink: UnlistenFn | null = null;
let clipboardTimer: ReturnType<typeof setInterval> | null = null;
let countdownTimer: ReturnType<typeof setInterval> | null = null;
let lastClipboard = "";
let lastDeepLinkUrl = "";
let lastDeepLinkAt = 0;

export const active = computed(
  () => clients.value.find((c) => c.id === selected.value) ?? null,
);

export const activePlatform = computed(
  () => platforms.value.find((p) => p.id === platform.value) ?? null,
);

export const platformClients = computed(() => {
  const allowed = activePlatform.value?.clients ?? [];
  return allowed
    .map((id) => clients.value.find((c) => c.id === id))
    .filter((c): c is ClientStatus => !!c && !hiddenClients.value.includes(c.id));
});

export const needsTicket = computed(() => activePlatform.value?.needsTicket ?? true);

export const visiblePlatforms = computed(() =>
  platforms.value.filter((p) => !hiddenPlatforms.value.includes(p.id)),
);

export const visibleOriginsServers = computed(() =>
  originsServers.value.filter((s) => !hiddenOriginsServers.value.includes(s.id)),
);

export const isAirClient = computed(() =>
  (["classic", "airPlus", "airBobba"] as ClientId[]).includes(selected.value),
);

export const needsServerChoice = computed(
  () => activePlatform.value?.needsServerChoice ?? false,
);

export const playTarget = computed(() => {
  if (needsServerChoice.value) {
    const srv = originsServers.value.find((s) => s.id === originsServer.value);
    return `Origins ${srv?.label ?? ""}`.trim();
  }
  return active.value?.label ?? "";
});

export const ticketMismatch = computed(
  () =>
    !!ticket.value &&
    !!activePlatform.value &&
    ticket.value.platform !== platform.value,
);

export const ticketLabel = computed(() => {
  if (!needsTicket.value) return "";
  if (!ticket.value) return t("play.waitingTicket");
  const hotel = ticket.value.serverId.replace(/^hh/i, "").toUpperCase();
  const user = ticket.value.username ? ` · ${ticket.value.username}` : "";
  return t("play.ticketDetected", { hotel, user });
});

export const statusLabel = computed(() => {
  if (!active.value) return "";
  if (launchCountdown.value != null) {
    return t("play.launchingIn", {
      client: active.value.label,
      seconds: launchCountdown.value,
    });
  }
  if (progress.value) return progress.value;
  if (active.value.ready) {
    return active.value.version
      ? t("play.readyVersion", { version: active.value.version })
      : t("play.ready");
  }
  return t("play.notInstalled");
});

export const canPlay = computed(() => {
  if (!active.value?.supported || busy.value || updatingLauncher.value) return false;
  if (!needsTicket.value) return true;
  return !!ticket.value && !ticketMismatch.value;
});

export function showCtx(e: MouseEvent, type: string, id: string) {
  ctxMenu.value = { x: e.clientX, y: e.clientY, type, id };
}

export function closeCtx() {
  ctxMenu.value = null;
}

export function clearLaunchCountdown() {
  if (countdownTimer) {
    clearInterval(countdownTimer);
    countdownTimer = null;
  }
  launchCountdown.value = null;
}

export function startLaunchCountdown() {
  clearLaunchCountdown();
  if (!canPlay.value) return;
  if (autoLaunchDelay.value === 0) return;
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

export async function refresh() {
  clients.value = await invoke<ClientStatus[]>("list_clients");
  platforms.value = await invoke<PlatformInfo[]>("list_platforms");
  originsServers.value = await invoke<OriginsServerInfo[]>("list_origins_servers");
  platform.value = await invoke<Platform>("get_platform");
  originsServer.value = await invoke<OriginsServerId>("get_origins_server");
  originsXl.value = await invoke<boolean>("get_origins_xl");
  hiddenPlatforms.value = await invoke<Platform[]>("get_hidden_platforms");
  hiddenOriginsServers.value = await invoke<OriginsServerId[]>(
    "get_hidden_origins_servers",
  );
  hiddenClients.value = JSON.parse(localStorage.getItem(HIDDEN_CLIENTS_KEY) || "[]");
  skipUpdateClients.value = await invoke<ClientId[]>("get_skip_update_clients");
  selected.value = await invoke<ClientId>("get_selected");
  autoDownloadUpdates.value = await invoke<boolean>("get_auto_download_updates");
  minimizeToTray.value = await invoke<boolean>("get_minimize_to_tray");
  launcherVersion.value = await invoke<string>("get_launcher_version");
}

export async function refreshExtras() {
  hotels.value = await invoke<Hotel[]>("list_hotels");
  gearth.value = await invoke<GEarthSettings>("get_gearth_settings");
  customSwf.value = await invoke<CustomSwfSettings>("get_custom_swf_settings");
  machineIdIsolation.value = await invoke<boolean>("get_machine_id_isolation");
  autoLaunchDelay.value = await invoke<number>("get_auto_launch_delay");
}

export async function toggleMinimizeToTray(enabled: boolean) {
  minimizeToTray.value = enabled;
  await invoke("set_minimize_to_tray", { enabled });
}

export async function setMachineId(enabled: boolean) {
  machineIdIsolation.value = enabled;
  await invoke("set_machine_id_isolation", { enabled });
}

export async function setAutoDelay(seconds: number) {
  autoLaunchDelay.value = seconds;
  await invoke("set_auto_launch_delay", { seconds });
}

export async function selectClient(id: ClientId) {
  const client = clients.value.find((c) => c.id === id);
  if (client && !client.supported) return;
  error.value = null;
  selected.value = id;
  await invoke("set_selected", { id });
}

export async function selectPlatform(id: Platform, manual = true) {
  if (platform.value === id) return;
  error.value = null;
  if (manual) platformSwitchNote.value = null;
  platform.value = id;
  clearLaunchCountdown();
  selected.value = await invoke<ClientId>("set_platform", { platform: id });
}

export async function setOriginsServer(id: OriginsServerId) {
  originsServer.value = id;
  await invoke("set_origins_server", { server: id });
}

export async function toggleOriginsXl(enabled: boolean) {
  originsXl.value = enabled;
  await invoke("set_origins_xl", { enabled });
}

async function pickGearthFile() {
  return openDialog({
    multiple: false,
    directory: false,
    filters: GEARTH_FILTERS,
  });
}

export async function toggleGearthQuick() {
  if (!gearth.value.enabled && !gearth.value.path) {
    const picked = await pickGearthFile();
    if (typeof picked === "string" && picked) {
      gearth.value.path = picked;
      gearth.value.enabled = true;
      await saveGearth();
    }
    return;
  }
  gearth.value.enabled = await invoke<boolean>("toggle_gearth");
}

export async function hidePlatform(id: Platform) {
  if (visiblePlatforms.value.length <= 1) return;
  hiddenPlatforms.value = [...hiddenPlatforms.value, id];
  await invoke("set_hidden_platforms", { hidden: hiddenPlatforms.value });
  if (platform.value === id) {
    const first = visiblePlatforms.value[0];
    if (first) await selectPlatform(first.id);
  }
}

export async function hideOriginsServer(id: OriginsServerId) {
  if (visibleOriginsServers.value.length <= 1) return;
  hiddenOriginsServers.value = [...hiddenOriginsServers.value, id];
  await invoke("set_hidden_origins_servers", {
    hidden: hiddenOriginsServers.value,
  });
}

export async function hideClient(id: ClientId) {
  if (platformClients.value.length <= 1) return;
  hiddenClients.value = [...hiddenClients.value, id];
  localStorage.setItem(HIDDEN_CLIENTS_KEY, JSON.stringify(hiddenClients.value));
  closeCtx();
}

export function unhideClient(id: ClientId) {
  hiddenClients.value = hiddenClients.value.filter((c) => c !== id);
  localStorage.setItem(HIDDEN_CLIENTS_KEY, JSON.stringify(hiddenClients.value));
  closeCtx();
}

export async function resetHidden() {
  hiddenPlatforms.value = [];
  hiddenOriginsServers.value = [];
  hiddenClients.value = [];
  await invoke("set_hidden_platforms", { hidden: [] });
  await invoke("set_hidden_origins_servers", { hidden: [] });
  localStorage.removeItem(HIDDEN_CLIENTS_KEY);
}

export async function unhidePlatform(id: Platform) {
  hiddenPlatforms.value = hiddenPlatforms.value.filter((p) => p !== id);
  await invoke("set_hidden_platforms", { hidden: hiddenPlatforms.value });
  closeCtx();
}

export async function unhideOriginsServer(id: OriginsServerId) {
  hiddenOriginsServers.value = hiddenOriginsServers.value.filter((s) => s !== id);
  await invoke("set_hidden_origins_servers", {
    hidden: hiddenOriginsServers.value,
  });
  closeCtx();
}

export async function toggleSkipUpdate(id: ClientId) {
  if (skipUpdateClients.value.includes(id)) {
    skipUpdateClients.value = skipUpdateClients.value.filter((c) => c !== id);
  } else {
    skipUpdateClients.value = [...skipUpdateClients.value, id];
  }
  await invoke("set_skip_update_clients", { clients: skipUpdateClients.value });
}

export async function toggleAutoDownload(enabled: boolean) {
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

  const detected = ticket.value?.platform;
  if (detected && detected !== platform.value) {
    const from = activePlatform.value?.label ?? platform.value;
    await selectPlatform(detected, false);
    const to = activePlatform.value?.label ?? detected;
    platformSwitchNote.value = { from, to };
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

export async function install() {
  if (!active.value?.supported) return;
  busy.value = true;
  error.value = null;
  progress.value = t("play.startingInstall");
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

export async function play() {
  clearLaunchCountdown();
  if (!active.value?.supported) return;
  if (needsTicket.value && (!ticketRaw.value.trim() || !ticket.value)) {
    error.value = t("play.ticketRequired");
    return;
  }
  if (ticketMismatch.value) {
    error.value = t("play.ticketMismatchError", {
      platform: activePlatform.value?.label ?? "",
    });
    return;
  }
  busy.value = true;
  error.value = null;
  progress.value = t("play.checkingUpdates");
  try {
    await invoke("launch_client", {
      id: selected.value,
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

export async function applyLauncherUpdate(update: LauncherUpdate) {
  updatingLauncher.value = true;
  error.value = null;
  progress.value = t("play.downloadingLauncher", { version: update.version });
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
      progress.value = t("play.launcherAvailable", { version: update.version });
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

export async function pickGearthPath() {
  const picked = await pickGearthFile();
  if (typeof picked === "string") {
    gearth.value.path = picked;
    await saveGearth();
  }
}

export async function saveGearth() {
  await invoke("set_gearth_settings", { value: gearth.value });
}

export async function onGearthCheckbox() {
  if (gearth.value.enabled && !gearth.value.path) {
    const picked = await pickGearthFile();
    if (typeof picked === "string" && picked) {
      gearth.value.path = picked;
    } else {
      gearth.value.enabled = false;
      return;
    }
  }
  await saveGearth();
}

export async function saveCustomSwf() {
  await invoke("set_custom_swf_settings", { value: customSwf.value });
}

export async function downloadCustomSwf() {
  error.value = null;
  progress.value = t("play.downloadingSwf");
  try {
    await saveCustomSwf();
    await invoke("download_custom_swf");
    progress.value = t("play.swfDownloaded");
  } catch (e) {
    error.value = String(e);
    progress.value = null;
  }
}

export function hideCtxItem() {
  if (!ctxMenu.value) return;
  if (ctxMenu.value.type === "platform") {
    void hidePlatform(ctxMenu.value.id as Platform);
  } else if (ctxMenu.value.type === "server") {
    void hideOriginsServer(ctxMenu.value.id as OriginsServerId);
  } else {
    void hideClient(ctxMenu.value.id as ClientId);
  }
}

export async function boot() {
  document.addEventListener("contextmenu", preventBrowserMenu);
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
}

export function shutdown() {
  document.removeEventListener("contextmenu", preventBrowserMenu);
  unlisten?.();
  unlistenUpdate?.();
  unlistenDeepLink?.();
  if (clipboardTimer) clearInterval(clipboardTimer);
  clearLaunchCountdown();
}

function preventBrowserMenu(e: Event) {
  e.preventDefault();
}

export function useLauncher() {
  return {
    view,
    clients,
    selected,
    platforms,
    platform,
    originsServers,
    originsServer,
    originsXl,
    platformSwitchNote,
    hiddenPlatforms,
    hiddenOriginsServers,
    hiddenClients,
    skipUpdateClients,
    ctxMenu,
    ticket,
    busy,
    error,
    autoDownloadUpdates,
    minimizeToTray,
    launcherVersion,
    pendingUpdate,
    updatingLauncher,
    launchCountdown,
    gearth,
    customSwf,
    machineIdIsolation,
    autoLaunchDelay,
    active,
    activePlatform,
    platformClients,
    needsTicket,
    visiblePlatforms,
    visibleOriginsServers,
    isAirClient,
    needsServerChoice,
    playTarget,
    ticketMismatch,
    ticketLabel,
    statusLabel,
    canPlay,
    showCtx,
    closeCtx,
    selectClient,
    selectPlatform,
    setOriginsServer,
    toggleOriginsXl,
    toggleGearthQuick,
    hidePlatform,
    hideOriginsServer,
    hideClient,
    unhideClient,
    resetHidden,
    unhidePlatform,
    unhideOriginsServer,
    toggleSkipUpdate,
    toggleAutoDownload,
    toggleMinimizeToTray,
    setMachineId,
    setAutoDelay,
    install,
    play,
    applyLauncherUpdate,
    pickGearthPath,
    saveGearth,
    onGearthCheckbox,
    saveCustomSwf,
    downloadCustomSwf,
    hideCtxItem,
    boot,
    shutdown,
  };
}
