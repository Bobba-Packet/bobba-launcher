export type ClientId = "classic" | "airPlus" | "airBobba";

export interface ClientStatus {
  id: ClientId;
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
}

export interface Hotel {
  id: string;
  host: string;
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
