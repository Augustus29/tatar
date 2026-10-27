import { useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';
import { ChevronsIcon } from './icons';

interface DropdownProps<T extends string> {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  label: string;
}

const ROW = 24;

/**
 * Özel macOS tarzı açılır menü: seçili öğe düğmenin üstüne hizalanır, yanında tik olur.
 * Tarayıcının yerel <select> menüsü kullanılmaz.
 */
export function Dropdown<T extends string>({ value, options, onChange, label }: DropdownProps<T>) {
  const [open, setOpen] = useState(false);
  const [focusIdx, setFocusIdx] = useState(0);
  const btnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const id = useId();
  const idx = Math.max(
    0,
    options.findIndex((o) => o.value === value),
  );
  const current = options[idx];

  useEffect(() => {
    if (open) menuRef.current?.querySelectorAll<HTMLButtonElement>('[role=option]')[focusIdx]?.focus();
  }, [open, focusIdx]);

  const close = (refocus = true) => {
    setOpen(false);
    if (refocus) btnRef.current?.focus();
  };

  const onMenuKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      setFocusIdx((i) => Math.min(options.length - 1, i + 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setFocusIdx((i) => Math.max(0, i - 1));
    } else if (e.key === 'Tab') {
      close(false);
    }
  };

  return (
    <span style={{ position: 'relative', display: 'inline-flex' }}>
      <button
        ref={btnRef}
        type="button"
        className="pop btn"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`${label}: ${current.label}`}
        aria-controls={open ? id : undefined}
        onClick={() => {
          setFocusIdx(idx);
          setOpen((o) => !o);
        }}
      >
        <span>{current.label}</span>
        <span className="chev" aria-hidden="true">
          <ChevronsIcon />
        </span>
      </button>
      {open && (
        <>
          <button
            type="button"
            tabIndex={-1}
            aria-hidden="true"
            onClick={() => close(false)}
            style={{ position: 'fixed', inset: 0, zIndex: 25, border: 0, padding: 0, background: 'transparent' }}
          />
          <div
            ref={menuRef}
            id={id}
            className="glass-strong popmenu"
            role="listbox"
            aria-label={label}
            onKeyDown={onMenuKey}
            style={{ top: -3 - idx * ROW, transformOrigin: `50% ${17 + idx * ROW}px` }}
          >
            {options.map((o, i) => (
              <button
                key={o.value}
                type="button"
                role="option"
                aria-selected={o.value === value}
                className="mi-opt"
                tabIndex={i === focusIdx ? 0 : -1}
                onClick={() => {
                  onChange(o.value);
                  close();
                }}
              >
                {o.value === value && (
                  <svg
                    aria-hidden="true"
                    viewBox="0 0 12 12"
                    width="12"
                    height="12"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.8"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <path d="M2 6.4L4.8 9L10 3" />
                  </svg>
                )}
                <span>{o.label}</span>
              </button>
            ))}
          </div>
        </>
      )}
    </span>
  );
}
