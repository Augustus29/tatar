import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { inTauri } from './platform';

/** Rust komutunu çağırır; Tauri dışında (tarayıcı önizlemesi) sessizce geçer. */
export async function call<T = void>(cmd: string, args?: Record<string, unknown>): Promise<T | undefined> {
  if (!inTauri) return undefined;
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    console.warn(`komut başarısız: ${cmd}`, e);
    return undefined;
  }
}

export function broadcast(event: string, payload?: unknown): void {
  if (inTauri) void emit(event, payload);
}

export function onEvent<T>(event: string, handler: (payload: T) => void): () => void {
  if (!inTauri) return () => {};
  let un: UnlistenFn | undefined;
  let cancelled = false;
  void listen<T>(event, (e) => handler(e.payload)).then((u) => {
    if (cancelled) u();
    else un = u;
  });
  return () => {
    cancelled = true;
    un?.();
  };
}

/**
 * Bu pencerenin (webview) dosya sürükle-bırak olayları. `onEvent('tauri://drag-drop')` tüm
 * pencerelerin olaylarını alabildiğinden (ana pencere + popover), yinelenen gönderimi önlemek
 * için yalnızca kendi webview'ine ait olaylar dinlenir.
 */
export function onDragDrop(handler: (e: { type: 'enter' | 'over' | 'drop' | 'leave'; paths: string[] }) => void): () => void {
  if (!inTauri) return () => {};
  let un: UnlistenFn | undefined;
  let cancelled = false;
  void getCurrentWebview()
    .onDragDropEvent((e) => {
      const p = e.payload;
      handler({ type: p.type, paths: 'paths' in p ? p.paths : [] });
    })
    .then((u) => {
      if (cancelled) u();
      else un = u;
    });
  return () => {
    cancelled = true;
    un?.();
  };
}
