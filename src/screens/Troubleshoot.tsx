import { useEffect, useRef, useState } from 'react';
import { useI18n } from '../i18n';
import { api, type CheckStatus, type TroubleshootId, type TroubleshootResult } from '../lib/api';
import { isMac, isWindows } from '../lib/platform';
import { CheckIcon, CloseIcon, ExclaimIcon, SpinnerIcon } from '../components/icons';

const ORDER: TroubleshootId[] = ['wifi', 'localNetwork', 'firewall', 'discovery', 'phoneApp'];
/** Düzeltme adımı olan denetimler */
const HAS_FIX: TroubleshootId[] = ['localNetwork', 'firewall', 'discovery'];
const STEP_MS = 700;
const IPV4 = /^(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}$/;

/**
 * Bağlantı sorunlarını gider. Rust `run_troubleshoot` bütün sonuçları tek seferde verir;
 * arayüz bunları prototipteki gibi sırayla (700 ms arayla) açar.
 * Başlat düğmesi araç çubuğu hizasında (ana pencerenin sağ üstü) durur.
 */
export function Troubleshoot() {
  const { t } = useI18n();
  /** -1: başlamadı · 0–4: o satır denetleniyor · 5: bitti */
  const [step, setStep] = useState(-1);
  const [results, setResults] = useState<TroubleshootResult[]>([]);
  const [notes, setNotes] = useState<Partial<Record<TroubleshootId, boolean>>>({});
  const [manual, setManual] = useState(false);
  const [ip, setIp] = useState('');
  const [tried, setTried] = useState<'no' | 'ok' | 'bad'>('no');
  const token = useRef(0);
  const timer = useRef<number | undefined>(undefined);

  useEffect(
    () => () => {
      token.current++;
      clearInterval(timer.current);
    },
    [],
  );

  const start = async () => {
    const mine = ++token.current;
    clearInterval(timer.current);
    setStep(0);
    setNotes({});
    setManual(false);
    setTried('no');
    const res = await api.runTroubleshoot();
    if (mine !== token.current) return;
    setResults(res);
    timer.current = window.setInterval(() => {
      setStep((n) => {
        if (n >= 4) clearInterval(timer.current);
        return n + 1;
      });
    }, STEP_MS);
  };
  const reset = () => {
    token.current++;
    clearInterval(timer.current);
    setStep(-1);
    setNotes({});
    setManual(false);
    setIp('');
    setTried('no');
  };

  const statusOf = (id: TroubleshootId): CheckStatus => results.find((r) => r.id === id)?.status ?? 'warn';
  const detailOf = (id: TroubleshootId, status: CheckStatus): string => {
    if (id === 'wifi' && status === 'ok') {
      const ssid = results.find((r) => r.id === 'wifi')?.info;
      return ssid ? t('troubleshoot.wifi.ok', { ssid }) : t('troubleshoot.wifi.okNoName');
    }
    return t(`troubleshoot.${id}.${status}`);
  };

  const isDone = step >= 5;
  const isRunning = step >= 0 && step < 5;
  const issues = isDone ? ORDER.filter((id) => statusOf(id) !== 'ok').length : 0;

  const fix = (id: TroubleshootId) => {
    if (id === 'firewall' || id === 'localNetwork') void api.openSystemSettings(id);
    setNotes((n) => ({ ...n, [id]: !n[id] }));
  };
  const connect = () => {
    const v = ip.trim();
    if (!IPV4.test(v)) {
      setTried('bad');
      return;
    }
    void api.addDeviceByIp(v);
    setTried('ok');
  };

  return (
    <>
      <div className="ts-action" style={{ right: isWindows ? 150 : 10 }}>
        {step < 0 && (
          <button type="button" className="btn btn-base xl pri" onClick={() => void start()}>
            {t('troubleshoot.start')}
          </button>
        )}
        {isRunning && (
          <span className="ts-running" role="status">
            <SpinnerIcon size={14} strokeWidth={2.6} className="spin" />
            {t('troubleshoot.running')}
          </span>
        )}
        {isDone && (
          <button type="button" className="btn btn-base xl glass sm btn-sec" onClick={reset}>
            {t('troubleshoot.retry')}
          </button>
        )}
      </div>
      <div className="scroll" style={{ gap: 12 }}>
        <p role="status" style={{ margin: 0, fontSize: 13, lineHeight: 1.4, color: 'var(--text2)' }}>
          {t(step < 0 ? 'troubleshoot.introIdle' : isRunning ? 'troubleshoot.introRunning' : 'troubleshoot.introDone')}
        </p>

        <ul className="ts-list">
          {ORDER.map((id, i) => {
            const finished = step > i;
            const running = step === i;
            const status = finished ? statusOf(id) : null;
            const bad = status !== null && status !== 'ok';
            return (
              <li key={id}>
                <div className="ts-row">
                  <span className="ts-ico">
                    {status === null && !running && <span className="pend" />}
                    {running && <SpinnerIcon size={18} strokeWidth={2.6} className="spin" style={{ color: 'var(--accent)' }} />}
                    {status === 'ok' && (
                      <span className="mark ok">
                        <CheckIcon size={15} strokeWidth={3} />
                      </span>
                    )}
                    {status === 'warn' && (
                      <span className="mark warn">
                        <ExclaimIcon size={15} />
                      </span>
                    )}
                    {status === 'fail' && (
                      <span className="mark fail">
                        <CloseIcon size={14} strokeWidth={3} />
                      </span>
                    )}
                    <span className="sr-only">{t(`troubleshoot.statusAria.${status ?? 'pending'}`)}</span>
                  </span>
                  <span className="stxt">
                    <b>{t(`troubleshoot.${id}.title`)}</b>
                    <span className={bad ? 'bad' : undefined}>
                      {status !== null ? detailOf(id, status) : t(running ? 'troubleshoot.checking' : 'troubleshoot.pending')}
                    </span>
                  </span>
                  {bad && HAS_FIX.includes(id) && (
                    <button
                      type="button"
                      className="btn btn-base glass sm btn-glass fix"
                      aria-expanded={!!notes[id]}
                      onClick={() => fix(id)}
                    >
                      {t(`troubleshoot.${id}.fix`)}
                    </button>
                  )}
                </div>
                {bad && notes[id] && <div className="ts-note">{t(`troubleshoot.${id}.${isMac ? 'noteMac' : 'noteWin'}`)}</div>}
              </li>
            );
          })}
        </ul>

        {isDone && (
          <>
            <div className={`ts-summary${issues === 0 ? ' good' : ''}`}>
              <span className="badge">{issues === 0 ? <CheckIcon size={16} strokeWidth={3} /> : <ExclaimIcon size={16} />}</span>
              <span className="stxt">
                <b>{issues === 0 ? t('troubleshoot.noIssues') : t('troubleshoot.issues', { n: issues })}</b>
                <span>{t(issues === 0 ? 'troubleshoot.allGood' : 'troubleshoot.summaryHint')}</span>
              </span>
              <button
                type="button"
                className="btn btn-base lg b600 glass sm btn-sec"
                aria-expanded={manual}
                onClick={() => setManual((m) => !m)}
              >
                {t('troubleshoot.manual')}
              </button>
            </div>
            {manual && (
              <>
                <div className="ts-manual">
                  <label htmlFor="ts-ip">{t('troubleshoot.ipLabel')}</label>
                  <input
                    id="ts-ip"
                    className="field"
                    type="text"
                    inputMode="decimal"
                    autoComplete="off"
                    placeholder={t('troubleshoot.ipPlaceholder')}
                    value={ip}
                    onChange={(e) => {
                      setIp(e.target.value);
                      setTried('no');
                    }}
                    onKeyDown={(e) => e.key === 'Enter' && connect()}
                  />
                  <button type="button" className="btn btn-base lg b600 pri" style={{ flex: 'none' }} onClick={connect}>
                    {t('troubleshoot.connect')}
                  </button>
                </div>
                {tried !== 'no' && (
                  <span className={`ts-tried${tried === 'bad' ? ' err' : ''}`} role="status">
                    {tried === 'ok' ? t('troubleshoot.tried', { ip: ip.trim() }) : t('troubleshoot.ipInvalid')}
                  </span>
                )}
              </>
            )}
          </>
        )}
      </div>
    </>
  );
}
