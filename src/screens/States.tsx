import { useI18n } from '../i18n';
import type { Device } from '../lib/api';
import { BrandBadge } from '../components/BrandBadge';
import { DeviceArt } from '../components/DeviceArt';
import { Sheet } from '../components/Sheet';
import { SpinnerIcon } from '../components/icons';

/** "Eşleşmiş cihaz yok" durumu. */
export function NoDevices({ onSearch, onTroubleshoot }: { onSearch: () => void; onTroubleshoot: () => void }) {
  const { t } = useI18n();
  return (
    <div className="empty-state">
      <span className="orb">
        <svg
          aria-hidden="true"
          viewBox="0 0 116 116"
          width="116"
          height="116"
          fill="none"
          stroke="currentColor"
          style={{ position: 'absolute', inset: 0 }}
        >
          <path d="M30 71h6" strokeWidth="2" strokeLinecap="round" />
          <path d="M92 64h.01" strokeWidth="3" strokeLinecap="round" />
          <g className="dash-line">
            <path d="M33 34 Q58 8 80 46" strokeWidth="2" strokeLinecap="round" />
          </g>
        </svg>
      </span>
      <h2>{t('states.noDevicesTitle')}</h2>
      <p>{t('states.noDevicesBody')}</p>
      <div className="btns">
        <button type="button" className="btn btn-base xl pri" onClick={onSearch}>
          {t('states.search')}
        </button>
        <button type="button" className="btn btn-base xl glass sm btn-sec" onClick={onTroubleshoot}>
          {t('states.troubleshoot')}
        </button>
      </div>
    </div>
  );
}

/** "Bağlantı koptu · yeniden bağlanılıyor…" kahraman kartı. */
export function ConnectionLost({ device, onTroubleshoot }: { device: Device; onTroubleshoot: () => void }) {
  const { t } = useI18n();
  return (
    <div className="hero lost" role="status">
      <DeviceArt type={device.type} label={t(device.type === 'phone' ? 'device.phone' : 'device.tablet')} width={120} height={86} dim />
      <div className="info">
        <span className="name" style={{ fontSize: 20 }}>
          {device.name}
        </span>
        <span className="line">
          <span className="dot" style={{ width: 7, height: 7, background: 'var(--off)' }} />
          {t('states.lostLine')}
          <SpinnerIcon size={13} strokeWidth={2.6} className="spin" style={{ flex: 'none' }} />
        </span>
        <span className="hint">
          {t('states.lostHint')}{' '}
          <button type="button" className="link-btn" onClick={onTroubleshoot}>
            {t('states.troubleshoot')}
          </button>
        </span>
      </div>
    </div>
  );
}

/** Eşleşmeyi kaldırma onayı. */
export function UnpairDialog({ device, onConfirm, onCancel }: { device: Device; onConfirm: () => void; onCancel: () => void }) {
  const { t } = useI18n();
  return (
    <Sheet variant="alert" label={t('states.unpairAria')} onClose={onCancel}>
      <BrandBadge size={48} radius={11} glyph={34} />
      <h2>{t('states.unpairTitle', { device: device.name })}</h2>
      <p>{t('states.unpairBody')}</p>
      <button type="button" className="btn btn-base lg b600 btn-danger" onClick={onConfirm}>
        {t('states.unpairConfirm')}
      </button>
      <button type="button" data-autofocus className="btn btn-base lg b600 glass sm btn-sec" onClick={onCancel}>
        {t('states.unpairCancel')}
      </button>
    </Sheet>
  );
}
