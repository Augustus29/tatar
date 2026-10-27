/**
 * Arayüz ↔ Rust sözleşmesi. Rust tarafı bu dosyaya göre yazılır.
 * Tarayıcıda (Tauri dışında) bütün çağrılar `mock.ts` içindeki sahte arka uca düşer.
 *
 * Genel kurallar
 *  - Komut adları snake_case, argüman nesnesinin alanları camelCase (Tauri varsayılanı).
 *  - Zamanlar epoch milisaniyedir. Boyutlar bayttır. "Kimlik" (id) alanları dizedir.
 *  - Bir komut hata verirse `call()` uyarı yazar ve `undefined` döner; arayüz bunu "başarısız" sayar.
 *
 * KOMUTLAR (UI → Rust)                          argümanlar                          dönüş
 *  list_devices                                  –                                   Device[]
 *  request_pair                                  { deviceId }                        PairingState   (state: requestedByUs)
 *  accept_pair                                   { deviceId }                        void           (gelen isteği kabul)
 *  reject_pair                                   { deviceId }                        void           (gelen isteği reddet VEYA giden isteği iptal et)
 *  unpair                                        { deviceId }                        void
 *  ping_device                                   { deviceId }                        void           (kdeconnect.ping)
 *  ring_device                                   { deviceId }                        void           (kdeconnect.findmyphone.request; tekrar çağrı durdurur)
 *  send_clipboard                                { deviceId, text?: string }         void           (text yoksa şu anki sistem panosu)
 *  pick_files                                    –                                   string[]       (yerel dosya seçici; iptalde [])
 *  pick_folder                                   –                                   string | null  (klasör seçici)
 *  send_files                                    { deviceId, paths: string[] }       void           (her dosya için bir Transfer oluşur)
 *  list_transfers                                –                                   Transfer[]
 *  cancel_transfer                               { id }                              void           (aktifse iptal eder; bitmiş/başarısız/iptal ise listeden siler)
 *  retry_transfer                                { id }                              void
 *  reveal_file                                   { id }                              void           (Finder/Explorer'da göster)
 *  open_file                                     { id }                              void
 *  get_clipboard_history                         –                                   ClipItem[]     (en yeni başta, en çok 20)
 *  clear_clipboard_history                       –                                   void
 *  get_settings                                  –                                   Settings
 *  set_settings                                  { patch: SettingsPatch }            Settings       (birleştirilmiş son hal; ayrıca `settings-changed` yayar)
 *  set_shortcut                                  { id: ShortcutId, keys: string[] }  boolean        (false/hata: kaydedilemedi, ör. başka uygulama kullanıyor)
 *                                                  keys: ['⇧','⌘','C'] biçimi. Sıra ⌃ ⌥ ⇧ ⌘ sonra tuş. Windows'ta ⌘ = Ctrl, ⌥ = Alt.
 *  run_troubleshoot                              –                                   TroubleshootResult[] (5 denetim, sabit sırayla)
 *  add_device_by_ip                              { ip }                              void
 *  open_system_settings                          { target: 'firewall' | 'localNetwork' } void
 *  request_local_network                         –                                   boolean        (macOS yerel ağ izni; Windows'ta true)
 *  app_info                                      –                                   { version: string }
 *
 * OLAYLAR (Rust → UI)                            payload
 *  devices-changed                               Device[]                (tam liste)
 *  pair-request                                  PairingState            (state: requestedByPeer, verificationKey dolu)
 *  pair-result                                   PairResult
 *  transfer-progress                             Transfer                (tek aktarımın güncel hali; yoksa eklenir)
 *  transfers-changed                             Transfer[]              (tam liste; silme sonrası)
 *  clipboard-history-changed                     ClipItem[]
 *  settings-changed                              Settings
 *  reduce-transparency-changed                   boolean                 (zaten theme.ts kullanıyor)
 *
 * Gizlilik: Rust `ClipItem.kind === 'concealed'` için `text` alanını BOŞ gönderir.
 */
import { call, onEvent } from './tauri';
import { inTauri } from './platform';
import { mock } from './mock';
import type {
  ClipItem,
  Device,
  EventMap,
  PairingState,
  Settings,
  SettingsPatch,
  SettingsTarget,
  ShortcutId,
  Transfer,
  TroubleshootResult,
} from './types';

export * from './types';

/* ---------- Yardımcılar ---------- */

async function run<T>(cmd: string, args: Record<string, unknown> | undefined, fallback: () => T | Promise<T>): Promise<T | undefined> {
  if (!inTauri) return fallback();
  return call<T>(cmd, args);
}

/** Olaya abone ol; Tauri dışında sahte arka uçtan dinler. Dönen işlev aboneliği bırakır. */
export function subscribe<K extends keyof EventMap>(event: K, handler: (payload: EventMap[K]) => void): () => void {
  return inTauri ? onEvent<EventMap[K]>(event, handler) : mock.on(event, handler);
}

/* ---------- Cephe (facade) ---------- */

export const api = {
  async listDevices(): Promise<Device[]> {
    return (await run('list_devices', undefined, () => mock.listDevices())) ?? [];
  },
  async requestPair(deviceId: string): Promise<PairingState | null> {
    return (await run<PairingState>('request_pair', { deviceId }, () => mock.requestPair(deviceId))) ?? null;
  },
  async acceptPair(deviceId: string): Promise<void> {
    await run('accept_pair', { deviceId }, () => mock.acceptPair(deviceId));
  },
  async rejectPair(deviceId: string): Promise<void> {
    await run('reject_pair', { deviceId }, () => mock.rejectPair());
  },
  async unpair(deviceId: string): Promise<void> {
    await run('unpair', { deviceId }, () => mock.unpair(deviceId));
  },
  async pingDevice(deviceId: string): Promise<void> {
    await run('ping_device', { deviceId }, () => undefined);
  },
  async ringDevice(deviceId: string): Promise<void> {
    await run('ring_device', { deviceId }, () => undefined);
  },
  async sendClipboard(deviceId: string, text?: string): Promise<void> {
    await run('send_clipboard', { deviceId, text }, () => undefined);
  },
  async pickFiles(): Promise<string[]> {
    return (await run('pick_files', undefined, () => mock.pickFiles())) ?? [];
  },
  async pickFolder(): Promise<string | null> {
    return (await run<string | null>('pick_folder', undefined, () => mock.pickFolder())) ?? null;
  },
  async sendFiles(deviceId: string, paths: string[]): Promise<void> {
    await run('send_files', { deviceId, paths }, () => mock.sendFiles(deviceId, paths));
  },
  async listTransfers(): Promise<Transfer[]> {
    return (await run('list_transfers', undefined, () => mock.listTransfers())) ?? [];
  },
  async cancelTransfer(id: string): Promise<void> {
    await run('cancel_transfer', { id }, () => mock.cancelTransfer(id));
  },
  async retryTransfer(id: string): Promise<void> {
    await run('retry_transfer', { id }, () => mock.retryTransfer(id));
  },
  async revealFile(id: string): Promise<void> {
    await run('reveal_file', { id }, () => undefined);
  },
  async openFile(id: string): Promise<void> {
    await run('open_file', { id }, () => undefined);
  },
  async getClipboardHistory(): Promise<ClipItem[]> {
    return (await run('get_clipboard_history', undefined, () => mock.getClipboardHistory())) ?? [];
  },
  async clearClipboardHistory(): Promise<void> {
    await run('clear_clipboard_history', undefined, () => mock.clearClipboardHistory());
  },
  async getSettings(): Promise<Settings | null> {
    return (await run<Settings>('get_settings', undefined, () => mock.getSettings())) ?? null;
  },
  async setSettings(patch: SettingsPatch): Promise<Settings | null> {
    return (await run<Settings>('set_settings', { patch }, () => mock.setSettings(patch))) ?? null;
  },
  async setShortcut(id: ShortcutId, keys: string[]): Promise<boolean> {
    return (await run<boolean>('set_shortcut', { id, keys }, () => mock.setShortcut(id, keys))) === true;
  },
  async runTroubleshoot(): Promise<TroubleshootResult[]> {
    return (await run('run_troubleshoot', undefined, () => mock.runTroubleshoot())) ?? [];
  },
  async addDeviceByIp(ip: string): Promise<void> {
    await run('add_device_by_ip', { ip }, () => undefined);
  },
  async openSystemSettings(target: SettingsTarget): Promise<void> {
    await run('open_system_settings', { target }, () => undefined);
  },
  async requestLocalNetwork(): Promise<boolean> {
    return (await run<boolean>('request_local_network', undefined, () => true)) ?? false;
  },
  async appVersion(): Promise<string> {
    return (await run<{ version: string }>('app_info', undefined, () => ({ version: '0.1.0' })))?.version ?? '0.1.0';
  },
};
