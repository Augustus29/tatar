import { useEffect } from 'react';
import { call, onEvent } from './tauri';

const params = new URLSearchParams(window.location.search);

/**
 * Temayı <html> üzerindeki sınıflara yazar:
 *  - `dark`: sistem koyu modda
 *  - `solid`: sistemde "Saydamlığı azalt" açık (cam yüzeyler opak olur)
 * Tarayıcıda önizlemek için ?dark=1 ve ?solid=1 kullanılabilir.
 */
export function useTheme(): void {
  useEffect(() => {
    const root = document.documentElement;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const forceDark = params.get('dark');
    const applyDark = () => root.classList.toggle('dark', forceDark ? forceDark === '1' : mq.matches);
    applyDark();
    mq.addEventListener('change', applyDark);

    const forceSolid = params.get('solid');
    const applySolid = (v: boolean) => root.classList.toggle('solid', forceSolid ? forceSolid === '1' : v);
    applySolid(false);
    void call<boolean>('reduce_transparency').then((v) => applySolid(!!v));
    const off = onEvent<boolean>('reduce-transparency-changed', applySolid);

    return () => {
      mq.removeEventListener('change', applyDark);
      off();
    };
  }, []);
}
