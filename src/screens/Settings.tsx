import { useState } from 'react';
import { useI18n, type LangPref } from '../i18n';
import { Dropdown } from '../components/Dropdown';
import { Segmented } from '../components/Segmented';
import { Switch } from '../components/Switch';

type ClipMode = 'auto' | 'manual' | 'off';
type CallAction = 'mute' | 'lower' | 'nothing';

/**
 * Aşama 1: yalnızca "Genel" bölümünün iskeleti (kontrolleri göstermek için).
 * Ayarların tamamı Aşama 7'de eklenecek.
 */
export function SettingsPane({ clipOn }: { clipOn: boolean }) {
  const { t, pref, setPref } = useI18n();
  const [mode, setMode] = useState<ClipMode>(clipOn ? 'auto' : 'off');
  const [call, setCall] = useState<CallAction>('mute');
  const [login, setLogin] = useState(false);
  const [menubar, setMenubar] = useState(true);
  const [dock, setDock] = useState(true);

  return (
    <div className="scroll">
      <section className="pane">
        <div className="card">
          <div className="setrow">
            <span className="lbl">{t('settings.language')}</span>
            <Dropdown<LangPref>
              label={t('settings.language')}
              value={pref}
              onChange={setPref}
              options={[
                { value: 'system', label: t('settings.langSystem') },
                { value: 'tr', label: t('settings.langTr') },
                { value: 'en', label: t('settings.langEn') },
              ]}
            />
          </div>
          <div className="setrow">
            <span className="lbl">{t('settings.clipboardMode')}</span>
            <Segmented<ClipMode>
              label={t('settings.clipboardMode')}
              value={mode}
              onChange={setMode}
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
              onChange={setCall}
              options={[
                { value: 'mute', label: t('settings.callMute') },
                { value: 'lower', label: t('settings.callLower') },
                { value: 'nothing', label: t('settings.callNothing') },
              ]}
            />
          </div>
        </div>
        <div className="card">
          <div className="setrow">
            <span className="lbl">{t('settings.launchAtLogin')}</span>
            <Switch checked={login} onChange={setLogin} label={t('settings.launchAtLogin')} />
          </div>
          <div className="setrow">
            <span className="lbl">{t('settings.showInMenuBar')}</span>
            <Switch checked={menubar} onChange={setMenubar} label={t('settings.showInMenuBar')} />
          </div>
          <div className="setrow">
            <span className="lbl">{t('settings.showInDock')}</span>
            <Switch checked={dock} onChange={setDock} label={t('settings.showInDock')} />
          </div>
        </div>
        <span className="note">{t('settings.stageNote')}</span>
      </section>
    </div>
  );
}
