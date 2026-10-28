import { useEffect, useSyncExternalStore } from 'react';
import { call, onEvent } from './tauri';

/** Telefondaki bir oynatıcının durumu (Rust `MediaInfo`; sözleşme: plugins/mpris.rs). */
export interface MediaInfo {
  deviceId: string;
  /** Gösterilen oynatıcının adı */
  player: string;
  /** Cihazdaki bütün oynatıcılar */
  players: string[];
  title: string;
  artist: string;
  album: string;
  isPlaying: boolean;
  /** ms; bilinmiyorsa -1 */
  length: number;
  /** 0–100; bilinmiyor / ayarlanamıyorsa -1 */
  volume: number;
  canPlay: boolean;
  canPause: boolean;
  canNext: boolean;
  canPrevious: boolean;
  canSeek: boolean;
}

export type MediaCommand = 'play' | 'pause' | 'playPause' | 'previous' | 'next';

let list: MediaInfo[] = [];
const subs = new Set<() => void>();
let started = false;

function setList(next: MediaInfo[]): void {
  list = next;
  subs.forEach((fn) => fn());
}

function start(): void {
  if (started) return;
  started = true;
  onEvent<MediaInfo[]>('media-changed', setList);
  void call<MediaInfo[]>('get_media').then((l) => l && setList(l));
}

function subscribe(fn: () => void): () => void {
  subs.add(fn);
  return () => subs.delete(fn);
}

/** Verilen cihazın gösterilecek oynatıcısı; oynatıcı yoksa (ya da Tauri dışında) null. */
export function useMedia(deviceId: string | undefined): MediaInfo | null {
  useEffect(start, []);
  const all = useSyncExternalStore(subscribe, () => list);
  return (deviceId && all.find((m) => m.deviceId === deviceId)) || null;
}

export const mediaApi = {
  async action(deviceId: string, player: string, action: MediaCommand): Promise<void> {
    await call('media_action', { deviceId, player, action });
  },
  async setVolume(deviceId: string, player: string, volume: number): Promise<void> {
    await call('media_set_volume', { deviceId, player, volume: Math.round(volume) });
  },
};
