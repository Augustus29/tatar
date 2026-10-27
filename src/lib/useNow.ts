import { useEffect, useState } from 'react';

/**
 * Belirli aralıklarla güncellenen "şimdi" (epoch ms). Render sırasında Date.now() çağırmamak için.
 * Pencere gizliyken zamanlayıcı durur; görünür olunca hemen yenilenir.
 */
export function useNow(intervalMs = 30_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    let id: number | undefined;
    const stop = () => {
      if (id !== undefined) window.clearInterval(id);
      id = undefined;
    };
    const start = () => {
      stop();
      id = window.setInterval(() => setNow(Date.now()), intervalMs);
    };
    const onVis = () => {
      if (document.hidden) {
        stop();
      } else {
        setNow(Date.now());
        start();
      }
    };
    if (!document.hidden) start();
    document.addEventListener('visibilitychange', onVis);
    return () => {
      stop();
      document.removeEventListener('visibilitychange', onVis);
    };
  }, [intervalMs]);
  return now;
}
