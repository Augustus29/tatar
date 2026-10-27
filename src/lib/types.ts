/** Arayüz ↔ Rust veri tipleri (sözleşme açıklaması: api.ts). */

/* ---------- Tipler ---------- */

export type DeviceType = 'phone' | 'tablet' | 'desktop' | 'laptop' | 'tv';

export interface Battery {
  /** 0–100 */
  charge: number;
  charging: boolean;
}

export interface Device {
  id: string;
  name: string;
  type: DeviceType;
  /** Şu an ağda görünüyor / bağlı */
  reachable: boolean;
  paired: boolean;
  battery: Battery | null;
  ip: string | null;
  /** Son görülme (epoch ms); hiç görülmediyse null */
  lastSeen: number | null;
}

export type PairingKind = 'none' | 'requestedByUs' | 'requestedByPeer' | 'paired';

export interface PairingState {
  deviceId: string;
  state: PairingKind;
  /** Her iki ekranda gösterilen doğrulama kodu (ör. "7F3A91C2"); yoksa boş */
  verificationKey: string;
}

export interface PairResult {
  deviceId: string;
  success: boolean;
  /** 'rejected' | 'timeout' | 'cancelled' | 'failed' | null */
  error: string | null;
}

export type TransferState = 'active' | 'done' | 'failed' | 'cancelled';

export interface Transfer {
  id: string;
  deviceId: string;
  name: string;
  /** Toplam bayt */
  size: number;
  direction: 'send' | 'receive';
  /** Şimdiye kadar aktarılan bayt */
  progress: number;
  state: TransferState;
  /** Başlangıç zamanı (epoch ms) */
  when: number;
  /** state === 'failed' ise: 'connectionLost' | 'other' */
  error: string | null;
}

export type ClipKind = 'text' | 'link' | 'concealed';

export interface ClipItem {
  id: string;
  kind: ClipKind;
  text: string;
  /** epoch ms */
  time: number;
}

export type LanguagePref = 'system' | 'tr' | 'en';
export type ClipboardMode = 'auto' | 'manual' | 'off';
export type MuteMode = 'mute' | 'lower';
export type ShortcutId = 'sendClip' | 'sendFile' | 'findDev' | 'openWin' | 'showHist';

export const SHORTCUT_IDS: ShortcutId[] = ['sendClip', 'sendFile', 'findDev', 'openWin', 'showHist'];

export const DEFAULT_SHORTCUTS: Record<ShortcutId, string[]> = {
  sendClip: ['⇧', '⌘', 'C'],
  sendFile: ['⇧', '⌘', 'F'],
  findDev: ['⌥', '⌘', 'R'],
  openWin: ['⌥', '⌘', 'T'],
  showHist: ['⇧', '⌘', 'V'],
};

export interface Settings {
  language: LanguagePref;
  clipboardMode: ClipboardMode;
  muteOnCall: boolean;
  muteMode: MuteMode;
  downloadDir: string;
  launchAtLogin: boolean;
  showInMenuBar: boolean;
  showInDock: boolean;
  shortcuts: Record<ShortcutId, string[]>;
  notifications: { fileReceived: boolean; batteryLow: boolean };
  /** Bu bilgisayarın telefonda görünen adı */
  deviceName: string;
  /** Bu bilgisayarın pil durumunu paylaş */
  shareBattery: boolean;
  /** Aynı ağda otomatik bağlan */
  autoConnect: boolean;
  /** İlk açılış akışı tamamlandı mı */
  onboarded: boolean;
}

export type SettingsPatch = Partial<Omit<Settings, 'shortcuts'>>;

export const DEFAULT_SETTINGS: Settings = {
  language: 'system',
  clipboardMode: 'auto',
  muteOnCall: true,
  muteMode: 'mute',
  downloadDir: '',
  launchAtLogin: false,
  showInMenuBar: true,
  showInDock: false,
  shortcuts: DEFAULT_SHORTCUTS,
  notifications: { fileReceived: true, batteryLow: true },
  deviceName: '',
  shareBattery: true,
  autoConnect: true,
  onboarded: true,
};

export type TroubleshootId = 'wifi' | 'localNetwork' | 'firewall' | 'discovery' | 'phoneApp';
export type CheckStatus = 'ok' | 'warn' | 'fail';

export interface TroubleshootResult {
  id: TroubleshootId;
  status: CheckStatus;
  /** wifi için ağ adı (SSID); diğerlerinde null */
  info: string | null;
}

export type SettingsTarget = 'firewall' | 'localNetwork';

export interface EventMap {
  'devices-changed': Device[];
  'pair-request': PairingState;
  'pair-result': PairResult;
  'transfer-progress': Transfer;
  'transfers-changed': Transfer[];
  'clipboard-history-changed': ClipItem[];
  'settings-changed': Settings;
}
