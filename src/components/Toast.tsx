import { CheckIcon } from './icons';

/** Alttan yaylanarak çıkan kısa bildirim (cam kapsül). Canlı bölge her zaman DOM'da kalır ki ekran okuyucu yeni metni okusun. */
export function Toast({ message }: { message: string }) {
  return (
    <div role="status" aria-live="polite" aria-atomic="true" style={{ display: 'contents' }}>
      {message && (
        <div className="glass-strong toast">
          <CheckIcon size={16} style={{ color: 'var(--accent)' }} />
          <span>{message}</span>
        </div>
      )}
    </div>
  );
}
