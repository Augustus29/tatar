import { CheckIcon } from './icons';

/** Alttan yaylanarak çıkan kısa bildirim (cam kapsül). */
export function Toast({ message }: { message: string }) {
  if (!message) return null;
  return (
    <div role="status" className="glass-strong toast">
      <CheckIcon size={16} style={{ color: 'var(--accent)' }} />
      <span>{message}</span>
    </div>
  );
}
