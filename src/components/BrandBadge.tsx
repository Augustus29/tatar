/** Marka rozeti: turkuaz zemin üzerinde atlı kurye (kaynak: Pairing/Onboarding/Settings prototipleri). */
export function BrandBadge({ size = 48, glyph, radius, glow = false }: { size?: number; glyph?: number; radius?: number; glow?: boolean }) {
  const g = glyph ?? Math.round(size * 0.71);
  const r = radius ?? Math.round(size * 0.23);
  return (
    <span
      aria-hidden="true"
      style={{
        position: 'relative',
        width: size,
        height: size,
        borderRadius: r,
        flex: 'none',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'linear-gradient(180deg,#127a86,#0b5f69 55%,#08474f)',
        boxShadow: `inset 0 1px 1px rgba(255,255,255,.45), inset 0 -2px 6px rgba(0,0,0,.25)${glow ? ', 0 8px 22px rgba(11,95,105,0.35)' : ''}`,
      }}
    >
      {glow && (
        <span
          style={{
            position: 'absolute',
            inset: 0,
            borderRadius: r,
            background: 'linear-gradient(160deg,rgba(255,255,255,0.28) 0%,rgba(255,255,255,0) 45%)',
          }}
        />
      )}
      <svg
        viewBox="0 0 100 100"
        width={g}
        height={g}
        style={{ position: 'relative', filter: 'drop-shadow(0 2px 3px rgba(0,0,0,.25)) drop-shadow(0 -0.6px 0 rgba(255,255,255,.6))' }}
      >
        <g fill="rgba(244,235,221,0.92)" stroke="rgba(244,235,221,0.92)" strokeLinecap="round" strokeLinejoin="round">
          <ellipse cx="48" cy="57" rx="21" ry="9.5" transform="rotate(-8 48 57)" strokeWidth="0" />
          <path d="M60 52 L69 36 L76 37 L71 57 Z" strokeWidth="2" />
          <path d="M69 34 L81 30 L88 37 L85 41 L76 40 Z" strokeWidth="2" />
          <path d="M63 60 L75 66 L86 61" fill="none" strokeWidth="5" />
          <path d="M58 63 L67 74 L76 77" fill="none" strokeWidth="5" />
          <path d="M35 61 L23 68 L12 65" fill="none" strokeWidth="5" />
          <path d="M39 63 L31 75 L23 81" fill="none" strokeWidth="5" />
          <path d="M29 53 Q17 47 9 57" fill="none" strokeWidth="4.5" />
          <path d="M45 48 L51 31 L58 33 L55 50 Z" strokeWidth="2" />
          <circle cx="57.5" cy="25.5" r="4.8" strokeWidth="0" />
          <path d="M54 37 L67 42" fill="none" strokeWidth="3.5" />
        </g>
        <path d="M51 32 Q38 26 28 33 Q38 33 47 40 Z" fill="#C8442F" />
      </svg>
    </span>
  );
}
