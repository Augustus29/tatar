import { useEffect, useRef, useState, type DragEvent } from 'react';
import { useI18n } from '../i18n';
import { formatShortcut, inTauri } from '../lib/platform';
import { call, onDragDrop } from '../lib/tauri';
import { shortAgo } from '../lib/format';
import { actions, selectPairedDevices, useStore } from '../lib/store';
import {
  DeviceIcon,
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
  TextIcon,
} from '../components/icons';
import { Switch } from '../components/Switch';
import { MediaMini } from '../components/MediaCard';
import { useMedia } from '../lib/media';

const CIRC = 2 * Math.PI * 18;

export function Popover() {
  const { t } = useI18n();
  const paired = useStore(selectPairedDevices);
  const clips = useStore((s) => s.clipboard);
  const transfers = useStore((s) => s.transfers);
  const mute = useStore((s) => s.settings.muteOnCall);
  const device = paired.find((d) => d.reachable) ?? paired[0];
  const online = !!device?.reachable;
  const media = useMedia(device?.id);
  const [ringing, setRinging] = useState(false);
  const [clipSent, setClipSent] = useState(false);
  const [drag, setDrag] = useState(false);
  const [lingerId, setLingerId] = useState<string | null>(null);
  const [sent, setSent] = useState<Record<string, boolean>>({});
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

  // Tauri'de dosya bırakma pencere olayıyla gelir (yerel yollarla); HTML5 bırakma yalnızca tarayıcı önizlemesinde çalışır.
  const dropTarget = useRef<string | null>(null);
  const dropDeviceId = online && device ? device.id : null;
  useEffect(() => {
    dropTarget.current = dropDeviceId;
  }, [dropDeviceId]);
  useEffect(
    () =>
      onDragDrop((e) => {
        if (e.type === 'drop') {
          setDrag(false);
          if (dropTarget.current && e.paths.length > 0) void actions.sendFiles(dropTarget.current, e.paths);
        } else if (e.type === 'leave') setDrag(false);
        else setDrag(true);
      }),
    [],
  );

  // Aktif aktarımı göster; bitince kısa süre "Gönderildi" olarak kalsın.
  const active = transfers.find((x) => x.state === 'active' && x.deviceId === device?.id);
  if (active && lingerId !== active.id) setLingerId(active.id);
  const lingering = transfers.find((x) => x.id === lingerId);
  const transfer = active ?? (lingering?.state === 'done' ? lingering : null);
  const lingerState = lingering?.state;
  useEffect(() => {
    if (lingerState !== 'done') return;
    const id = window.setTimeout(() => setLingerId(null), 1400);
    return () => clearTimeout(id);
  }, [lingerState]);
  const tpct = transfer && transfer.size > 0 ? Math.min(100, Math.floor((transfer.progress / transfer.size) * 100)) : 0;

  const ping = () => {
    if (!device || !online) return;
    void actions.ringDevice(device.id).then((ok) => {
      if (!ok) setRinging(false);
    });
    const r = !ringing;
    setRinging(r);
    if (r) later(() => setRinging(false), 4000);
  };
  const sendClip = () => {
    if (!device || !online) return;
    void actions.sendClipboard(device.id);
    setClipSent(true);
    later(() => setClipSent(false), 1600);
  };
  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setDrag(false);
    // Tauri'de bırakma yerel pencere olayıyla gelir (onDragDrop); burası yalnızca tarayıcı önizlemesi.
    if (inTauri) return;
    const files = Array.from(e.dataTransfer?.files ?? []);
    if (device && online && files.length > 0) {
      void actions.sendFiles(
        device.id,
        files.slice(0, 6).map((f) => (f as File & { path?: string }).path ?? f.name),
      );
    }
  };
  const copyClip = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      /* pano erişimi yok */
    }
  };

  const battery = device?.battery?.charge ?? null;
  const shortcuts = { open: formatShortcut(['⌘', 'O']), settings: formatShortcut(['⌘', ',']), quit: formatShortcut(['⌘', 'Q']) };

  const foot = (
    <>
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
    </>
  );

  if (!device) {
    return (
      <div className="pop-root">
        <div ref={panel} role="dialog" aria-label={t('app.name')} className="pop-panel">
          <div className="pop-head">
            <span className="brand-badge">
              <DeviceIcon type="tablet" size={22} />
            </span>
            <span className="nm">
              <b>{t('popover.noDevice')}</b>
              <span>{t('popover.noDeviceSub')}</span>
            </span>
          </div>
          {foot}
        </div>
      </div>
    );
  }

  return (
    <div className="pop-root">
      <div ref={panel} role="dialog" aria-label={t('app.name')} className="pop-panel">
        <div className="pop-head">
          <span className="brand-badge">
            <DeviceIcon type={device.type} size={22} />
          </span>
          <span className="nm">
            <b>{device.name}</b>
            <span>
              <span className="dot" style={{ background: online ? 'var(--green)' : 'var(--off)' }} />
              {online ? t('popover.connectedWifi') : t('device.notConnected')}
            </span>
          </span>
          {battery !== null && (
            <span className="ring">
              <svg
                role="img"
                aria-label={t(device.battery?.charging ? 'popover.batteryAria' : 'popover.batteryAriaPlain', { n: battery })}
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
                  style={{
                    stroke: battery <= 15 ? 'var(--red)' : 'var(--green)',
                    strokeWidth: 4,
                    strokeDasharray: `${(CIRC * battery) / 100} 200`,
                  }}
                />
              </svg>
              <span className="num">{battery}</span>
            </span>
          )}
        </div>

        <div className="qa-grid" style={{ opacity: online ? 1 : 0.5 }}>
          <button
            type="button"
            className="qa glass"
            disabled={!online}
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
            disabled={!online}
            onClick={sendClip}
            style={clipSent ? { ['--qbg' as string]: 'var(--green)', ['--qfg' as string]: '#fff' } : undefined}
          >
            <span className="qico">
              <ClipboardIcon size={20} strokeWidth={1.9} />
            </span>
            <span className="lbl">{clipSent ? t('popover.clipboardSent') : t('popover.clipboard')}</span>
          </button>
          <button type="button" className="qa glass" disabled={!online} onClick={() => void actions.pickAndSend(device.id)}>
            <span className="qico">
              <FileUploadIcon size={20} strokeWidth={1.9} />
            </span>
            <span className="lbl">{t('popover.file')}</span>
          </button>
        </div>

        {online && media && <MediaMini m={media} />}

        <div className="sub-panel">
          <SpeakerOffIcon size={18} style={{ color: 'var(--text2)' }} strokeWidth={1.8} />
          <span style={{ flex: 1, fontSize: 13 }}>{t('sync.muteOnCall')}</span>
          <Switch checked={mute} onChange={(v) => void actions.updateSettings({ muteOnCall: v })} label={t('sync.muteOnCall')} />
        </div>

        {transfer && (
          <div className="sub-panel" style={{ flexDirection: 'column', alignItems: 'stretch', gap: 6 }}>
            <span style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12 }}>
              <span style={{ fontWeight: 600 }}>{transfer.name}</span>
              <span style={{ color: 'var(--text2)', fontVariantNumeric: 'tabular-nums' }}>
                {transfer.state === 'done' ? t('transfers.sent') : `%${tpct}`}
              </span>
            </span>
            <span style={{ position: 'relative', height: 5, borderRadius: 3, background: 'var(--track)', overflow: 'hidden' }}>
              <span
                style={{
                  position: 'absolute',
                  left: 0,
                  top: 0,
                  bottom: 0,
                  width: `${transfer.state === 'done' ? 100 : tpct}%`,
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
            <button type="button" className="tlink" onClick={() => void actions.clearClipboard()}>
              {t('popover.clear')}
            </button>
          </div>
          <div className="hist">
            {clips.map((c) => {
              const lock = c.kind === 'concealed';
              return (
                <div className="clip" key={c.id} style={{ opacity: lock ? 0.8 : 1 }}>
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
                    {lock && <LockIcon size={16} />}
                  </span>
                  <span className="ctext" style={{ color: lock ? 'var(--muted)' : 'var(--text)' }}>
                    {lock ? t('popover.concealed') : c.text}
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
                  {!sent[c.id] && <span className="tm">{shortAgo(t, c.time)}</span>}
                  {!lock && (
                    <span className="acts">
                      <button type="button" className="cb" aria-label={t('popover.copy')} onClick={() => void copyClip(c.text)}>
                        <CopyIcon size={14} />
                      </button>
                      <button
                        type="button"
                        className="cb"
                        aria-label={t('popover.sendToPhone')}
                        disabled={!online}
                        onClick={() => {
                          void actions.sendClipboard(device.id, c.text);
                          setSent((x) => ({ ...x, [c.id]: true }));
                          later(() => setSent((x) => ({ ...x, [c.id]: false })), 1600);
                        }}
                      >
                        <SendToPhoneIcon size={14} />
                      </button>
                    </span>
                  )}
                </div>
              );
            })}
            {clips.length === 0 && (
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

        {foot}
      </div>
    </div>
  );
}
