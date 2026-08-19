import { ref } from "vue";
import en from "./locales/en.json";
import es from "./locales/es.json";
import pt from "./locales/pt.json";
import fr from "./locales/fr.json";

export const LOCALES = ["en", "es", "pt", "fr"] as const;
export type Locale = (typeof LOCALES)[number];

export const LOCALE_OPTIONS: { id: Locale; label: string }[] = [
  { id: "en", label: "English" },
  { id: "es", label: "Español" },
  { id: "pt", label: "Português" },
  { id: "fr", label: "Français" },
];

const STORAGE_KEY = "bobba-locale";

const catalogs: Record<Locale, Record<string, unknown>> = { en, es, pt, fr };

function isLocale(value: string | null): value is Locale {
  return !!value && (LOCALES as readonly string[]).includes(value);
}

function detectLocale(): Locale {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (isLocale(stored)) return stored;
  } catch {
    // private mode / unavailable storage
  }
  const lang = (navigator.language || "en").slice(0, 2).toLowerCase();
  if (isLocale(lang)) return lang;
  return "en";
}

export const locale = ref<Locale>(detectLocale());

export function setLocale(next: Locale) {
  locale.value = next;
  document.documentElement.lang = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // ignore
  }
}

function lookup(tree: unknown, key: string): string | undefined {
  let cur: unknown = tree;
  for (const part of key.split(".")) {
    if (!cur || typeof cur !== "object" || !(part in cur)) return undefined;
    cur = (cur as Record<string, unknown>)[part];
  }
  return typeof cur === "string" ? cur : undefined;
}

export function t(
  key: string,
  params?: Record<string, string | number>,
): string {
  const raw =
    lookup(catalogs[locale.value], key) ?? lookup(catalogs.en, key) ?? key;
  if (!params) return raw;
  return raw.replace(/\{(\w+)\}/g, (_, name: string) =>
    params[name] == null ? `{${name}}` : String(params[name]),
  );
}

document.documentElement.lang = locale.value;
