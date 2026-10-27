import { useEffect, useState, type ReactNode } from 'react';
import { useI18n, type LangPref } from '../i18n';
import { api, DEFAULT_SHORTCUTS, SHORTCUT_IDS, type ClipboardMode, type Device, type ShortcutId } from '../lib/api';
import { actions, useStore } from '../lib/store';
import { formatShortcut, isMac } from '../lib/platform';
import { keysFromEvent, prettyPath } from '../lib/format';
import { Dropdown } from '../components/Dropdown';
import { Segmented } from '../components/Segmented';
import { Switch } from '../components/Switch';
import { BrandBadge } from '../components/BrandBadge';
import { BackIcon, DeviceIcon, InfoCircleIcon, InfoMarkIcon, KeysIcon, SlidersIcon, TabletOutlineIcon } from '../components/icons';

export type SettingsTab = 'general' | 'device' | 'shortcuts' | 'about';
type CallAction = 'mute' | 'lower' | 'nothing';

const TAB_COLORS: Record<SettingsTab, string> = { general: '#8E8E93', device: '#0A7E8C', shortcuts: '#6E5BD6', about: '#8E8E93' };
const SHORTCUT_LABEL: Record<ShortcutId, string> = {
  sendClip: 'settings.scSendClip',
  sendFile: 'settings.scSendFile',
  findDev: 'settings.scFindDev',
  openWin: 'settings.scOpenWin',
  showHist: 'settings.scShowHist',
};

/** Ayarlar açıkken kenar çubuğunda görünen bölüm listesi. */
export function SettingsNav({
  tab,
  onTab,
  onBack,
  deviceName,
}: {
  tab: SettingsTab;
  onTab: (t: SettingsTab) => void;
  onBack: () => void;
  deviceName: string | null;
}) {
  const { t } = useI18n();
  const tabs: { id: SettingsTab; label: string; icon: ReactNode }[] = [
    { id: 'general', label: t('settings.title'), icon: <SlidersIcon size={14} strokeWidth={2.2} /> },
    { id: 'device', label: deviceName ?? t('settings.titleDevice'), icon: <TabletOutlineIcon size={14} /> },
    { id: 'shortcuts', label: t('settings.tabShortcuts'), icon: <KeysIcon size={14} /> },
    { id: 'about', label: t('settings.tabAbout'), icon: <InfoMarkIcon size={14} /> },
  ];
  return (
    <nav aria-label={t('settings.navAria')} style={{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1 }}>
      <button type="button" className="nav-item" onClick={onBack} style={{ marginBottom: 10 }}>
        <span className="nico" style={{ background: 'transparent', color: 'var(--text2)', boxShadow: 'none' }}>
          <BackIcon size={16} />
        </span>
        <span className="nname">{t('settings.back')}</span>
      </button>
      {tabs.map((tb) => (
        <button
          key={tb.id}
          type="button"
          className="nav-item"
          aria-current={tab === tb.id ? 'page' : undefined}
          onClick={() => onTab(tb.id)}
        >
          <span className="nico" style={{ background: TAB_COLORS[tb.id] }}>
            {tb.icon}
          </span>
          <span className="nname">{tb.label}</span>
        </button>
      ))}
    </nav>
  );
}

interface PaneProps {
  tab: SettingsTab;
  /** "Cihaz" bölümünde ayarlanan cihaz */
  device: Device | undefined;
  onUnpair: (d: Device) => void;
  onTroubleshoot: () => void;
}

export function SettingsPane({ tab, device, onUnpair, onTroubleshoot }: PaneProps) {
  return (
    <div className="scroll">
      {tab === 'general' && <GeneralTab onTroubleshoot={onTroubleshoot} />}
      {tab === 'device' && <DeviceTab device={device} onUnpair={onUnpair} />}
      {tab === 'shortcuts' && <ShortcutsTab />}
      {tab === 'about' && <AboutTab />}
    </div>
  );
}

/* ---------- Genel ---------- */

function GeneralTab({ onTroubleshoot }: { onTroubleshoot: () => void }) {
  const { t, pref, setPref } = useI18n();
  const s = useStore((x) => x.settings);
  const pickDir = async () => {
    const dir = await api.pickFolder();
    if (dir) void actions.updateSettings({ downloadDir: dir });
  };
  const sw = (label: string, checked: boolean, onChange: (v: boolean) => void) => (
    <div className="setrow" key={label}>
      <span className="lbl">{label}</span>
      <Switch checked={checked} onChange={onChange} label={label} />
    </div>
  );

  return (
    <section className="pane gap18">
      <div className="card">
        {sw(t('settings.launchAtLogin'), s.launchAtLogin, (v) => void actions.updateSettings({ launchAtLogin: v }))}
        {sw(t('settings.showInMenuBar'), s.showInMenuBar, (v) => void actions.updateSettings({ showInMenuBar: v }))}
        {isMac && sw(t('settings.showInDock'), s.showInDock, (v) => void actions.updateSettings({ showInDock: v }))}
      </div>

      <div className="card">
        <div className="setrow">
          <span className="lbl">{t('settings.language')}</span>
          <Dropdown<LangPref>
            label={t('settings.language')}
            value={pref}
            onChange={(v) => {
              setPref(v);
              void actions.updateSettings({ language: v });
            }}
            options={[
              { value: 'system', label: t('settings.langSystem') },
              { value: 'tr', label: t('settings.langTr') },
              { value: 'en', label: t('settings.langEn') },
            ]}
          />
        </div>
        <div className="setrow">
          <label htmlFor="devname">{t('settings.computerName')}</label>
          <NameField key={s.deviceName} value={s.deviceName} />
        </div>
        <div className="setrow">
          <span className="stack">
            <span>{t('settings.incomingFiles')}</span>
            <span className="sub">{prettyPath(s.downloadDir)}</span>
          </span>
          <button type="button" className="btn btn-base lg glass sm btn-sec" onClick={() => void pickDir()}>
            {t('settings.change')}
          </button>
        </div>
      </div>

      <div className="group">
        <span className="group-label">{t('settings.notifications')}</span>
        <div className="card">
          {sw(
            t('settings.notifyFile'),
            s.notifications.fileReceived,
            (v) => void actions.updateSettings({ notifications: { ...s.notifications, fileReceived: v } }),
          )}
          {sw(
            t('settings.notifyBattery'),
            s.notifications.batteryLow,
            (v) => void actions.updateSettings({ notifications: { ...s.notifications, batteryLow: v } }),
          )}
        </div>
      </div>

      <div className="card">
        <div className="setrow">
          <span className="stack">
            <span>{t('settings.troubleshootRow')}</span>
            <span className="sub">{t('settings.troubleshootSub')}</span>
          </span>
          <button type="button" className="btn btn-base lg glass sm btn-sec" onClick={onTroubleshoot}>
            {t('troubleshoot.open')}
          </button>
        </div>
      </div>
    </section>
  );
}

/** Bilgisayar adı: yazarken yerel, Enter/odak kaybında kaydedilir. Kayıtlı ad değişince yeniden kurulur (key). */
function NameField({ value }: { value: string }) {
  const [name, setName] = useState(value);
  const commit = () => {
    const v = name.trim();
    if (!v) setName(value);
    else if (v !== value) void actions.updateSettings({ deviceName: v });
  };
  return (
    <input
      id="devname"
      className="field"
      value={name}
      maxLength={64}
      onChange={(e) => setName(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
    />
  );
}

/* ---------- Cihaz ---------- */

function DeviceTab({ device, onUnpair }: { device: Device | undefined; onUnpair: (d: Device) => void }) {
  const { t } = useI18n();
  const s = useStore((x) => x.settings);
  const call: CallAction = !s.muteOnCall ? 'nothing' : s.muteMode;

  if (!device) {
    return (
      <section className="pane">
        <span className="note">{t('settings.noDevice')}</span>
      </section>
    );
  }
  return (
    <section className="pane gap18">
      <div className="devhead">
        <span className="ico">
          <DeviceIcon type={device.type} size={22} />
        </span>
        <span className="stack" style={{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1 }}>
          <span style={{ fontSize: 14, fontWeight: 700 }}>{device.name}</span>
          <span style={{ fontSize: 12, color: 'var(--text2)' }}>
            {device.reachable ? t('device.connectedLine', { net: t('device.wifi'), ip: device.ip ?? '' }) : t('device.notConnected')}
          </span>
        </span>
      </div>

      <div className="card">
        <div className="setrow">
          <span className="lbl">{t('settings.clipboardMode')}</span>
          <Segmented<ClipboardMode>
            label={t('settings.clipboardMode')}
            value={s.clipboardMode}
            onChange={(v) => void actions.updateSettings({ clipboardMode: v })}
            options={[
              { value: 'auto', label: t('settings.modeAuto') },
              { value: 'manual', label: t('settings.modeManual') },
              { value: 'off', label: t('settings.modeOff') },
            ]}
          />
        </div>
        <div className="setrow">
          <span className="lbl">{t('settings.onCall')}</span>
          <Dropdown<CallAction>
            label={t('settings.onCall')}
            value={call}
            onChange={(v) => void actions.updateSettings(v === 'nothing' ? { muteOnCall: false } : { muteOnCall: true, muteMode: v })}
            options={[
              { value: 'mute', label: t('settings.callMute') },
              { value: 'lower', label: t('settings.callLower') },
              { value: 'nothing', label: t('settings.callNothing') },
            ]}
          />
        </div>
        <div className="setrow">
          <span className="stack">
            <span>{t('settings.shareBattery')}</span>
            <span className="sub">{t('settings.shareBatterySub')}</span>
          </span>
          <Switch
            checked={s.shareBattery}
            onChange={(v) => void actions.updateSettings({ shareBattery: v })}
            label={t('settings.shareBattery')}
          />
        </div>
        <div className="setrow">
          <span className="stack">
            <span>{t('settings.autoConnect')}</span>
            <span className="sub">{t('settings.autoConnectSub')}</span>
          </span>
          <Switch
            checked={s.autoConnect}
            onChange={(v) => void actions.updateSettings({ autoConnect: v })}
            label={t('settings.autoConnect')}
          />
        </div>
      </div>

      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <button type="button" className="btn btn-base lg btn-ghost-danger" onClick={() => onUnpair(device)}>
          {t('settings.unpair')}
        </button>
      </div>
    </section>
  );
}

/* ---------- Kısayollar ---------- */

const caps = (keys: string[]): string[] => (isMac ? keys : formatShortcut(keys).split('+'));

function ShortcutsTab() {
  const { t } = useI18n();
  const shortcuts = useStore((x) => x.settings.shortcuts);
  const [rec, setRec] = useState<ShortcutId | null>(null);
  const [err, setErr] = useState('');

  // Kayıt sırasında tuşları yakala: Esc iptal eder, değiştirici + tuş kısayol olur.
  useEffect(() => {
    if (!rec) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        setRec(null);
        return;
      }
      if (e.metaKey || e.ctrlKey || e.altKey) e.preventDefault();
      if (['Shift', 'Control', 'Alt', 'Meta'].includes(e.key)) return;
      const keys = keysFromEvent(e, isMac);
      if (!keys) return;
      e.stopPropagation();
      const joined = keys.join('');
      const clash = SHORTCUT_IDS.find((id) => id !== rec && shortcuts[id].join('') === joined);
      if (clash) {
        setErr(t('settings.shortcutTaken'));
        return;
      }
      const id = rec;
      setRec(null);
      void actions.setShortcut(id, keys).then((ok) => setErr(ok ? '' : t('settings.shortcutFailed')));
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [rec, shortcuts, t]);

  const reset = () => {
    setRec(null);
    setErr('');
    for (const id of SHORTCUT_IDS) {
      if (shortcuts[id].join('') !== DEFAULT_SHORTCUTS[id].join('')) void actions.setShortcut(id, DEFAULT_SHORTCUTS[id]);
    }
  };

  return (
    <section className="pane gap18">
      <span className="note" style={{ fontSize: 13 }}>
        {t('settings.shortcutsHint')}
      </span>
      <div className="card">
        {SHORTCUT_IDS.map((id) => {
          const label = t(SHORTCUT_LABEL[id]);
          const recording = rec === id;
          return (
            <div className="setrow" key={id}>
              <span className="lbl">{label}</span>
              <button
                type="button"
                className="rec"
                data-rec={recording}
                aria-label={t(recording ? 'settings.pressKeys' : 'settings.shortcutRecordAria', {
                  label,
                  keys: formatShortcut(shortcuts[id]),
                })}
                onClick={() => {
                  setErr('');
                  setRec(recording ? null : id);
                }}
                onBlur={() => setRec((r) => (r === id ? null : r))}
              >
                {recording ? (
                  <span>{t('settings.pressKeys')}</span>
                ) : (
                  caps(shortcuts[id]).map((k, i) => (
                    <span className="kc" key={i}>
                      {k}
                    </span>
                  ))
                )}
              </button>
            </div>
          );
        })}
        <div className="sc-foot">
          <button type="button" className="tlink" aria-label={t('settings.resetAria')} onClick={reset}>
            {t('settings.resetShortcuts')}
          </button>
        </div>
      </div>
      {err && (
        <span className="sc-err" role="alert">
          {err}
        </span>
      )}
      <div className="sc-note">
        <InfoCircleIcon size={14} />
        <span>{t('settings.winNote')}</span>
      </div>
    </section>
  );
}

/* ---------- Hakkında ---------- */

function AboutTab() {
  const { t } = useI18n();
  const [version, setVersion] = useState('0.1.0');
  /** Gizli tanılama düğmesi: sürüm yazısına Option/Alt ile tıklayınca görünür. */
  const [diag, setDiag] = useState(false);
  const [diagMsg, setDiagMsg] = useState('');
  useEffect(() => {
    void api.appVersion().then(setVersion);
  }, []);
  const copyDiag = async () => {
    try {
      await navigator.clipboard.writeText(await api.getDiagnosticLog());
      setDiagMsg(t('settings.diagCopied'));
    } catch {
      setDiagMsg(t('settings.diagFailed'));
    }
    window.setTimeout(() => setDiagMsg(''), 2400);
  };
  return (
    <section className="about">
      <BrandBadge size={96} radius={22} glyph={76} glow />
      <span className="nm">{t('app.name')}</span>
      <span className="ver" onClick={(e) => e.altKey && setDiag(true)}>
        {t('settings.version', { version })}
      </span>
      {diag && (
        <button type="button" className="btn btn-base lg glass sm btn-sec" onClick={() => void copyDiag()}>
          {t('settings.diagCopy')}
        </button>
      )}
      {diagMsg && (
        <span role="status" style={{ fontSize: 12, color: 'var(--text2)' }}>
          {diagMsg}
        </span>
      )}
      <p>{t('settings.aboutText')}</p>
      <span className="gh">{t('settings.github')}</span>
    </section>
  );
}
