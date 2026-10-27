import { useI18n } from '../i18n';
import type { Transfer } from '../lib/api';
import { actions } from '../lib/store';
import { whenLabel } from '../lib/format';
import { isMac } from '../lib/platform';
import { ArrowDownIcon, ArrowUpIcon, CloseIcon, OpenIcon, SearchIcon } from './icons';

/** Aktarım listesi satırı: aktif · bitti · başarısız (Tekrar dene / Kaldır) · iptal edildi. */
export function TransferRow({ x }: { x: Transfer }) {
  const { t, lang, formatSize } = useI18n();
  const pct = x.size > 0 ? Math.min(100, Math.floor((x.progress / x.size) * 100)) : 0;
  const dirIcon = x.direction === 'send' ? <ArrowUpIcon size={17} /> : <ArrowDownIcon size={17} />;
  const locale = lang === 'tr' ? 'tr-TR' : 'en-US';

  if (x.state === 'active') {
    return (
      <div className="trow">
        <span className="tico">{dirIcon}</span>
        <div className="tbody">
          <span className="tname">{x.name}</span>
          <span className="prog">
            <span className="bar" role="progressbar" aria-label={x.name} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}>
              <span style={{ width: `${pct}%` }} />
            </span>
            <span className="pt">%{pct}</span>
          </span>
        </div>
        <button
          type="button"
          className="hov round-btn"
          aria-label={t('transfers.cancel')}
          onClick={() => void actions.cancelTransfer(x.id)}
        >
          <CloseIcon size={16} />
        </button>
      </div>
    );
  }

  if (x.state === 'failed') {
    return (
      <div className="trow failed" role="group" aria-label={t('transfers.failedAria', { name: x.name })}>
        <span className="tico">{dirIcon}</span>
        <div className="tbody">
          <span className="tname">{x.name}</span>
          <span className="bar-fail">
            <span style={{ width: `${pct}%` }} />
          </span>
          <span className="meta fail">
            {t(x.error === 'connectionLost' ? 'transfers.failedLost' : 'transfers.failedOther', { p: pct })}
          </span>
        </div>
        <span className="acts">
          <button type="button" className="btn btn-base lg b600 pri" onClick={() => void actions.retryTransfer(x.id)}>
            {t('transfers.retry')}
          </button>
          <button type="button" className="btn btn-base lg b600 glass sm btn-sec" onClick={() => void actions.cancelTransfer(x.id)}>
            {t('transfers.remove')}
          </button>
        </span>
      </div>
    );
  }

  if (x.state === 'cancelled') {
    return (
      <div className="trow">
        <span className="tico">{dirIcon}</span>
        <div className="tbody">
          <span className="tname">{x.name}</span>
          <span className="meta">{t('transfers.cancelled')}</span>
        </div>
        <button type="button" className="btn btn-base lg b600 glass sm btn-sec" onClick={() => void actions.cancelTransfer(x.id)}>
          {t('transfers.remove')}
        </button>
      </div>
    );
  }

  return (
    <div className="trow">
      <span className="tico">{dirIcon}</span>
      <div className="tbody">
        <span className="tname">{x.name}</span>
        <span className="meta">
          {formatSize(x.size)} · {t(x.direction === 'send' ? 'transfers.sent' : 'transfers.received')} · {whenLabel(t, x.when, locale)}
        </span>
      </div>
      {x.direction === 'receive' && (
        <button type="button" className="hov round-btn" aria-label={t('transfers.open')} onClick={() => void actions.openFile(x.id)}>
          <OpenIcon size={17} />
        </button>
      )}
      <button
        type="button"
        className="hov round-btn"
        aria-label={t(isMac ? 'transfers.showInFolder' : 'transfers.showInFolderWin')}
        onClick={() => void actions.revealFile(x.id)}
      >
        <SearchIcon size={17} />
      </button>
    </div>
  );
}
