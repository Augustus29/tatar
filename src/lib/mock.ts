/** Aşama 1: sahte veriler. Gerçek veriler Aşama 2'den itibaren Rust tarafından gelir. */

export interface MockDevice {
  id: string;
  name: string;
  kind: 'tablet' | 'phone';
  online: boolean;
  battery: number;
  net: string;
  ip: string;
  /** Çevrilecek "son görülme" — saat sayısı */
  seenHours: number;
}

export const mockDevices: MockDevice[] = [
  { id: 'tab', name: 'Galaxy Tab S10 FE', kind: 'tablet', online: true, battery: 78, net: 'wifi', ip: '192.168.1.101', seenHours: 0 },
  { id: 'phone', name: 'Galaxy S23', kind: 'phone', online: false, battery: 41, net: '', ip: '', seenHours: 2 },
];

export interface MockTransfer {
  id: number;
  name: string;
  bytes: number;
  dir: 'out' | 'in';
  /** 0–100 */
  p: number;
  when: string;
}

export const initialTransfers: MockTransfer[] = [
  { id: 2, name: 'Proje_Sunum_v3.pdf', bytes: 4_404_019, dir: 'out', p: 100, when: '14:32' },
  { id: 1, name: 'IMG_2041.HEIC', bytes: 2_936_012, dir: 'in', p: 100, when: 'yesterday' },
];

export const pickNames = ['Kat_Planı_A1.dwg', 'Render_Salon_04.png', 'Şartname_Rev2.docx', 'Cephe_Detay.pdf'];
export const pickBytes = [13_212_057, 8_808_038, 655_360, 3_250_585];

export interface MockClip {
  id: string;
  kind: 'link' | 'text' | 'lock';
  text: string;
  /** 'now' | dakika | saat */
  time: { unit: 'now' } | { unit: 'min' | 'hour'; n: number };
}

export const mockClips: MockClip[] = [
  { id: 'c1', kind: 'link', text: 'https://meet.google.com/abc-defg-hij', time: { unit: 'now' } },
  { id: 'c2', kind: 'text', text: 'Ofis adresi: Kuştepe Mah. … [ADRES]', time: { unit: 'min', n: 3 } },
  { id: 'c3', kind: 'text', text: 'A-A kesiti kot +3.40 revize', time: { unit: 'min', n: 12 } },
  { id: 'c4', kind: 'lock', text: '', time: { unit: 'hour', n: 1 } },
];
