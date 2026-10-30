// Colour schemes: token sets in app.css under [data-theme]. "system" sets no
// attribute and follows the OS's light/dark setting.

export type SchemeId = "system" | "light" | "dark" | "studio" | "ivory" | "contrast";

export const SCHEMES: { id: SchemeId; name: string }[] = [
  { id: "system", name: "System" },
  { id: "light", name: "Light" },
  { id: "dark", name: "Dark" },
  { id: "studio", name: "Studio" },
  { id: "ivory", name: "Ivory" },
  { id: "contrast", name: "High contrast" },
];

// Also kept in localStorage, so the scheme is right from the first frame,
// before the settings file has loaded.
const KEY = "s5k-scheme";

export function applyScheme(id: SchemeId | undefined) {
  const root = document.documentElement;
  if (!id || id === "system") delete root.dataset.theme;
  else root.dataset.theme = id;
  try {
    localStorage.setItem(KEY, id ?? "system");
  } catch {
    // Not available: the settings file still has it.
  }
}

/** Applies the last scheme from localStorage, before anything renders. */
export function applyCachedScheme() {
  try {
    const id = localStorage.getItem(KEY) as SchemeId | null;
    if (id && SCHEMES.some((s) => s.id === id)) applyScheme(id);
  } catch {
    // Not available.
  }
}
