import { useCallback, useEffect, useRef, useState, type DragEvent } from 'react';
import { useI18n } from '../i18n';
import { formatShortcut, isMac, isWindows } from '../lib/platform';
import { initialTransfers, mockDevices, pickBytes, pickNames, type MockTransfer } from '../lib/mock';
import { Switch } from '../components/Switch';
import { Toast } from '../components/Toast';
import {
  ArrowDownIcon,
  ArrowUpIcon,
  ClipboardIcon,
  ClipboardPlainIcon,
  DotsIcon,
  DropIcon,
  FileUploadIcon,
  PhoneIcon,
  PingIcon,
  SearchIcon,
  SlidersIcon,
  SpeakerOffIcon,
  SpinnerIcon,
  TabletIcon,
} from '../components/icons';
import { SettingsPane } from './Settings';

type View = 'device' | 'settings';

const SHORTCUT = {
  clip: formatShortcut(['⇧', '⌘', 'C']),
  file: formatShortcut(['⇧', '⌘', 'F']),
  ring: formatShortcut(['⌘', 'R']),
};

export function MainWindow() {
  const { t, formatSize } = useI18n();
  const [view, setView] = useState<View>('device');
  const [sel, setSel] = useState(mockDevices[0].id);
  const [menu, setMenu] = useState(false);
  const [ringing, setRinging] = useState(false);
  const [toast, setToast] = useState('');
  const [clipOn, setClipOn] = useState(true);
  const [muteOn, setMuteOn] = useState(true);
  const [drag, setDrag] = useState(false);
  const [transfers, setTransfers] = useState<MockTransfer[]>(initialTransfers);
  const nextId = useRef(3);
  const timers = useRef<number[]>([]);
  const ringTimer = useRef<number | undefined>(undefined);
  const announced = useRef(new Set<number>(initialTransfers.map((x) => x.id)));

  const cur = mockDevices.find((d) => d.id === sel) ?? mockDevices[0];
  const online = cur.online;
  const isRinging = ringing && online;

  useEffect(
    () => () => {
      timers.current.forEach(clearTimeout);
      clearTimeout(ringTimer.current);
    },
    [],
  );

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    timers.current.push(window.setTimeout(() => setToast((c) => (c === msg ? '' : c)), 2400));
  }, []);

  // Sahte aktarım ilerlemesi (prototipteki gibi)
  const hasActive = transfers.some((x) => x.p < 100);
  useEffect(() => {
    if (!hasActive) return;
    const id = window.setInterval(() => {
      setTransfers((l) => l.map((x) => (x.p >= 100 ? x : { ...x, p: Math.min(100, x.p + 3 + Math.floor(Math.random() * 8)) })));
    }, 140);
    return () => clearInterval(id);
  }, [hasActive]);

  useEffect(() => {
    for (const x of transfers) {
      if (x.p >= 100 && !announced.current.has(x.id)) {
        announced.current.add(x.id);
        showToast(t('toast.fileSent', { name: x.name }));
      }
    }
  }, [transfers, showToast, t]);

  const addTransfers = (files: { name: string; bytes: number }[]) => {
    const fresh: MockTransfer[] = files.map((f) => ({ id: nextId.current++, name: f.name, bytes: f.bytes, dir: 'out', p: 0, when: 'now' }));
    setMenu(false);
    setTransfers((l) => fresh.concat(l).slice(0, 6));
  };

  const ping = () => {
    setMenu(false);
    if (!online) return;
    clearTimeout(ringTimer.current);
    if (ringing) {
      setRinging(false);
      return;
    }
    setRinging(true);
    ringTimer.current = window.setTimeout(() => setRinging(false), 5000);
  };
  const sendClip = () => {
    setMenu(false);
    if (online) showToast(t('toast.clipboardSent', { device: cur.name }));
  };
  const sendFile = () => {
    if (!online) {
      setMenu(false);
      return;
    }
    const k = Math.floor(Math.random() * pickNames.length);
    addTransfers([{ name: pickNames[k], bytes: pickBytes[k] }]);
  };

  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setDrag(false);
    const files = Array.from(e.dataTransfer?.files ?? []);
    if (!online || files.length === 0) return;
    addTransfers(files.slice(0, 4).map((f) => ({ name: f.name, bytes: f.size })));
  };

  const whenLabel = (w: string) => (w === 'now' ? t('transfers.now') : w === 'yesterday' ? t('transfers.yesterday') : w);
  const seen = t('device.hoursAgo', { n: cur.seenHours });
  const battColor = cur.battery <= 15 ? 'var(--red)' : 'var(--green)';
  const title = view === 'settings' ? t('settings.title') : cur.name;
  const subtitle = view === 'settings' ? '' : online ? t('device.connectedBattery', { battery: cur.battery }) : t('device.notConnected');

  return (
    <div className="app">
      <aside className={`glass sbar sidebar${isMac ? ' mac' : ''}`}>
        {isMac && <div className="drag-top" data-tauri-drag-region />}
        <div className="side-label">{t('sidebar.myDevices')}</div>
        {mockDevices.map((d) => {
          const selected = view === 'device' && d.id === sel;
          return (
            <button
              key={d.id}
              type="button"
              className="hov dev-row"
              data-sel={selected ? '1' : '0'}
              aria-current={selected ? 'true' : undefined}
              onClick={() => {
                setSel(d.id);
                setView('device');
                setMenu(false);
                setRinging(false);
              }}
            >
              <span className="dev-icon">{d.kind === 'tablet' ? <TabletIcon /> : <PhoneIcon />}</span>
              <span className="dev-text">
                <span className="dev-name">{d.name}</span>
                <span className="dev-status">
                  <span className="dot" style={{ background: d.online ? 'var(--green)' : 'var(--off)' }} />
                  {d.online ? t('device.connectedBattery', { battery: d.battery }) : t('device.notConnected')}
                </span>
              </span>
            </button>
          );
        })}

        <div className="side-label row">
          <span>{t('sidebar.nearby')}</span>
          <SpinnerIcon size={12} style={{ animation: 'spin 1.1s linear infinite' }} />
        </div>
        <div className="nearby-row">
          <span className="dev-icon">
            <PhoneIcon />
          </span>
          <span style={{ flex: 1, fontSize: 13, fontWeight: 500 }}>Pixel 8</span>
          <button type="button" className="btn btn-base pri">
            {t('sidebar.pair')}
          </button>
        </div>

        <div style={{ flex: 1, minHeight: 24 }} />
        <div className="side-foot">
          <span className="who">
            <span style={{ fontSize: 11, color: 'var(--text2)' }}>{t('sidebar.thisComputer')}</span>
            <span style={{ fontSize: 12, fontWeight: 600 }}>MacBook Pro</span>
          </span>
          <button
            type="button"
            aria-label={t('sidebar.settings')}
            aria-pressed={view === 'settings'}
            className="hov round-btn"
            data-sel={view === 'settings' ? '1' : '0'}
            onClick={() => {
              setView((v) => (v === 'settings' ? 'device' : 'settings'));
              setMenu(false);
            }}
          >
            <SlidersIcon />
          </button>
        </div>
      </aside>

      <main className="main">
        <div className="toolbar-fade" />
        <div className="toolbar" style={{ paddingRight: isWindows ? 150 : 10 }}>
          <div className="title" data-tauri-drag-region>
            <span className="t1">{title}</span>
            {subtitle && <span className="t2">{subtitle}</span>}
          </div>
          {view === 'device' && (
            <div className="glass capsule">
              <button type="button" className="tbb" aria-label={t('toolbar.findDevice')} onClick={ping}>
                <PingIcon strokeWidth={1.7} />
              </button>
              <button type="button" className="tbb" aria-label={t('toolbar.sendClipboard')} onClick={sendClip}>
                <ClipboardIcon strokeWidth={1.7} />
              </button>
              <button
                type="button"
                className="tbb"
                aria-label={t('toolbar.moreActions')}
                aria-expanded={menu}
                aria-haspopup="menu"
                onClick={() => setMenu((m) => !m)}
              >
                <DotsIcon />
              </button>
            </div>
          )}
        </div>

        {view === 'settings' ? (
          <SettingsPane clipOn={clipOn} />
        ) : (
          <div className="scroll">
            {online ? (
              <div className="hero" key={`on-${cur.id}`}>
                <div style={{ position: 'relative', width: 140, height: 100, flex: 'none' }}>
                  {isRinging && (
                    <>
                      <span className="pulse-ring" />
                      <span className="pulse-ring" style={{ animationDelay: '.55s' }} />
                    </>
                  )}
                  <svg
                    role="img"
                    aria-label={t('device.tablet')}
                    viewBox="0 0 140 100"
                    width="140"
                    height="100"
                    style={{ position: 'relative', animation: isRinging ? 'wiggle .55s ease-in-out infinite' : 'none' }}
                  >
                    <rect x="4" y="6" width="132" height="88" rx="12" style={{ fill: 'var(--devFrame)' }} />
                    <rect x="10" y="12" width="120" height="76" rx="7" style={{ fill: 'var(--devScreen)' }} />
                    <path
                      d="M10 70 Q50 56 90 64 T130 60 V81 a7 7 0 0 1 -7 7 H17 a7 7 0 0 1 -7 -7 Z"
                      style={{ fill: 'var(--accent)', opacity: 0.35 }}
                    />
                    <circle cx="102" cy="34" r="9" style={{ fill: 'var(--accent)', opacity: 0.5 }} />
                  </svg>
                </div>
                <div className="info">
                  <span className="name">{cur.name}</span>
                  <span className="line">
                    <span className="dot" style={{ width: 7, height: 7, background: 'var(--green)' }} />
                    {t('device.connectedLine', { net: t('device.wifi'), ip: cur.ip })}
                  </span>
                  {isRinging && <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--accent)' }}>{t('device.ringing')}</span>}
                </div>
                <div className="batt">
                  <span className="pct">%{cur.battery}</span>
                  <span className="bar-batt">
                    <span style={{ width: `${cur.battery}%`, background: battColor }} />
                  </span>
                  <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12, color: 'var(--text2)' }}>
                    <svg aria-hidden="true" viewBox="0 0 24 24" width="12" height="12" fill="currentColor">
                      <path d="M13 2 5 14h6l-1 8 8-12h-6z" />
                    </svg>
                    {t('device.charging')}
                  </span>
                </div>
              </div>
            ) : (
              <div className="hero" key={`off-${cur.id}`}>
                <svg
                  role="img"
                  aria-label={t('device.phone')}
                  viewBox="0 0 140 100"
                  width="140"
                  height="100"
                  style={{ opacity: 0.5, flex: 'none' }}
                >
                  <rect x="44" y="2" width="52" height="96" rx="11" style={{ fill: 'var(--devFrame)' }} />
                  <rect x="49" y="7" width="42" height="86" rx="7" style={{ fill: 'var(--devScreen)' }} />
                </svg>
                <div className="info">
                  <span className="name">{cur.name}</span>
                  <span className="line">
                    <span className="dot" style={{ width: 7, height: 7, background: 'var(--off)' }} />
                    {t('device.lastSeen', { seen })}
                  </span>
                  <span style={{ fontSize: 13, color: 'var(--text2)', maxWidth: 420 }}>{t('device.autoReconnect')}</span>
                </div>
                <div className="batt" style={{ gap: 4 }}>
                  <span className="pct" style={{ color: 'var(--text2)' }}>
                    %{cur.battery}
                  </span>
                  <span style={{ fontSize: 12, color: 'var(--text2)' }}>{t('device.lastKnown')}</span>
                </div>
              </div>
            )}

            <section>
              <h2 className="h2">{t('actions.heading')}</h2>
              <div className="tiles" style={{ opacity: online ? 1 : 0.4 }}>
                <button type="button" className="tile" onClick={ping}>
                  <span className={`ico${isRinging ? ' on' : ''}`}>
                    <PingIcon size={20} style={{ animation: isRinging ? 'wiggle .55s ease-in-out infinite' : 'none' }} />
                  </span>
                  <span className="txt">
                    <b>{isRinging ? t('actions.stopRinging') : t('actions.findDevice')}</b>
                    <span>{isRinging ? t('device.ringing') : t('actions.findDeviceSub')}</span>
                  </span>
                </button>
                <button type="button" className="tile" onClick={sendClip}>
                  <span className="ico">
                    <ClipboardIcon size={20} />
                  </span>
                  <span className="txt">
                    <b>{t('actions.sendClipboard')}</b>
                    <span>{SHORTCUT.clip}</span>
                  </span>
                </button>
                <button type="button" className="tile" onClick={sendFile}>
                  <span className="ico">
                    <FileUploadIcon size={20} />
                  </span>
                  <span className="txt">
                    <b>{t('actions.sendFile')}</b>
                    <span>{SHORTCUT.file}</span>
                  </span>
                </button>
              </div>
            </section>

            <section>
              <h2 className="h2">{t('sync.heading')}</h2>
              <div className="card">
                <div className="srow">
                  <span className="sico" style={{ background: '#E07A1F' }}>
                    <ClipboardPlainIcon size={17} />
                  </span>
                  <span className="stxt">
                    <b>{t('sync.clipboard')}</b>
                    <span>{t('sync.clipboardSub')}</span>
                  </span>
                  <Switch checked={clipOn} onChange={setClipOn} label={t('sync.clipboard')} />
                </div>
                <div className="rule" />
                <div className="srow">
                  <span className="sico" style={{ background: '#5B5BD6' }}>
                    <SpeakerOffIcon size={17} />
                  </span>
                  <span className="stxt">
                    <b>{t('sync.muteOnCall')}</b>
                    <span>{t('sync.muteOnCallSub')}</span>
                  </span>
                  <Switch checked={muteOn} onChange={setMuteOn} label={t('sync.muteOnCall')} />
                </div>
              </div>
            </section>

            <section>
              <div className="sec-head">
                <h2 className="h2">{t('transfers.heading')}</h2>
                <button
                  type="button"
                  className="btn btn-base glass sm btn-glass"
                  onClick={() => setTransfers((l) => l.filter((x) => x.p < 100))}
                >
                  {t('transfers.clearDone')}
                </button>
              </div>
              <div
                className={`dz${drag ? ' over' : ''}`}
                onDragOver={(e) => {
                  e.preventDefault();
                  if (!drag) setDrag(true);
                }}
                onDragLeave={() => setDrag(false)}
                onDrop={onDrop}
              >
                <DropIcon />
                <span>{drag ? t('transfers.dropRelease') : t('transfers.dropHint')}</span>
                <button type="button" className="btn btn-base pri" onClick={sendFile}>
                  {t('transfers.chooseFile')}
                </button>
              </div>
              <div className="card">
                {transfers.map((x) => (
                  <div className="trow" key={x.id}>
                    <span className="tico">{x.dir === 'out' ? <ArrowUpIcon size={17} /> : <ArrowDownIcon size={17} />}</span>
                    <div className="tbody">
                      <span className="tname">{x.name}</span>
                      {x.p < 100 ? (
                        <span className="prog">
                          <span className="bar">
                            <span style={{ width: `${x.p}%` }} />
                          </span>
                          <span className="pt">%{x.p}</span>
                        </span>
                      ) : (
                        <span className="meta">
                          {formatSize(x.bytes)} · {x.dir === 'out' ? t('transfers.sent') : t('transfers.received')} · {whenLabel(x.when)}
                        </span>
                      )}
                    </div>
                    {x.p >= 100 && (
                      <button
                        type="button"
                        className="hov round-btn"
                        aria-label={t(isMac ? 'transfers.showInFolder' : 'transfers.showInFolderWin')}
                      >
                        <SearchIcon size={17} />
                      </button>
                    )}
                  </div>
                ))}
                {transfers.length === 0 && <div className="empty">{t('transfers.none')}</div>}
              </div>
            </section>
          </div>
        )}

        {menu && view === 'device' && (
          <>
            <button type="button" className="scrim" aria-label={t('toolbar.closeMenu')} onClick={() => setMenu(false)} />
            <div role="menu" className="glass-strong menu" onKeyDown={(e) => e.key === 'Escape' && setMenu(false)}>
              <button type="button" role="menuitem" className="mi" onClick={sendClip}>
                <span>{t('actions.sendClipboard')}</span>
                <span className="sc">{SHORTCUT.clip}</span>
              </button>
              <button type="button" role="menuitem" className="mi" onClick={sendFile}>
                <span>{t('actions.sendFile')}</span>
                <span className="sc">{SHORTCUT.file}</span>
              </button>
              <button type="button" role="menuitem" className="mi" onClick={ping}>
                <span>{t('menu.findDevice')}</span>
                <span className="sc">{SHORTCUT.ring}</span>
              </button>
              <div className="menu-sep" />
              <button
                type="button"
                role="menuitem"
                className="mi"
                onClick={() => {
                  setMenu(false);
                  setView('settings');
                }}
              >
                {t('menu.deviceSettings')}
              </button>
              <button
                type="button"
                role="menuitem"
                className="mi danger"
                onClick={() => {
                  setMenu(false);
                  showToast(t('toast.unpairConfirm'));
                }}
              >
                {t('menu.unpair')}
              </button>
            </div>
          </>
        )}

        <Toast message={toast} />
      </main>
    </div>
  );
}
