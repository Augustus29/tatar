/**
 * Sahte arka uç: yalnızca Tauri dışında (tarayıcı önizlemesi) kullanılır.
 * Gerçek veriler Rust'tan gelir; sözleşme için api.ts'ye bakın.
 *
 * Önizleme için adres parametreleri:
 *  ?empty=1       hiç cihaz yok          ?lost=1      tablet bağlantısı yeni koptu
 *  ?failed=1      başarısız aktarım var  ?incoming=1  1,5 sn sonra gelen eşleştirme isteği
 *  ?onboarding=1  ilk açılış akışı
 */
import {
  DEFAULT_SETTINGS,
  DEFAULT_SHORTCUTS,
  type ClipItem,
  type Device,
  type EventMap,
  type PairingState,
  type Settings,
  type SettingsPatch,
  type ShortcutId,
  type Transfer,
  type TroubleshootResult,
} from './types';

const qp = new URLSearchParams(window.location.search);
const MIN = 60_000;
const HOUR = 3_600_000;
const t0 = Date.now();

const pickNames = ['Kat_Planı_A1.dwg', 'Render_Salon_04.png', 'Şartname_Rev2.docx', 'Cephe_Detay.pdf'];
const pickBytes = [13_212_057, 8_808_038, 655_360, 3_250_585];

function initialDevices(): Device[] {
  if (qp.get('empty')) return [];
  const lost = !!qp.get('lost');
  return [
    {
      id: 'tab',
      name: 'Galaxy Tab S10 FE',
      type: 'tablet',
      reachable: !lost,
      paired: true,
      battery: { charge: 78, charging: true },
      ip: lost ? null : '192.168.1.101',
      lastSeen: lost ? t0 - 20_000 : t0,
    },
    {
      id: 'phone',
      name: 'Galaxy S23',
      type: 'phone',
      reachable: false,
      paired: true,
      battery: { charge: 41, charging: false },
      ip: null,
      lastSeen: t0 - 2 * HOUR,
    },
    { id: 'pixel', name: 'Pixel 8', type: 'phone', reachable: true, paired: false, battery: null, ip: '192.168.1.115', lastSeen: t0 },
  ];
}

function initialTransfers(): Transfer[] {
  const list: Transfer[] = [
    {
      id: 't2',
      deviceId: 'tab',
      name: 'Proje_Sunum_v3.pdf',
      size: 4_404_019,
      direction: 'send',
      progress: 4_404_019,
      state: 'done',
      when: t0 - HOUR,
      error: null,
    },
    {
      id: 't1',
      deviceId: 'tab',
      name: 'IMG_2041.HEIC',
      size: 2_936_012,
      direction: 'receive',
      progress: 2_936_012,
      state: 'done',
      when: t0 - 26 * HOUR,
      error: null,
    },
  ];
  if (qp.get('failed')) {
    list.unshift({
      id: 't3',
      deviceId: 'tab',
      name: 'Render_Salon_04.png',
      size: 8_808_038,
      direction: 'send',
      progress: Math.round(8_808_038 * 0.62),
      state: 'failed',
      when: t0 - 5 * MIN,
      error: 'connectionLost',
    });
  }
  return list;
}

function initialClips(): ClipItem[] {
  return [
    { id: 'c1', kind: 'link', text: 'https://meet.google.com/abc-defg-hij', time: t0 },
    { id: 'c2', kind: 'text', text: 'Ofis adresi: Kuştepe Mah. … [ADRES]', time: t0 - 3 * MIN },
    { id: 'c3', kind: 'text', text: 'A-A kesiti kot +3.40 revize', time: t0 - 12 * MIN },
    { id: 'c4', kind: 'concealed', text: '', time: t0 - HOUR },
  ];
}

function storedLang(): Settings['language'] {
  try {
    const v = localStorage.getItem('tatar.lang');
    if (v === 'tr' || v === 'en' || v === 'system') return v;
  } catch {
    /* yok say */
  }
  return 'system';
}

let devices = initialDevices();
let transfers = initialTransfers();
let clips = initialClips();
let settings: Settings = {
  ...DEFAULT_SETTINGS,
  language: storedLang(),
  downloadDir: '/Users/kullanici/İndirilenler/Tatar',
  deviceName: 'MacBook Pro',
  shortcuts: { ...DEFAULT_SHORTCUTS },
  notifications: { ...DEFAULT_SETTINGS.notifications },
  launchAtLogin: true,
  onboarded: !qp.get('onboarding'),
};

/* ---------- Olay yolu ---------- */

type Listener = (payload: never) => void;
const listeners = new Map<string, Set<Listener>>();

function emit<K extends keyof EventMap>(event: K, payload: EventMap[K]): void {
  listeners.get(event)?.forEach((fn) => (fn as (p: EventMap[K]) => void)(payload));
}

const timers: Record<string, number> = {};
let seq = 10;

function pairKey(): string {
  return '7F3A91C2';
}

function finishPair(deviceId: string) {
  devices = devices.map((d) => (d.id === deviceId ? { ...d, paired: true, battery: d.battery ?? { charge: 64, charging: false } } : d));
  emit('devices-changed', devices);
  emit('pair-result', { deviceId, success: true, error: null });
}

function tickTransfer(id: string) {
  window.clearInterval(timers[id]);
  timers[id] = window.setInterval(() => {
    const cur = transfers.find((x) => x.id === id);
    if (!cur || cur.state !== 'active') {
      window.clearInterval(timers[id]);
      return;
    }
    const next = Math.min(cur.size, cur.progress + cur.size * (0.03 + Math.random() * 0.07));
    const upd: Transfer = { ...cur, progress: Math.round(next), state: next >= cur.size ? 'done' : 'active' };
    transfers = transfers.map((x) => (x.id === id ? upd : x));
    emit('transfer-progress', upd);
    if (upd.state === 'done') window.clearInterval(timers[id]);
  }, 140);
}

if (qp.get('incoming')) {
  window.setTimeout(() => {
    const p: PairingState = { deviceId: 'pixel', state: 'requestedByPeer', verificationKey: pairKey() };
    emit('pair-request', p);
  }, 1500);
}

export const mock = {
  on<K extends keyof EventMap>(event: K, handler: (payload: EventMap[K]) => void): () => void {
    const set = listeners.get(event) ?? new Set<Listener>();
    set.add(handler as Listener);
    listeners.set(event, set);
    return () => set.delete(handler as Listener);
  },
  listDevices: () => devices,
  requestPair(deviceId: string): PairingState {
    window.clearTimeout(timers.pair);
    timers.pair = window.setTimeout(() => finishPair(deviceId), 3600);
    return { deviceId, state: 'requestedByUs', verificationKey: pairKey() };
  },
  acceptPair(deviceId: string) {
    finishPair(deviceId);
  },
  rejectPair() {
    window.clearTimeout(timers.pair);
  },
  unpair(deviceId: string) {
    devices = devices.flatMap((d) => (d.id !== deviceId ? [d] : d.reachable ? [{ ...d, paired: false }] : []));
    emit('devices-changed', devices);
  },
  pickFiles: () => [pickNames[Math.floor(Math.random() * pickNames.length)]],
  pickFolder: () => '/Users/kullanici/Masaüstü/Gelenler',
  sendFiles(deviceId: string, paths: string[]) {
    for (const p of paths.slice(0, 6)) {
      const k = pickNames.indexOf(p);
      const name = p.split(/[\\/]/).pop() ?? p;
      const tr: Transfer = {
        id: `t${seq++}`,
        deviceId,
        name,
        size: k >= 0 ? pickBytes[k] : 1_500_000 + Math.floor(Math.random() * 6_000_000),
        direction: 'send',
        progress: 0,
        state: 'active',
        when: Date.now(),
        error: null,
      };
      transfers = [tr, ...transfers];
      emit('transfer-progress', tr);
      tickTransfer(tr.id);
    }
  },
  listTransfers: () => transfers,
  cancelTransfer(id: string) {
    const cur = transfers.find((x) => x.id === id);
    if (!cur) return;
    window.clearInterval(timers[id]);
    transfers = transfers.filter((x) => x.id !== id);
    emit('transfers-changed', transfers);
  },
  retryTransfer(id: string) {
    const cur = transfers.find((x) => x.id === id);
    if (!cur) return;
    const upd: Transfer = { ...cur, progress: 0, state: 'active', error: null, when: Date.now() };
    transfers = transfers.map((x) => (x.id === id ? upd : x));
    emit('transfer-progress', upd);
    tickTransfer(id);
  },
  getClipboardHistory: () => clips,
  clearClipboardHistory() {
    clips = [];
    emit('clipboard-history-changed', clips);
  },
  getSettings: () => settings,
  setSettings(patch: SettingsPatch) {
    settings = { ...settings, ...patch };
    emit('settings-changed', settings);
    return settings;
  },
  setShortcut(id: ShortcutId, keys: string[]) {
    settings = { ...settings, shortcuts: { ...settings.shortcuts, [id]: keys } };
    emit('settings-changed', settings);
    return true;
  },
  async runTroubleshoot(): Promise<TroubleshootResult[]> {
    return [
      { id: 'wifi', status: 'ok', info: 'Ev' },
      { id: 'localNetwork', status: 'ok', info: null },
      { id: 'firewall', status: 'warn', info: null },
      { id: 'discovery', status: 'warn', info: null },
      { id: 'phoneApp', status: 'ok', info: null },
    ];
  },
};
