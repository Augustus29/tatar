import { useEffect, useState } from 'react';

/** Belirli aralıklarla güncellenen "şimdi" (epoch ms). Render sırasında Date.now() çağırmamak için. */
export function useNow(intervalMs = 30_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}
