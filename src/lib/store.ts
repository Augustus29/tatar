import { useSyncExternalStore } from 'react';
import {
  api,
  DEFAULT_SETTINGS,
  subscribe as onApiEvent,
  type ClipItem,
  type Device,
  type PairingState,
  type Settings,
  type SettingsPatch,
  type ShortcutId,
  type Transfer,
} from './api';

/**
 * Uygulama durumu: tek bir küçük depo (useSyncExternalStore). Arayüz yalnızca buradan okur;
 * veriyi `api.ts` cephesi (Rust veya tarayıcıda sahte arka uç) sağlar.
 */
export interface AppState {
  /** İlk yükleme bitti mi (ilk açılış akışı bundan önce gösterilmez) */
  ready: boolean;
  devices: Device[];
  /** devices'tan türetilir (referans kararlı): eşleşmiş cihazlar */
  paired: Device[];
  /** eşleşmemiş ama ağda görünen cihazlar */
  nearby: Device[];
  transfers: Transfer[];
  clipboard: ClipItem[];
  settings: Settings;
  /** Açık eşleştirme penceresi; null = kapalı. state 'paired' = başarı ekranı. */
  pairing: PairingState | null;
}

/** Kısa süreli ileti (toast). `key` i18n anahtarıdır. */
export interface Notice {
  key: string;
  params?: Record<string, string | number>;
}

let state: AppState = {
  ready: false,
  devices: [],
  paired: [],
  nearby: [],
  transfers: [],
  clipboard: [],
  settings: DEFAULT_SETTINGS,
  pairing: null,
};

const subs = new Set<() => void>();
const noticeSubs = new Set<(n: Notice) => void>();

function set(patch: Partial<AppState>): void {
  state = { ...state, ...patch };
  if (patch.devices) {
    state.paired = patch.devices.filter((d) => d.paired);
    state.nearby = patch.devices.filter((d) => !d.paired && d.reachable);
  }
  subs.forEach((fn) => fn());
}

function notify(n: Notice): void {
  noticeSubs.forEach((fn) => fn(n));
}

export function getState(): AppState {
  return state;
}

export function subscribeStore(fn: () => void): () => void {
  subs.add(fn);
  return () => subs.delete(fn);
}

export function useStore<T>(selector: (s: AppState) => T): T {
  return useSyncExternalStore(subscribeStore, () => selector(state));
}

/** Toast iletilerine abone ol. */
export function onNotice(fn: (n: Notice) => void): () => void {
  noticeSubs.add(fn);
  return () => noticeSubs.delete(fn);
}

/* ---------- Başlatma ve olaylar ---------- */

function upsertTransfer(list: Transfer[], t: Transfer): Transfer[] {
  return list.some((x) => x.id === t.id) ? list.map((x) => (x.id === t.id ? t : x)) : [t, ...list];
}

async function init(): Promise<void> {
  onApiEvent('devices-changed', (devices) => set({ devices }));
  onApiEvent('pair-request', (p) => set({ pairing: p }));
  onApiEvent('pair-result', (r) => {
    const cur = state.pairing;
    if (r.success) {
      if (cur && cur.deviceId === r.deviceId) set({ pairing: { ...cur, state: 'paired' } });
      return;
    }
    if (cur && cur.deviceId === r.deviceId) set({ pairing: null });
    const device = state.devices.find((d) => d.id === r.deviceId)?.name ?? '';
    if (r.error === 'rejected') notify({ key: 'toast.pairRejected', params: { device } });
    else if (r.error !== 'cancelled') notify({ key: 'toast.pairFailed', params: { device } });
  });
  onApiEvent('transfer-progress', (t) => {
    const prev = state.transfers.find((x) => x.id === t.id);
    set({ transfers: upsertTransfer(state.transfers, t) });
    if (prev && prev.state === 'active' && t.state === 'done' && t.direction === 'send') {
      notify({ key: 'toast.fileSent', params: { name: t.name } });
    }
  });
  onApiEvent('transfers-changed', (transfers) => set({ transfers }));
  onApiEvent('clipboard-history-changed', (clipboard) => set({ clipboard }));
  onApiEvent('settings-changed', (settings) => set({ settings }));

  const [devices, transfers, clipboard, settings] = await Promise.all([
    api.listDevices(),
    api.listTransfers(),
    api.getClipboardHistory(),
    api.getSettings(),
  ]);
  set({ ready: true, devices, transfers, clipboard, settings: settings ?? state.settings });
}

void init();

/* ---------- Seçiciler ---------- */

export const selectPairedDevices = (s: AppState): Device[] => s.paired;
export const selectNearbyDevices = (s: AppState): Device[] => s.nearby;

/* ---------- Eylemler ---------- */

export const actions = {
  async requestPair(deviceId: string): Promise<void> {
    set({ pairing: { deviceId, state: 'requestedByUs', verificationKey: '' } });
    const p = await api.requestPair(deviceId);
    if (!p) {
      set({ pairing: null });
      notify({ key: 'toast.pairFailed', params: { device: state.devices.find((d) => d.id === deviceId)?.name ?? '' } });
      return;
    }
    // Yanıt geldiğinde pencere hâlâ bu cihaz için açıksa ve başarı ekranına geçmediyse kodu işle.
    if (state.pairing?.deviceId === deviceId && state.pairing.state !== 'paired') set({ pairing: p });
  },
  /** Gelen isteği kabul et. */
  async acceptPair(): Promise<void> {
    const cur = state.pairing;
    if (cur) await api.acceptPair(cur.deviceId);
  },
  /** Gelen isteği reddet ya da giden isteği iptal et; pencereyi kapatır. */
  async rejectPair(): Promise<void> {
    const cur = state.pairing;
    set({ pairing: null });
    if (cur) await api.rejectPair(cur.deviceId);
  },
  dismissPairing(): void {
    set({ pairing: null });
  },
  async unpair(deviceId: string): Promise<void> {
    await api.unpair(deviceId);
  },

  async ringDevice(deviceId: string): Promise<void> {
    await api.ringDevice(deviceId);
  },
  async sendClipboard(deviceId: string, text?: string): Promise<void> {
    await api.sendClipboard(deviceId, text);
  },

  async sendFiles(deviceId: string, paths: string[]): Promise<void> {
    if (paths.length > 0) await api.sendFiles(deviceId, paths);
  },
  async pickAndSend(deviceId: string): Promise<void> {
    await actions.sendFiles(deviceId, await api.pickFiles());
  },
  async cancelTransfer(id: string): Promise<void> {
    set({ transfers: state.transfers.filter((x) => x.id !== id) });
    await api.cancelTransfer(id);
  },
  async retryTransfer(id: string): Promise<void> {
    await api.retryTransfer(id);
  },
  /** Bitmiş, başarısız ve iptal edilmiş aktarımları listeden kaldırır. */
  async clearFinished(deviceId: string): Promise<void> {
    const gone = state.transfers.filter((x) => x.deviceId === deviceId && x.state !== 'active');
    set({ transfers: state.transfers.filter((x) => !gone.includes(x)) });
    await Promise.all(gone.map((x) => api.cancelTransfer(x.id)));
  },
  revealFile: (id: string) => api.revealFile(id),
  openFile: (id: string) => api.openFile(id),

  async clearClipboard(): Promise<void> {
    set({ clipboard: [] });
    await api.clearClipboardHistory();
  },

  /** Ayarı hemen uygular (iyimser güncelleme), sonra Rust'a bildirir. */
  async updateSettings(patch: SettingsPatch): Promise<void> {
    set({ settings: { ...state.settings, ...patch } });
    const next = await api.setSettings(patch);
    if (next) set({ settings: next });
  },
  /** Kısayolu kaydeder; kaydedilemezse eski değere döner ve false verir. */
  async setShortcut(id: ShortcutId, keys: string[]): Promise<boolean> {
    const before = state.settings.shortcuts[id];
    set({ settings: { ...state.settings, shortcuts: { ...state.settings.shortcuts, [id]: keys } } });
    const ok = await api.setShortcut(id, keys);
    if (!ok) set({ settings: { ...state.settings, shortcuts: { ...state.settings.shortcuts, [id]: before } } });
    return ok;
  },
};
