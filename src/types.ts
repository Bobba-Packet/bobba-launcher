/** The three Habbo products, each with its own clients and launch rules. */
export type Platform = "habboHotel" | "origins" | "habboX";

export type ClientId =
  | "classic"
  | "airPlus"
  | "airBobba"
  | "unity"
  | "origins"
  | "habbox";

export type OriginsServerId = "com" | "es" | "br";

export interface Hotel {
  id: string;
  host: string;
  label: string;
}

export interface PlatformInfo {
  id: Platform;
  label: string;
  blurb: string;
  needsTicket: boolean;
  needsServerChoice: boolean;
  clients: ClientId[];
  hotels: Hotel[];
}

export interface OriginsServerInfo {
  id: OriginsServerId;
  label: string;
  host: string;
}

export interface ClientStatus {
  id: ClientId;
  platform: Platform;
  label: string;
  blurb: string;
  supported: boolean;
  ready: boolean;
  version: string | null;
  installPath: string | null;
}

export interface LoginTicket {
  serverId: string;
  ssoTicket: string;
  serverHost: string;
  username: string | null;
  platform: Platform;
}

export interface ProgressEvent {
  stage: string;
  percent: number | null;
  message: string;
}

export interface LauncherUpdate {
  version: string;
  notes: string | null;
  htmlUrl: string;
  downloadUrl: string;
  assetName: string;
}

export interface GEarthSettings {
  enabled: boolean;
  path: string;
  originsPath: string;
}

export interface CustomSwfSettings {
  enabled: boolean;
  link: string;
}
