import { useCallback, useEffect, useRef, useState, type DragEvent, type KeyboardEvent as ReactKeyEvent } from 'react';
import { useI18n } from '../i18n';
import { formatShortcut, inTauri, isMac, isWindows } from '../lib/platform';
import { onDragDrop } from '../lib/tauri';
import { actions, onNotice, selectNearbyDevices, selectPairedDevices, useStore } from '../lib/store';
import { seenAgo } from '../lib/format';
import { useNow } from '../lib/useNow';
import type { Device } from '../lib/api';
import { Switch } from '../components/Switch';
import { Toast } from '../components/Toast';
import { WindowControls } from '../components/WindowControls';
import { DeviceArt } from '../components/DeviceArt';
import { TransferRow } from '../components/TransferRow';
import { MediaCard } from '../components/MediaCard';
import { useMedia } from '../lib/media';
import {
  ClipboardIcon,
  ClipboardPlainIcon,
  DeviceIcon,
  DotsIcon,
  DropIcon,
  FileUploadIcon,
  PingIcon,
  SlidersIcon,
  SpeakerOffIcon,
  SpinnerIcon,
} from '../components/icons';
import { SettingsNav, SettingsPane, type SettingsTab } from './Settings';
import { PairingSheet } from './Pairing';
import { Onboarding } from './Onboarding';
import { Troubleshoot } from './Troubleshoot';
import { ConnectionLost, NoDevices, UnpairDialog } from './States';

type View = 'device' | 'settings' | 'troubleshoot';

/** Bu süreden kısa önce görülen ama şu an erişilemeyen cihaz "bağlantı koptu" sayılır. */
const LOST_WINDOW_MS = 10 * 60_000;

export function MainWindow() {
  const { t } = useI18n();
  const now = useNow();
  const ready = useStore((s) => s.ready);
  const settings = useStore((s) => s.settings);
  const paired = useStore(selectPairedDevices);
  const nearby = useStore(selectNearbyDevices);
  const allTransfers = useStore((s) => s.transfers);
  const pairing = useStore((s) => s.pairing);

  const [view, setView] = useState<View>('device');
  const [tab, setTab] = useState<SettingsTab>('general');
  const [selId, setSelId] = useState<string | null>(null);
  const [menu, setMenu] = useState(false);
  const [ringing, setRinging] = useState(false);
  const [toast, setToast] = useState('');
  const [drag, setDrag] = useState(false);
  const [unpairTarget, setUnpairTarget] = useState<Device | null>(null);
  const [onbReopen, setOnbReopen] = useState(false);
  const timers = useRef<number[]>([]);
  const ringTimer = useRef<number | undefined>(undefined);

  const cur = paired.find((d) => d.id === selId) ?? paired[0];
  const online = !!cur?.reachable;
  const media = useMedia(cur?.id);
  const isRinging = ringing && online;
  const transfers = allTransfers.filter((x) => x.deviceId === cur?.id);
  const hasFinished = transfers.some((x) => x.state !== 'active');
  const lost = !!cur && !cur.reachable && cur.lastSeen !== null && now - cur.lastSeen < LOST_WINDOW_MS;

  const sc = {
    clip: formatShortcut(settings.shortcuts.sendClip),
    file: formatShortcut(settings.shortcuts.sendFile),
    ring: formatShortcut(settings.shortcuts.findDev),
  };

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

  // Depodan gelen kısa iletiler (eşleştirme reddedildi, dosya gönderildi…)
  useEffect(() => onNotice((n) => showToast(t(n.key, n.params))), [showToast, t]);

  // Tauri'de dosya bırakma pencere olayıyla gelir (yerel yollarla).
  const dropTarget = useRef<string | null>(null);
  useEffect(() => {
    dropTarget.current = online && cur ? cur.id : null;
  }, [online, cur]);
  useEffect(() => {
    return onDragDrop((e) => {
      if (e.type === 'drop') {
        setDrag(false);
        if (dropTarget.current) void actions.sendFiles(dropTarget.current, e.paths);
      } else if (e.type === 'leave') setDrag(false);
      else setDrag(true);
    });
  }, []);

  const menuRef = useRef<HTMLDivElement>(null);
  const moreRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (menu) menuRef.current?.querySelector<HTMLElement>('[role=menuitem]')?.focus();
  }, [menu]);
  const onMenuKey = (e: ReactKeyEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeMenu();
      return;
    }
    const items = Array.from(menuRef.current?.querySelectorAll<HTMLElement>('[role=menuitem]') ?? []);
    const i = items.indexOf(document.activeElement as HTMLElement);
    let n = -1;
    if (e.key === 'ArrowDown') n = (i + 1) % items.length;
    else if (e.key === 'ArrowUp') n = (i - 1 + items.length) % items.length;
    else if (e.key === 'Home') n = 0;
    else if (e.key === 'End') n = items.length - 1;
    else if (e.key === 'Tab') setMenu(false);
    if (n >= 0) {
      e.preventDefault();
      items[n]?.focus();
    }
  };
  const closeMenu = () => {
    setMenu(false);
    moreRef.current?.focus();
  };
  const go = (v: View) => {
    setView(v);
    setMenu(false);
    setRinging(false);
  };

  const ring = () => {
    setMenu(false);
    if (!cur || !online) return;
    clearTimeout(ringTimer.current);
    void actions.ringDevice(cur.id).then((ok) => {
      if (!ok) setRinging(false);
    });
    if (ringing) {
      setRinging(false);
      return;
    }
    setRinging(true);
    ringTimer.current = window.setTimeout(() => setRinging(false), 5000);
  };
  const sendClip = () => {
    setMenu(false);
    if (!cur || !online) return;
    void actions.sendClipboard(cur.id);
    showToast(t('toast.clipboardSent', { device: cur.name }));
  };
  const sendFile = () => {
    setMenu(false);
    if (cur && online) void actions.pickAndSend(cur.id);
  };
  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setDrag(false);
    // Tauri'de bırakma yerel pencere olayıyla gelir (yukarıdaki onDragDrop); burası yalnızca tarayıcı önizlemesi.
    if (inTauri) return;
    const files = Array.from(e.dataTransfer?.files ?? []);
    if (!cur || !online || files.length === 0) return;
    void actions.sendFiles(
      cur.id,
      files.slice(0, 6).map((f) => (f as File & { path?: string }).path ?? f.name),
    );
  };
  const confirmUnpair = async () => {
    const d = unpairTarget;
    if (!d) return;
    setUnpairTarget(null);
    await actions.unpair(d.id);
    setSelId(null);
    setView('device');
    showToast(t('toast.unpaired', { device: d.name }));
  };
  const finishOnboarding = () => {
    setOnbReopen(false);
    if (!settings.onboarded) void actions.updateSettings({ onboarded: true });
  };

  const seen = seenAgo(t, cur?.lastSeen ?? null);
  const battery = cur?.battery ?? null;
  const battColor = battery && battery.charge <= 15 ? 'var(--red)' : 'var(--green)';
  const tabTitle: Record<SettingsTab, string> = {
    general: t('settings.title'),
    device: t('settings.titleDevice'),
    shortcuts: t('settings.tabShortcuts'),
    about: t('settings.tabAbout'),
  };
  const title = view === 'settings' ? tabTitle[tab] : view === 'troubleshoot' ? t('troubleshoot.title') : (cur?.name ?? t('app.name'));
  const subtitle =
    view !== 'device' || !cur
      ? ''
      : online
        ? battery
          ? t('device.connectedBattery', { battery: battery.charge })
          : t('device.wifi')
        : t('device.notConnected');
  const showTools = view === 'device' && !!cur;
  const deviceKind = (d: Device) => t(d.type === 'phone' ? 'device.phone' : 'device.tablet');

  if (ready && (!settings.onboarded || onbReopen)) {
    return (
      <>
        <Onboarding startAt={onbReopen ? 'search' : 'steps'} onFinish={finishOnboarding} />
        {/* Akış sırasında telefondan eşleştirme isteği gelirse de görünsün */}
        <PairingSheet />
        {isWindows && <WindowControls />}
      </>
    );
  }

  return (
    <div className="app">
      <aside className={`glass sbar sidebar${isMac ? ' mac' : ''}${isWindows ? ' win' : ''}`}>
        {isMac && <div className="drag-top" data-tauri-drag-region />}
        {view === 'settings' ? (
          <SettingsNav tab={tab} onTab={setTab} onBack={() => go('device')} deviceName={cur?.name ?? null} />
        ) : (
          <>
            <div className="side-label">{t('sidebar.myDevices')}</div>
            {paired.map((d) => {
              const selected = view === 'device' && d.id === cur?.id;
              return (
                <button
                  key={d.id}
                  type="button"
                  className="hov dev-row"
                  data-sel={selected ? '1' : '0'}
                  aria-current={selected ? 'true' : undefined}
                  onClick={() => {
                    setSelId(d.id);
                    go('device');
                  }}
                >
                  <span className="dev-icon">
                    <DeviceIcon type={d.type} />
                  </span>
                  <span className="dev-text">
                    <span className="dev-name">{d.name}</span>
                    <span className="dev-status">
                      <span className="dot" style={{ background: d.reachable ? 'var(--green)' : 'var(--off)' }} />
                      {d.reachable && d.battery
                        ? t('device.connectedBattery', { battery: d.battery.charge })
                        : d.reachable
                          ? t('device.wifi')
                          : t('device.notConnected')}
                    </span>
                  </span>
                </button>
              );
            })}

            <div className="side-label row">
              <span>{t('sidebar.nearby')}</span>
              <SpinnerIcon size={12} style={{ animation: 'spin 1.1s linear infinite' }} />
            </div>
            {nearby.map((d) => {
              const requested = pairing?.deviceId === d.id && pairing.state === 'requestedByUs';
              return (
                <div className="nearby-row" key={d.id}>
                  <span className="dev-icon">
                    <DeviceIcon type={d.type} />
                  </span>
                  <span className="dev-text" style={{ flex: 1 }}>
                    <span className="dev-name">{d.name}</span>
                    <span className="dev-sub">{t(requested ? 'pairing.statusRequested' : 'pairing.statusUnpaired')}</span>
                  </span>
                  <button type="button" className="btn btn-base pri" disabled={requested} onClick={() => void actions.requestPair(d.id)}>
                    {t('sidebar.pair')}
                  </button>
                </div>
              );
            })}

            <div style={{ flex: 1, minHeight: 24 }} />
            <div className="side-foot">
              <span className="who">
                <span style={{ fontSize: 11, color: 'var(--text2)' }}>{t('sidebar.thisComputer')}</span>
                <span style={{ fontSize: 12, fontWeight: 600 }}>{settings.deviceName}</span>
              </span>
              <button
                type="button"
                aria-label={t('sidebar.settings')}
                aria-pressed={false}
                className="hov round-btn"
                data-sel="0"
                onClick={() => {
                  setTab('general');
                  go('settings');
                }}
              >
                <SlidersIcon />
              </button>
            </div>
          </>
        )}
      </aside>

      <main className="main">
        <div className="toolbar-fade" />
        <div className={`toolbar${isWindows ? ' win' : ''}`}>
          <div className="title" data-tauri-drag-region>
            <span className="t1">{title}</span>
            {subtitle && <span className="t2">{subtitle}</span>}
          </div>
          {showTools && (
            <div className="glass capsule">
              <button type="button" className="tbb" aria-label={t('toolbar.findDevice')} onClick={ring}>
                <PingIcon strokeWidth={1.7} />
              </button>
              <button type="button" className="tbb" aria-label={t('toolbar.sendClipboard')} onClick={sendClip}>
                <ClipboardIcon strokeWidth={1.7} />
              </button>
              <button
                ref={moreRef}
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

        {view === 'settings' && (
          <SettingsPane tab={tab} device={cur} onUnpair={setUnpairTarget} onTroubleshoot={() => go('troubleshoot')} />
        )}
        {view === 'troubleshoot' && <Troubleshoot />}

        {view === 'device' && !cur && (
          <div className="scroll">
            <NoDevices onSearch={() => setOnbReopen(true)} onTroubleshoot={() => go('troubleshoot')} />
          </div>
        )}

        {view === 'device' && cur && (
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
                  <DeviceArt
                    type={cur.type}
                    label={deviceKind(cur)}
                    style={{ position: 'relative', animation: isRinging ? 'wiggle .55s ease-in-out infinite' : 'none' }}
                  />
                </div>
                <div className="info">
                  <span className="name">{cur.name}</span>
                  <span className="line">
                    <span className="dot" style={{ width: 7, height: 7, background: 'var(--green)' }} />
                    {t('device.connectedLine', { net: t('device.wifi'), ip: cur.ip ?? '' })}
                  </span>
                  {isRinging && <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--accent)' }}>{t('device.ringing')}</span>}
                </div>
                {battery && (
                  <div className="batt">
                    <span className="pct">%{battery.charge}</span>
                    <span className="bar-batt">
                      <span style={{ width: `${battery.charge}%`, background: battColor }} />
                    </span>
                    {battery.charging && (
                      <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12, color: 'var(--text2)' }}>
                        <svg aria-hidden="true" viewBox="0 0 24 24" width="12" height="12" fill="currentColor">
                          <path d="M13 2 5 14h6l-1 8 8-12h-6z" />
                        </svg>
                        {t('device.charging')}
                      </span>
                    )}
                  </div>
                )}
              </div>
            ) : lost ? (
              <ConnectionLost device={cur} onTroubleshoot={() => go('troubleshoot')} />
            ) : (
              <div className="hero" key={`off-${cur.id}`}>
                <DeviceArt type={cur.type} label={deviceKind(cur)} dim live={false} />
                <div className="info">
                  <span className="name">{cur.name}</span>
                  <span className="line">
                    <span className="dot" style={{ width: 7, height: 7, background: 'var(--off)' }} />
                    {t('device.lastSeen', { seen })}
                  </span>
                  <span style={{ fontSize: 13, color: 'var(--text2)', maxWidth: 420 }}>{t('device.autoReconnect')}</span>
                </div>
                {battery && (
                  <div className="batt" style={{ gap: 4 }}>
                    <span className="pct" style={{ color: 'var(--text2)' }}>
                      %{battery.charge}
                    </span>
                    <span style={{ fontSize: 12, color: 'var(--text2)' }}>{t('device.lastKnown')}</span>
                  </div>
                )}
              </div>
            )}

            <section>
              <h2 className="h2">{t('actions.heading')}</h2>
              <div className="tiles" style={{ opacity: online ? 1 : 0.4 }} aria-disabled={!online}>
                <button type="button" className="tile" onClick={ring} disabled={!online}>
                  <span className={`ico${isRinging ? ' on' : ''}`}>
                    <PingIcon size={20} style={{ animation: isRinging ? 'wiggle .55s ease-in-out infinite' : 'none' }} />
                  </span>
                  <span className="txt">
                    <b>{isRinging ? t('actions.stopRinging') : t('actions.findDevice')}</b>
                    <span>{isRinging ? t('device.ringing') : t('actions.findDeviceSub')}</span>
                  </span>
                </button>
                <button type="button" className="tile" onClick={sendClip} disabled={!online}>
                  <span className="ico">
                    <ClipboardIcon size={20} />
                  </span>
                  <span className="txt">
                    <b>{t('actions.sendClipboard')}</b>
                    <span>{sc.clip}</span>
                  </span>
                </button>
                <button type="button" className="tile" onClick={sendFile} disabled={!online}>
                  <span className="ico">
                    <FileUploadIcon size={20} />
                  </span>
                  <span className="txt">
                    <b>{t('actions.sendFile')}</b>
                    <span>{sc.file}</span>
                  </span>
                </button>
              </div>
            </section>

            {online && media && (
              <section>
                <h2 className="h2">{t('media.heading')}</h2>
                <MediaCard m={media} />
              </section>
            )}

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
                  <Switch
                    checked={settings.clipboardMode !== 'off'}
                    onChange={(v) => void actions.updateSettings({ clipboardMode: v ? 'auto' : 'off' })}
                    label={t('sync.clipboard')}
                  />
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
                  <Switch
                    checked={settings.muteOnCall}
                    onChange={(v) => void actions.updateSettings({ muteOnCall: v })}
                    label={t('sync.muteOnCall')}
                  />
                </div>
              </div>
            </section>

            <section>
              <div className="sec-head">
                <h2 className="h2">{t('transfers.heading')}</h2>
                <button
                  type="button"
                  className="btn btn-base glass sm btn-glass"
                  disabled={!hasFinished}
                  onClick={() => void actions.clearFinished(cur.id)}
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
                <button type="button" className="btn btn-base pri" disabled={!online} onClick={sendFile}>
                  {t('transfers.chooseFile')}
                </button>
              </div>
              <div className="card">
                {transfers.map((x) => (
                  <TransferRow key={x.id} x={x} />
                ))}
                {transfers.length === 0 && <div className="empty">{t('transfers.none')}</div>}
              </div>
            </section>
          </div>
        )}

        {menu && showTools && (
          <>
            <button type="button" className="scrim" aria-label={t('toolbar.closeMenu')} onClick={closeMenu} />
            <div ref={menuRef} role="menu" aria-label={t('toolbar.moreActions')} className="glass-strong menu" onKeyDown={onMenuKey}>
              <button type="button" role="menuitem" className="mi" onClick={sendClip}>
                <span>{t('actions.sendClipboard')}</span>
                <span className="sc">{sc.clip}</span>
              </button>
              <button type="button" role="menuitem" className="mi" onClick={sendFile}>
                <span>{t('actions.sendFile')}</span>
                <span className="sc">{sc.file}</span>
              </button>
              <button type="button" role="menuitem" className="mi" onClick={ring}>
                <span>{t('menu.findDevice')}</span>
                <span className="sc">{sc.ring}</span>
              </button>
              <div className="menu-sep" />
              <button
                type="button"
                role="menuitem"
                className="mi"
                onClick={() => {
                  setTab('device');
                  go('settings');
                }}
              >
                {t('menu.deviceSettings')}
              </button>
              <button type="button" role="menuitem" className="mi" onClick={() => go('troubleshoot')}>
                {t('menu.troubleshoot')}
              </button>
              <button
                type="button"
                role="menuitem"
                className="mi danger"
                onClick={() => {
                  setMenu(false);
                  setUnpairTarget(cur ?? null);
                }}
              >
                {t('menu.unpair')}
              </button>
            </div>
          </>
        )}

        <Toast message={toast} />
      </main>

      <PairingSheet />
      {isWindows && <WindowControls />}
      {unpairTarget && <UnpairDialog device={unpairTarget} onConfirm={() => void confirmUnpair()} onCancel={() => setUnpairTarget(null)} />}
    </div>
  );
}
