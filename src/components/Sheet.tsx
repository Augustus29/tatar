import { useEffect, useRef, type KeyboardEvent, type ReactNode } from 'react';

interface SheetProps {
  label: string;
  /** 'sheet': araç çubuğundan inen sayfa · 'alert': ortada küçük uyarı kutusu */
  variant?: 'sheet' | 'alert';
  /** Esc veya dışarı tıklama */
  onClose: () => void;
  children: ReactNode;
}

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Pencere içi modal: karartma + cam sayfa. Odağı içeride tutar (Tab döngüsü), Esc ile kapanır,
 * kapanınca odağı önceki öğeye geri verir.
 */
export function Sheet({ label, variant = 'sheet', onClose, children }: SheetProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const first = ref.current?.querySelector<HTMLElement>('[data-autofocus]') ?? ref.current?.querySelector<HTMLElement>(FOCUSABLE);
    first?.focus();
    return () => prev?.focus?.();
  }, []);

  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key !== 'Tab') return;
    const items = Array.from(ref.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? []);
    if (items.length === 0) return;
    const firstEl = items[0];
    const lastEl = items[items.length - 1];
    if (e.shiftKey && document.activeElement === firstEl) {
      e.preventDefault();
      lastEl.focus();
    } else if (!e.shiftKey && document.activeElement === lastEl) {
      e.preventDefault();
      firstEl.focus();
    }
  };

  return (
    <>
      <button type="button" className="sheet-scrim" tabIndex={-1} aria-hidden="true" onClick={onClose} />
      <div
        ref={ref}
        role={variant === 'alert' ? 'alertdialog' : 'dialog'}
        aria-modal="true"
        aria-label={label}
        className={`glass-strong sheet${variant === 'alert' ? ' alert' : ''}`}
        onKeyDown={onKey}
      >
        {children}
      </div>
    </>
  );
}
