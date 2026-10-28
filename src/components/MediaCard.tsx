import { useRef, useState } from 'react';
import { useI18n } from '../i18n';
import { mediaApi, type MediaInfo } from '../lib/media';
import { MusicIcon, PauseIcon, PlayIcon, SkipNextIcon, SkipPreviousIcon, SpeakerIcon } from './icons';

/** Önceki / oynat-duraklat / sonraki düğmeleri. */
function Controls({ m, size }: { m: MediaInfo; size: number }) {
  const { t } = useI18n();
  const send = (a: 'previous' | 'next' | 'play' | 'pause') => void mediaApi.action(m.deviceId, m.player, a);
  return (
    <span className="media-ctl">
      <button
        type="button"
        className="media-btn"
        aria-label={t('media.previous')}
        disabled={!m.canPrevious}
        onClick={() => send('previous')}
      >
        <SkipPreviousIcon size={size} />
      </button>
      {m.isPlaying ? (
        <button type="button" className="media-btn main" aria-label={t('media.pause')} disabled={!m.canPause} onClick={() => send('pause')}>
          <PauseIcon size={size} />
        </button>
      ) : (
        <button type="button" className="media-btn main" aria-label={t('media.play')} disabled={!m.canPlay} onClick={() => send('play')}>
          <PlayIcon size={size} />
        </button>
      )}
      <button type="button" className="media-btn" aria-label={t('media.next')} disabled={!m.canNext} onClick={() => send('next')}>
        <SkipNextIcon size={size} />
      </button>
    </span>
  );
}

function VolumeRow({ m }: { m: MediaInfo }) {
  const { t } = useI18n();
  // Sürüklerken yerel değer gösterilir; bırakınca telefonun bildirdiği değere dönülür.
  const [drag, setDrag] = useState<number | null>(null);
  const vol = drag ?? m.volume;
  const last = useRef(0);
  const commit = () => {
    if (drag !== null) void mediaApi.setVolume(m.deviceId, m.player, drag);
    setDrag(null);
  };
  const change = (v: number) => {
    setDrag(v);
    // Sürüklerken telefonu sıkıştırma: en çok ~7 istek/sn.
    const now = Date.now();
    if (now - last.current > 150) {
      last.current = now;
      void mediaApi.setVolume(m.deviceId, m.player, v);
    }
  };
  return (
    <div className="srow media-vol">
      <SpeakerIcon size={17} style={{ color: 'var(--text2)', flex: 'none' }} />
      <input
        type="range"
        className="media-range"
        min={0}
        max={100}
        value={vol}
        aria-label={t('media.volume')}
        onChange={(e) => change(Number(e.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
    </div>
  );
}

/** Telefondaki oynatıcıyı denetleyen kart (ana pencere). */
export function MediaCard({ m }: { m: MediaInfo }) {
  const { t } = useI18n();
  return (
    <div className="card">
      <div className="srow">
        <span className="sico" style={{ background: '#C2336B' }}>
          <MusicIcon size={17} />
        </span>
        <span className="stxt">
          <b className="media-title">{m.title || t('media.nothingPlaying')}</b>
          <span className="media-sub">{m.artist ? `${m.artist} · ${m.player}` : m.player}</span>
        </span>
        <Controls m={m} size={18} />
      </div>
      {m.volume >= 0 && (
        <>
          <div className="rule" />
          <VolumeRow m={m} />
        </>
      )}
    </div>
  );
}

/** Menü çubuğu popover'ı için küçük sürüm. */
export function MediaMini({ m }: { m: MediaInfo }) {
  const { t } = useI18n();
  return (
    <div className="sub-panel" role="group" aria-label={t('media.heading')}>
      <MusicIcon size={18} style={{ color: 'var(--text2)', flex: 'none' }} strokeWidth={1.8} />
      <span style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' }}>
        <span className="media-title" style={{ fontSize: 13, fontWeight: 600 }}>
          {m.title || t('media.nothingPlaying')}
        </span>
        <span className="media-sub" style={{ fontSize: 11, color: 'var(--text2)' }}>
          {m.artist ? `${m.artist} · ${m.player}` : m.player}
        </span>
      </span>
      <Controls m={m} size={16} />
    </div>
  );
}
