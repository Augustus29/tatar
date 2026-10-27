import React from 'react';
import ReactDOM from 'react-dom/client';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { I18nProvider } from './i18n';
import { useTheme } from './lib/theme';
import { inTauri } from './lib/platform';
import { MainWindow } from './screens/MainWindow';
import { Popover } from './screens/Popover';
import './styles/base.css';

// Aynı arayüz paketi iki pencerede çalışır: ana pencere ve menü çubuğu popover'ı.
// Tarayıcıda önizlemek için ?window=popover kullanılabilir.
const label = inTauri ? getCurrentWindow().label : (new URLSearchParams(window.location.search).get('window') ?? 'main');
const isPopover = label === 'popover';
if (isPopover) document.body.classList.add('popover-window');
// Tarayıcı önizlemesinde masaüstünü taklit eden bir zemin (gerçek uygulamada yerel cam kullanılır).
if (isPopover && !inTauri) document.body.style.background = 'linear-gradient(160deg,#e8dfd3,#cdbcaa 60%,#7f6b5e)';

function Root() {
  useTheme();
  return isPopover ? <Popover /> : <MainWindow />;
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <I18nProvider>
      <Root />
    </I18nProvider>
  </React.StrictMode>,
);
