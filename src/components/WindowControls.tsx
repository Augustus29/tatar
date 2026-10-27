import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useI18n } from '../i18n';
import { inTauri } from '../lib/platform';

/** Windows 11 tarzı küçült / büyüt-geri al / kapat düğmeleri (yalnızca Windows'ta çizilir). */
export function WindowControls() {
  const { t } = useI18n();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!inTauri) return;
    const win = getCurrentWindow();
    let un: (() => void) | undefined;
    let cancelled = false;
    const sync = () => void win.isMaximized().then((m) => !cancelled && setMaximized(m));
    sync();
    void win.onResized(sync).then((u) => {
      if (cancelled) u();
      else un = u;
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, []);

  const act = (fn: (w: ReturnType<typeof getCurrentWindow>) => Promise<void>) => () => {
    if (inTauri) void fn(getCurrentWindow());
  };

  return (
    <div className="caps">
      <button type="button" className="cap" aria-label={t('common.minimize')} onClick={act((w) => w.minimize())}>
        <svg aria-hidden="true" viewBox="0 0 10 10" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="1">
          <path d="M0 5.5h10" />
        </svg>
      </button>
      <button
        type="button"
        className="cap"
        aria-label={maximized ? t('common.restore') : t('common.maximize')}
        onClick={act((w) => w.toggleMaximize())}
      >
        {maximized ? (
          <svg aria-hidden="true" viewBox="0 0 10 10" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="1">
            <rect x="0.5" y="2.5" width="7" height="7" />
            <path d="M2.5 2.5v-2h7v7h-2" />
          </svg>
        ) : (
          <svg aria-hidden="true" viewBox="0 0 10 10" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="1">
            <rect x="0.5" y="0.5" width="9" height="9" />
          </svg>
        )}
      </button>
      <button type="button" className="cap close" aria-label={t('common.close')} onClick={act((w) => w.close())}>
        <svg aria-hidden="true" viewBox="0 0 10 10" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="1">
          <path d="M0 0l10 10M10 0L0 10" />
        </svg>
      </button>
    </div>
  );
}
