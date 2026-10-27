import { useRef, type KeyboardEvent } from 'react';

interface SegmentedProps<T extends string> {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  label: string;
}

/** Kayan beyaz topuzlu segment denetimi (radiogroup). */
export function Segmented<T extends string>({ value, options, onChange, label }: SegmentedProps<T>) {
  const ref = useRef<HTMLDivElement>(null);
  const idx = Math.max(
    0,
    options.findIndex((o) => o.value === value),
  );

  const onKey = (e: KeyboardEvent) => {
    const d = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const next = options[(idx + d + options.length) % options.length];
    onChange(next.value);
    ref.current?.querySelectorAll<HTMLButtonElement>('button')[(idx + d + options.length) % options.length]?.focus();
  };

  return (
    <div ref={ref} role="radiogroup" aria-label={label} className="seg" onKeyDown={onKey}>
      <span className="thumb" style={{ transform: `translateX(${idx * 76}px)` }} />
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          tabIndex={o.value === value ? 0 : -1}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
