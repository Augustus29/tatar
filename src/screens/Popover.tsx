import { useEffect, useRef, useState, type DragEvent } from 'react';
import { useI18n } from '../i18n';
import { formatShortcut } from '../lib/platform';
import { call } from '../lib/tauri';
import { mockClips } from '../lib/mock';
import {
  CheckIcon,
  ClipboardIcon,
  CopyIcon,
  DropIcon,
  FileUploadIcon,
  LinkIcon,
  LockIcon,
  PingIcon,
  SendToPhoneIcon,
  SpeakerOffIcon,
  TabletIcon,
  TextIcon,
} from '../components/icons';
import { Switch } from '../components/Switch';

const CIRC = 2 * Math.PI * 18;

export function Popover() {
  const { t } = useI18n();
  const [ringing, setRinging] = useState(false);
  const [clipSent, setClipSent] = useState(false);
  const [mute, setMute] = useState(true);
  const [drag, setDrag] = useState(false);
  const [transfer, setTransfer] = useState<{ name: string; p: number } | null>(null);
  const [sent, setSent] = useState<Record<string, boolean>>({});
  const [cleared, setCleared] = useState(false);
  const panel = useRef<HTMLDivElement>(null);
  const timers = useRef<number[]>([]);

  const later = (fn: () => void, ms: number) => timers.current.push(window.setTimeout(fn, ms));
  useEffect(() => () => timers.current.forEach(clearTimeout), []);

  // İçerik yüksekliği değişince pencereyi uyarla
  useEffect(() => {
    const el = panel.current;
    if (!el) return;
    const report = () => void call('resize_popover', { height: el.offsetHeight });
    const ro = new ResizeObserver(report);
    ro.observe(el);
    report();
    return () => ro.disconnect();
  }, []);

  // Esc ile kapat
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && void call('hide_popover');
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  // Sahte aktarım ilerlemesi
  const transferActive = transfer !== null && transfer.p < 100;
  useEffect(() => {
    if (!transferActive) return;
    const id = window.setInterval(() => {
      setTransfer((c) => (c ? { ...c, p: Math.min(100, c.p + 3 + Math.floor(Math.random() * 7)) } : c));
    }, 120);
    return () => clearInterval(id);
  }, [transferActive]);
  useEffect(() => {
    if (transfer?.p === 100) {
      const id = window.setTimeout(() => setTransfer(null), 1400);
      return () => clearTimeout(id);
    }
  }, [transfer?.p]);

  const ping = () => {
    const r = !ringing;
    setRinging(r);
    if (r) later(() => setRinging(false), 4000);
  };
  const sendClip = () => {
    setClipSent(true);
    later(() => setClipSent(false), 1600);
  };
  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setDrag(false);
    const f = e.dataTransfer?.files?.[0];
    if (f) setTransfer({ name: f.name, p: 0 });
  };

  const timeLabel = (c: (typeof mockClips)[number]) =>
    c.time.unit === 'now'
      ? t('popover.now')
      : c.time.unit === 'min'
        ? t('popover.minutes', { n: c.time.n })
        : t('popover.hours', { n: c.time.n });

  const shortcuts = { open: formatShortcut(['⌘', 'O']), settings: formatShortcut(['⌘', ',']), quit: formatShortcut(['⌘', 'Q']) };
  const battery = 78;

  return (
    <div className="pop-root">
      <div ref={panel} role="dialog" aria-label={t('app.name')} className="pop-panel">
        <div className="pop-head">
          <span className="brand-badge">
            <TabletIcon size={22} />
          </span>
          <span className="nm">
            <b>Galaxy Tab S10 FE</b>
            <span>
              <span className="dot" style={{ background: 'var(--green)' }} />
              {t('popover.connectedWifi')}
            </span>
          </span>
          <span className="ring">
            <svg
              role="img"
              aria-label={t('popover.batteryAria', { n: battery })}
              viewBox="0 0 44 44"
              width="44"
              height="44"
              style={{ transform: 'rotate(-90deg)' }}
            >
              <circle cx="22" cy="22" r="18" fill="none" style={{ stroke: 'var(--track)', strokeWidth: 4 }} />
              <circle
                cx="22"
                cy="22"
                r="18"
                fill="none"
                strokeLinecap="round"
                style={{ stroke: 'var(--green)', strokeWidth: 4, strokeDasharray: `${(CIRC * battery) / 100} 200` }}
              />
            </svg>
            <span className="num">{battery}</span>
          </span>
        </div>

        <div className="qa-grid">
          <button
            type="button"
            className="qa glass"
            onClick={ping}
            style={ringing ? { ['--qbg' as string]: 'var(--accent)', ['--qfg' as string]: 'var(--onAccent)' } : undefined}
          >
            <span className="qico">
              {ringing && (
                <span
                  className="pulse-ring"
                  style={{ width: 40, height: 40, borderRadius: 20, animation: 'pulse 1.4s ease-out infinite' }}
                />
              )}
              <PingIcon size={20} strokeWidth={1.9} style={{ animation: ringing ? 'wiggle .45s ease-in-out infinite' : 'none' }} />
            </span>
            <span className="lbl">{ringing ? t('popover.pingStop') : t('popover.ping')}</span>
          </button>
          <button
            type="button"
            className="qa glass"
            onClick={sendClip}
            style={clipSent ? { ['--qbg' as string]: 'var(--green)', ['--qfg' as string]: '#fff' } : undefined}
          >
            <span className="qico">
              <ClipboardIcon size={20} strokeWidth={1.9} />
            </span>
            <span className="lbl">{clipSent ? t('popover.clipboardSent') : t('popover.clipboard')}</span>
          </button>
          <button type="button" className="qa glass" onClick={() => setTransfer({ name: 'Kat_Planı_A1.dwg', p: 0 })}>
            <span className="qico">
              <FileUploadIcon size={20} strokeWidth={1.9} />
            </span>
            <span className="lbl">{t('popover.file')}</span>
          </button>
        </div>

        <div className="sub-panel">
          <SpeakerOffIcon size={18} style={{ color: 'var(--text2)' }} strokeWidth={1.8} />
          <span style={{ flex: 1, fontSize: 13 }}>{t('sync.muteOnCall')}</span>
          <Switch checked={mute} onChange={setMute} label={t('sync.muteOnCall')} />
        </div>

        {transfer && (
          <div className="sub-panel" style={{ flexDirection: 'column', alignItems: 'stretch', gap: 6 }}>
            <span style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12 }}>
              <span style={{ fontWeight: 600 }}>{transfer.name}</span>
              <span style={{ color: 'var(--text2)', fontVariantNumeric: 'tabular-nums' }}>
                {transfer.p >= 100 ? t('transfers.sent') : `%${transfer.p}`}
              </span>
            </span>
            <span style={{ position: 'relative', height: 5, borderRadius: 3, background: 'var(--track)', overflow: 'hidden' }}>
              <span
                style={{
                  position: 'absolute',
                  left: 0,
                  top: 0,
                  bottom: 0,
                  width: `${transfer.p}%`,
                  borderRadius: 3,
                  background: 'var(--accent)',
                  transition: 'width .14s linear',
                }}
              />
            </span>
          </div>
        )}

        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          <div className="hist-head">
            <span>{t('popover.history')}</span>
            <button type="button" className="tlink" onClick={() => setCleared(true)}>
              {t('popover.clear')}
            </button>
          </div>
          <div className="hist">
            {(cleared ? [] : mockClips).map((c) => (
              <div className="clip" key={c.id} style={{ opacity: c.kind === 'lock' ? 0.8 : 1 }}>
                <span
                  style={{
                    width: 16,
                    height: 16,
                    flex: 'none',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    color: 'var(--text2)',
                  }}
                >
                  {c.kind === 'text' && <TextIcon size={16} />}
                  {c.kind === 'link' && <LinkIcon size={16} />}
                  {c.kind === 'lock' && <LockIcon size={16} />}
                </span>
                <span className="ctext" style={{ color: c.kind === 'lock' ? 'var(--muted)' : 'var(--text)' }}>
                  {c.kind === 'lock' ? t('popover.concealed') : c.text}
                </span>
                {sent[c.id] && (
                  <span
                    style={{
                      flex: 'none',
                      display: 'flex',
                      alignItems: 'center',
                      gap: 3,
                      fontSize: 11,
                      fontWeight: 600,
                      color: 'var(--green)',
                    }}
                  >
                    <CheckIcon size={12} strokeWidth={3} />
                    {t('popover.clipboardSent')}
                  </span>
                )}
                {!sent[c.id] && <span className="tm">{timeLabel(c)}</span>}
                {c.kind !== 'lock' && (
                  <span className="acts">
                    <button type="button" className="cb" aria-label={t('popover.copy')}>
                      <CopyIcon size={14} />
                    </button>
                    <button
                      type="button"
                      className="cb"
                      aria-label={t('popover.sendToPhone')}
                      onClick={() => {
                        setSent((s) => ({ ...s, [c.id]: true }));
                        later(() => setSent((s) => ({ ...s, [c.id]: false })), 1600);
                      }}
                    >
                      <SendToPhoneIcon size={14} />
                    </button>
                  </span>
                )}
              </div>
            ))}
            {cleared && (
              <div
                style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: 40, fontSize: 12, color: 'var(--text2)' }}
              >
                {t('popover.empty')}
              </div>
            )}
          </div>
        </div>

        <div
          className={`pop-drop${drag ? ' over' : ''}`}
          onDragOver={(e) => {
            e.preventDefault();
            if (!drag) setDrag(true);
          }}
          onDragLeave={() => setDrag(false)}
          onDrop={onDrop}
        >
          <DropIcon size={16} />
          <span>{drag ? t('popover.dropRelease') : t('popover.dropHint')}</span>
        </div>

        <div className="pop-sep" />
        <div className="pop-foot">
          <button type="button" className="mi" onClick={() => void call('show_main_window')}>
            {t('popover.open')}
            <span className="sc">{shortcuts.open}</span>
          </button>
          <button type="button" className="mi" onClick={() => void call('show_main_window')}>
            {t('popover.settings')}
            <span className="sc">{shortcuts.settings}</span>
          </button>
          <button type="button" className="mi" onClick={() => void call('quit_app')}>
            {t('popover.quit')}
            <span className="sc">{shortcuts.quit}</span>
          </button>
        </div>
      </div>
    </div>
  );
}
