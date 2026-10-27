import type { TFunction } from '../i18n';

const MIN = 60_000;
const HOUR = 3_600_000;
const DAY = 86_400_000;

/** "şimdi", "3 dk", "1 sa" — pano geçmişi için kısa süre. */
export function shortAgo(t: TFunction, ts: number): string {
  const d = Date.now() - ts;
  if (d < MIN) return t('popover.now');
  if (d < HOUR) return t('popover.minutes', { n: Math.floor(d / MIN) });
  return t('popover.hours', { n: Math.floor(d / HOUR) });
}

/** "Son görülme" için uzun süre: az önce / 5 dakika önce / 2 saat önce / 3 gün önce. */
export function seenAgo(t: TFunction, ts: number | null): string {
  if (ts === null) return t('device.longAgo');
  const d = Date.now() - ts;
  if (d < MIN) return t('device.justNow');
  if (d < HOUR) return t('device.minutesAgo', { n: Math.floor(d / MIN) });
  if (d < DAY) return t('device.hoursAgo', { n: Math.floor(d / HOUR) });
  return t('device.daysAgo', { n: Math.floor(d / DAY) });
}

/** Aktarım zamanı: bugün saat (14:32), dün "Dün", daha eskisi tarih. */
export function whenLabel(t: TFunction, ts: number, locale: string): string {
  const d = new Date(ts);
  const now = new Date();
  const startToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  if (Date.now() - ts < MIN) return t('transfers.now');
  if (ts >= startToday) return d.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' });
  if (ts >= startToday - DAY) return t('transfers.yesterday');
  return d.toLocaleDateString(locale, { day: 'numeric', month: 'short' });
}

/** "/Users/ad/İndirilenler/Tatar" → "İndirilenler › Tatar" */
export function prettyPath(p: string): string {
  const parts = p.split(/[\\/]+/).filter(Boolean);
  return parts.slice(-2).join(' › ') || p;
}

/** "7F3A91C2" → ["7F3A", "91C2"] */
export function codeGroups(key: string): string[] {
  if (key.length < 2) return [key];
  const h = Math.ceil(key.length / 2);
  return [key.slice(0, h), key.slice(h)];
}

const SPECIAL_KEYS: Record<string, string> = {
  Space: 'Space',
  Enter: 'Enter',
  Tab: 'Tab',
  Backspace: 'Backspace',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  Minus: '-',
  Equal: '=',
  Comma: ',',
  Period: '.',
  Slash: '/',
  Semicolon: ';',
  Quote: "'",
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Backquote: '`',
};

/**
 * Klavye olayından kısayol dizisi üretir: ['⇧','⌘','C'] (sıra ⌃ ⌥ ⇧ ⌘ + tuş).
 * macOS'ta ⌘=Cmd; Windows'ta ⌘=Ctrl. Geçersizse (değiştirici yok, yalnız değiştirici) null.
 */
export function keysFromEvent(e: KeyboardEvent, mac: boolean): string[] | null {
  const code = e.code;
  let key: string | undefined;
  if (code.startsWith('Key')) key = code.slice(3);
  else if (code.startsWith('Digit')) key = code.slice(5);
  else if (/^F\d{1,2}$/.test(code)) key = code;
  else key = SPECIAL_KEYS[code];
  if (!key) return null;
  const mods: string[] = [];
  if (mac) {
    if (e.ctrlKey) mods.push('⌃');
    if (e.altKey) mods.push('⌥');
    if (e.shiftKey) mods.push('⇧');
    if (e.metaKey) mods.push('⌘');
  } else {
    if (e.metaKey) return null;
    if (e.altKey) mods.push('⌥');
    if (e.shiftKey) mods.push('⇧');
    if (e.ctrlKey) mods.push('⌘');
  }
  if (mods.length === 0) return null;
  return [...mods, key];
}
