import { useEffect, useState } from 'react';
import { useI18n } from '../i18n';
import { api } from '../lib/api';
import { actions, selectNearbyDevices, useStore } from '../lib/store';
import { isMac } from '../lib/platform';
import { BrandBadge } from '../components/BrandBadge';
import { CheckIcon, DeviceIcon, WifiIcon } from '../components/icons';

type Phase = 'steps' | 'search' | 'found';

const PLAY_LINK = 'https://play.google.com/store/apps/details?id=org.kde.kdeconnect_tp';

interface OnboardingProps {
  /** 'search': adımları atlayıp doğrudan aramayı göster (ör. "Cihaz ara" düğmesinden) */
  startAt?: Phase;
  /** Akış bitti ya da atlandı */
  onFinish: () => void;
}

/** İlk açılış: üç adım → cihaz arama → bulunan cihazı eşleştir. */
export function Onboarding({ startAt = 'steps', onFinish }: OnboardingProps) {
  const { t } = useI18n();
  const [d1, setD1] = useState(false);
  const [d2, setD2] = useState(false);
  const [phase, setPhase] = useState<Phase>(startAt);
  const nearby = useStore(selectNearbyDevices);
  const found = nearby[0];
  const ready = d1 && d2;

  // Arama aşaması: gerçek bir cihaz görününce (en az 1,6 sn aradıktan sonra) "bulundu"ya geç.
  const [waited, setWaited] = useState(false);
  useEffect(() => {
    if (phase !== 'search') return;
    const id = window.setTimeout(() => setWaited(true), 1600);
    return () => clearTimeout(id);
  }, [phase]);
  const shown: Phase = phase === 'search' && waited && found ? 'found' : phase;

  const copyLink = async () => {
    try {
      await navigator.clipboard.writeText(PLAY_LINK);
    } catch {
      /* pano erişimi yoksa yine de adımı tamamla */
    }
    setD1(true);
  };
  const allowNetwork = async () => {
    await api.requestLocalNetwork();
    setD2(true);
  };
  const next = () => {
    if (!ready) {
      setD1(true);
      setD2(true);
      return;
    }
    setWaited(false);
    setPhase('search');
  };
  const pair = () => {
    if (!found) return;
    void actions.requestPair(found.id);
    onFinish();
  };

  const steps = [
    {
      n: 1,
      title: t('onboarding.step1Title'),
      body: t('onboarding.step1Body'),
      done: d1,
      btn: t('onboarding.step1Btn'),
      act: () => void copyLink(),
      primary: false,
    },
    {
      n: 2,
      title: t('onboarding.step2Title'),
      body: t('onboarding.step2Body'),
      done: d2,
      btn: t('onboarding.step2Btn'),
      act: () => void allowNetwork(),
      primary: true,
    },
    { n: 3, title: t('onboarding.step3Title'), body: t('onboarding.step3Body'), done: false, btn: '', act: () => {}, primary: false },
  ];

  return (
    <div className="onb" role="region" aria-label={t('onboarding.welcome')}>
      {isMac && <div className="drag-top" data-tauri-drag-region />}
      <span style={{ marginTop: 6, animation: shown === 'search' ? 'gallop .4s ease-in-out infinite' : 'none', display: 'flex' }}>
        <BrandBadge size={76} radius={17} glyph={60} glow />
      </span>
      <h1>{t('onboarding.welcome')}</h1>
      <p className="intro">{t('onboarding.intro')}</p>

      {shown === 'steps' && (
        <ol className="onb-steps" aria-label={t('onboarding.stepsAria')}>
          {steps.map((s) => (
            <li className="onb-step" key={s.n}>
              <span className={`badge${s.done ? ' done' : ''}`} aria-hidden="true">
                {s.done ? <CheckIcon size={16} strokeWidth={2.8} /> : s.n}
              </span>
              <span className="stxt">
                <b>{s.title}</b>
                <span>{s.body}</span>
              </span>
              {s.done && <span className="sr-only">{t('onboarding.stepDone')}</span>}
              {s.btn && !s.done && (
                <button type="button" className={`btn btn-base ${s.primary ? 'pri' : 'glass sm btn-sec'}`} onClick={s.act}>
                  {s.btn}
                </button>
              )}
            </li>
          ))}
        </ol>
      )}

      {shown === 'search' && (
        <div className="onb-phase" role="status">
          <div className="radar" aria-hidden="true">
            <span className="wave" />
            <span className="wave" style={{ animationDelay: '.73s' }} />
            <span className="wave" style={{ animationDelay: '1.46s' }} />
            <span className="core">
              <WifiIcon size={24} />
            </span>
          </div>
          <span style={{ fontSize: 14, fontWeight: 600 }}>{t('onboarding.searching')}</span>
          <span style={{ fontSize: 12, color: 'var(--text2)' }}>{t('onboarding.searchingHint')}</span>
        </div>
      )}

      {shown === 'found' && found && (
        <div className="onb-phase">
          <div className="found-card">
            <span className="ico">
              <DeviceIcon type={found.type} size={24} />
            </span>
            <span className="stxt">
              <span style={{ fontSize: 14, fontWeight: 700 }}>{t('onboarding.found', { device: found.name })}</span>
              <span style={{ fontSize: 12, color: 'var(--text2)' }}>{t('onboarding.foundLine', { ip: found.ip ?? '' })}</span>
            </span>
            <button type="button" data-autofocus className="btn btn-base lg b600 pri" onClick={pair}>
              {t('onboarding.pair')}
            </button>
          </div>
        </div>
      )}

      <span style={{ flex: 1, minHeight: 16 }} />
      {shown === 'steps' ? (
        <div className="onb-foot">
          <span>{t('onboarding.needPhone')}</span>
          <button type="button" className={`btn btn-base lg b600 ${ready ? 'pri' : 'glass sm btn-sec'}`} onClick={next}>
            {t('onboarding.search')}
          </button>
        </div>
      ) : (
        <button type="button" className="link-btn" onClick={onFinish}>
          {t('onboarding.skip')}
        </button>
      )}
    </div>
  );
}
