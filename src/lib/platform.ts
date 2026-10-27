export type Platform = 'macos' | 'windows' | 'other';

function detect(): Platform {
  const ua = navigator.userAgent;
  if (/Mac/i.test(ua)) return 'macos';
  if (/Windows/i.test(ua)) return 'windows';
  return 'other';
}

export const platform: Platform = detect();
export const isMac = platform === 'macos';
export const isWindows = platform === 'windows';

/** Tauri çalışma zamanı içinde miyiz? (Tarayıcıda önizleme için false.) */
export const inTauri = '__TAURI_INTERNALS__' in window;

const WIN_KEYS: Record<string, string> = { '⇧': 'Shift', '⌘': 'Ctrl', '⌥': 'Alt', '⌃': 'Ctrl' };

/** ['⇧','⌘','C'] → macOS'ta "⇧⌘C", Windows'ta "Ctrl+Shift+C" (⌘→Ctrl, ⌥→Alt). */
export function formatShortcut(keys: string[]): string {
  if (isMac) return keys.join('');
  const order = ['Ctrl', 'Alt', 'Shift'];
  const mods = keys.filter((k) => k in WIN_KEYS).map((k) => WIN_KEYS[k]);
  const rest = keys.filter((k) => !(k in WIN_KEYS));
  mods.sort((a, b) => order.indexOf(a) - order.indexOf(b));
  return [...mods, ...rest].join('+');
}
