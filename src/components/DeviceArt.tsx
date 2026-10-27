import type { CSSProperties } from 'react';

interface DeviceArtProps {
  type: string;
  label: string;
  width?: number;
  height?: number;
  /** Soluk (bağlı değil) görünüm */
  dim?: boolean;
  /** Bağlı (ekran dalgalı turkuaz) çizim */
  live?: boolean;
  style?: CSSProperties;
}

/** Ana ekrandaki büyük cihaz çizimi: telefon ya da tablet (kaynak: Main.dc.html). */
export function DeviceArt({ type, label, width = 140, height = 100, dim = false, live = true, style }: DeviceArtProps) {
  const phone = type === 'phone';
  return (
    <svg
      role="img"
      aria-label={label}
      viewBox="0 0 140 100"
      width={width}
      height={height}
      style={{ opacity: dim ? 0.5 : 1, flex: 'none', ...style }}
    >
      {phone ? (
        <>
          <rect x="44" y="2" width="52" height="96" rx="11" style={{ fill: 'var(--devFrame)' }} />
          <rect x="49" y="7" width="42" height="86" rx="7" style={{ fill: 'var(--devScreen)' }} />
        </>
      ) : (
        <>
          <rect x="4" y="6" width="132" height="88" rx="12" style={{ fill: 'var(--devFrame)' }} />
          <rect x="10" y="12" width="120" height="76" rx="7" style={{ fill: 'var(--devScreen)' }} />
          {live && (
            <>
              <path
                d="M10 70 Q50 56 90 64 T130 60 V81 a7 7 0 0 1 -7 7 H17 a7 7 0 0 1 -7 -7 Z"
                style={{ fill: 'var(--accent)', opacity: 0.35 }}
              />
              <circle cx="102" cy="34" r="9" style={{ fill: 'var(--accent)', opacity: 0.5 }} />
            </>
          )}
        </>
      )}
    </svg>
  );
}
