import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
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
