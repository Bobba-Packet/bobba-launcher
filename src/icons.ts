import type { ClientId, OriginsServerId, Platform } from "./types";

export function platformIcon(id: Platform): string {
  if (id === "habboX") return "/icons/habbox.png";
  if (id === "origins") return "/icons/origin.png";
  return "/icons/air.png";
}

export function clientIcon(id: ClientId): string {
  if (id === "unity") return "/icons/unity.png";
  if (id === "habbox") return "/icons/habbox.png";
  if (id === "airPlus") return "/icons/airplus.png";
  if (id === "airBobba") return "/icons/bobba.png";
  return "/icons/air.png";
}

export function originsServerIcon(id: OriginsServerId): string {
  if (id === "es") return "/icons/spain.png";
  if (id === "br") return "/icons/brazil.png";
  return "/icons/us_flag.png";
}
