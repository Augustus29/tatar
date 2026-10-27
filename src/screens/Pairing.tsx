import { useI18n } from '../i18n';
import { actions, useStore } from '../lib/store';
import { codeGroups } from '../lib/format';
import { BrandBadge } from '../components/BrandBadge';
import { Sheet } from '../components/Sheet';
import { CheckIcon, DeviceIcon, SpinnerIcon } from '../components/icons';

/**
 * Eşleştirme sayfası (pencere içi sheet). Üç durum:
 *  giden istek (kod + telefondan onay bekleniyor) · gelen istek (Reddet / Kabul et) · başarı.
 * Durum `store.pairing` içinden gelir; Rust `pair-request` / `pair-result` olaylarıyla günceller.
 */
export function PairingSheet() {
  const { t } = useI18n();
  const pairing = useStore((s) => s.pairing);
  const device = useStore((s) => s.devices.find((d) => d.id === s.pairing?.deviceId));
  if (!pairing || pairing.state === 'none') return null;

  const name = device?.name ?? '';
  const incoming = pairing.state === 'requestedByPeer';
  const success = pairing.state === 'paired';
  const groups = codeGroups(pairing.verificationKey);

  if (success) {
    return (
      <Sheet label={t('pairing.aria')} onClose={actions.dismissPairing}>
        <span className="success-badge">
          <CheckIcon size={34} strokeWidth={2.6} className="draw-check" />
        </span>
        <h2>{t('pairing.successTitle')}</h2>
        <p>{t('pairing.successBody', { device: name })}</p>
        <div className="sheet-actions">
          <button type="button" data-autofocus className="btn btn-base xl pri" onClick={actions.dismissPairing}>
            {t('common.done')}
          </button>
        </div>
      </Sheet>
    );
  }

  return (
    <Sheet label={t('pairing.aria')} onClose={() => void actions.rejectPair()}>
      <div className="pair-link">
        <BrandBadge size={48} radius={11} glyph={34} />
        <svg aria-hidden="true" viewBox="0 0 80 10" width="80" height="10" className="dash-line" fill="none" stroke="var(--accent)">
          <path d="M2 5 H78" strokeWidth="2.5" strokeLinecap="round" />
        </svg>
        <span className="phone-chip">
          <DeviceIcon type={device?.type ?? 'phone'} size={26} strokeWidth={1.6} />
        </span>
      </div>
      <h2>{t(incoming ? 'pairing.inTitle' : 'pairing.outTitle', { device: name })}</h2>
      <p>{t(incoming ? 'pairing.inBody' : 'pairing.outBody')}</p>
      {pairing.verificationKey && (
        <div className="pair-code" role="img" aria-label={t('pairing.codeAria', { code: groups.join(' ') })}>
          {groups.map((g, i) => (
            <span key={i} style={{ display: 'contents' }}>
              {i > 0 && <span className="sepdot">·</span>}
              <span>{g}</span>
            </span>
          ))}
        </div>
      )}
      {incoming ? (
        <div className="sheet-actions">
          <button type="button" className="btn btn-base lg glass sm btn-sec" onClick={() => void actions.rejectPair()}>
            {t('pairing.reject')}
          </button>
          <button type="button" data-autofocus className="btn btn-base lg b600 pri" onClick={() => void actions.acceptPair()}>
            {t('pairing.accept')}
          </button>
        </div>
      ) : (
        <>
          <span className="pair-wait" role="status">
            <SpinnerIcon size={14} strokeWidth={2.4} className="spin" />
            {t('pairing.waiting')}
          </span>
          <div className="sheet-actions">
            <button type="button" className="btn btn-base lg glass sm btn-sec" onClick={() => void actions.rejectPair()}>
              {t('common.cancel')}
            </button>
          </div>
        </>
      )}
    </Sheet>
  );
}
