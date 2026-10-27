import type { ReactNode, SVGProps } from 'react';

interface IconProps extends Omit<SVGProps<SVGSVGElement>, 'children'> {
  size?: number;
  strokeWidth?: number;
}

function make(draw: ReactNode, defaults: { fill?: string; strokeWidth: number; linejoin?: boolean }) {
  return function Icon({ size = 18, strokeWidth = defaults.strokeWidth, ...rest }: IconProps) {
    return (
      <svg
        aria-hidden="true"
        viewBox="0 0 24 24"
        width={size}
        height={size}
        fill={defaults.fill ?? 'none'}
        stroke="currentColor"
        strokeWidth={strokeWidth}
        strokeLinecap="round"
        strokeLinejoin={defaults.linejoin === false ? undefined : 'round'}
        {...rest}
      >
        {draw}
      </svg>
    );
  };
}

export const TabletIcon = make(
  <>
    <rect x="3" y="5" width="18" height="14" rx="2.5" />
    <path d="M18 12h.01" />
  </>,
  { strokeWidth: 1.7 },
);
export const PhoneIcon = make(
  <>
    <rect x="7" y="2.5" width="10" height="19" rx="2.5" />
    <path d="M11 18.5h2" />
  </>,
  { strokeWidth: 1.7 },
);
export const PingIcon = make(
  <>
    <circle cx="12" cy="12" r="1.8" fill="currentColor" />
    <path d="M8.6 8.6a4.8 4.8 0 0 0 0 6.8M15.4 8.6a4.8 4.8 0 0 1 0 6.8M5.6 5.6a9 9 0 0 0 0 12.8M18.4 5.6a9 9 0 0 1 0 12.8" />
  </>,
  { strokeWidth: 1.8, linejoin: false },
);
export const ClipboardIcon = make(
  <>
    <rect x="5.5" y="4.5" width="13" height="16.5" rx="2" />
    <rect x="9" y="2.5" width="6" height="3.5" rx="1" />
    <path d="M9 11h6M9 15h4" />
  </>,
  { strokeWidth: 1.8 },
);
export const ClipboardPlainIcon = make(
  <>
    <rect x="5.5" y="4.5" width="13" height="16.5" rx="2" />
    <rect x="9" y="2.5" width="6" height="3.5" rx="1" />
  </>,
  { strokeWidth: 1.9 },
);
export const FileUploadIcon = make(
  <>
    <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
    <path d="M14 3v5h5M12 17v-6M9.5 13.5 12 11l2.5 2.5" />
  </>,
  { strokeWidth: 1.8 },
);
export const DotsIcon = ({ size = 18, ...rest }: IconProps) => (
  <svg aria-hidden="true" viewBox="0 0 24 24" width={size} height={size} fill="currentColor" {...rest}>
    <circle cx="6" cy="12" r="1.6" />
    <circle cx="12" cy="12" r="1.6" />
    <circle cx="18" cy="12" r="1.6" />
  </svg>
);
export const SlidersIcon = make(
  <>
    <path d="M4 7h9M17 7h3M4 17h4M12 17h8" />
    <circle cx="15" cy="7" r="2" />
    <circle cx="10" cy="17" r="2" />
  </>,
  { strokeWidth: 1.7, linejoin: false },
);
export const SpinnerIcon = make(<path d="M12 3a9 9 0 1 0 9 9" />, { strokeWidth: 2.4, linejoin: false });
export const CheckIcon = make(<path d="M5 12.5l4.5 4.5L19 7.5" />, { strokeWidth: 2.2 });
export const ArrowUpIcon = make(<path d="M12 19V5M6 11l6-6 6 6" />, { strokeWidth: 1.9 });
export const ArrowDownIcon = make(<path d="M12 5v14M6 13l6 6 6-6" />, { strokeWidth: 1.9 });
export const SearchIcon = make(
  <>
    <circle cx="11" cy="11" r="6" />
    <path d="M20 20l-4.5-4.5" />
  </>,
  { strokeWidth: 1.8, linejoin: false },
);
export const SpeakerOffIcon = make(
  <>
    <path d="M11 5 6 9H3v6h3l5 4z" />
    <path d="M16 9.5l5 5M21 9.5l-5 5" />
  </>,
  { strokeWidth: 1.9 },
);
export const TextIcon = make(<path d="M5 7V5h14v2M12 5v14M9 19h6" />, { strokeWidth: 1.9 });
export const LinkIcon = make(
  <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />,
  { strokeWidth: 1.9 },
);
export const LockIcon = make(
  <>
    <rect x="5" y="11" width="14" height="9" rx="2" />
    <path d="M8 11V8a4 4 0 0 1 8 0v3" />
  </>,
  { strokeWidth: 1.9 },
);
export const CopyIcon = make(
  <>
    <rect x="9" y="9" width="11" height="11" rx="2.5" />
    <path d="M5 15V6.5A2.5 2.5 0 0 1 7.5 4H15" />
  </>,
  { strokeWidth: 1.9 },
);
export const SendToPhoneIcon = make(
  <>
    <rect x="7" y="2.5" width="10" height="19" rx="2.5" />
    <path d="M12 15V9M9.5 11.5 12 9l2.5 2.5" />
  </>,
  { strokeWidth: 1.9 },
);
export const DropIcon = make(<path d="M12 15V4M8 8l4-4 4 4M5 14v4a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-4" />, {
  strokeWidth: 1.7,
});
export const BackIcon = make(<path d="M15 5l-7 7 7 7" />, { strokeWidth: 1.9 });

export const CloseIcon = make(<path d="M6 6l12 12M18 6 6 18" />, { strokeWidth: 1.9 });
export const OpenIcon = make(<path d="M14 4h6v6M20 4l-9 9M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4" />, {
  strokeWidth: 1.8,
});
export const WifiIcon = make(
  <>
    <path d="M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0M8.5 16a5 5 0 0 1 7 0" />
    <circle cx="12" cy="19.2" r="1" fill="currentColor" />
  </>,
  { strokeWidth: 2, linejoin: false },
);
export const InfoMarkIcon = make(<path d="M12 11v6M12 7.5h.01" />, { strokeWidth: 2.4, linejoin: false });
export const InfoCircleIcon = make(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 11v6M12 7.5h.01" />
  </>,
  { strokeWidth: 2, linejoin: false },
);
export const ExclaimIcon = make(<path d="M12 6v7M12 18h.01" />, { strokeWidth: 3, linejoin: false });
export const KeysIcon = make(
  <path d="M9 9V6a3 3 0 1 0-3 3h3zm0 0h6m-6 0v6m6-6V6a3 3 0 1 1 3 3h-3zm0 0v6m0 0h3a3 3 0 1 1-3 3v-3zm0 0H9m0 0v3a3 3 0 1 1-3-3h3z" />,
  { strokeWidth: 2 },
);
export const TabletOutlineIcon = make(<rect x="3" y="5" width="18" height="14" rx="2.5" />, { strokeWidth: 2.2 });
export const LaptopIcon = make(
  <>
    <rect x="4.5" y="5" width="15" height="11" rx="2" />
    <path d="M2.5 19.5h19" />
  </>,
  { strokeWidth: 1.7 },
);
export const DesktopIcon = make(
  <>
    <rect x="3" y="4" width="18" height="12" rx="2" />
    <path d="M8 20h8M12 16v4" />
  </>,
  { strokeWidth: 1.7 },
);

/** Cihaz türüne göre ikon. */
export function DeviceIcon({ type, size, strokeWidth }: { type: string; size?: number; strokeWidth?: number }) {
  const p = { size, strokeWidth };
  if (type === 'phone') return <PhoneIcon {...p} />;
  if (type === 'laptop') return <LaptopIcon {...p} />;
  if (type === 'desktop' || type === 'tv') return <DesktopIcon {...p} />;
  return <TabletIcon {...p} />;
}

/** Açılır menü düğmesindeki turkuaz çift ok. */
export function ChevronsIcon() {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 10 12"
      width="10"
      height="12"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M2 4.6L5 2l3 2.6" />
      <path d="M2 7.4L5 10l3-2.6" />
    </svg>
  );
}

/** Menü çubuğu ve marka için atlı kurye silueti (kaynak: Icon.dc.html). */
export function RiderGlyph({ size = 20, color = 'currentColor' }: { size?: number; color?: string }) {
  return (
    <svg aria-hidden="true" viewBox="0 0 100 100" width={size} height={size} style={{ position: 'relative' }}>
      <g fill={color} stroke={color} strokeLinecap="round" strokeLinejoin="round">
        <ellipse cx="48" cy="57" rx="21" ry="9.5" transform="rotate(-8 48 57)" strokeWidth="0" />
        <path d="M60 52 L69 36 L76 37 L71 57 Z" strokeWidth="2" />
        <path d="M69 34 L81 30 L88 37 L85 41 L76 40 Z" strokeWidth="2" />
        <path d="M63 60 L75 66 L86 61" fill="none" strokeWidth="6" />
        <path d="M58 63 L67 74 L76 77" fill="none" strokeWidth="6" />
        <path d="M35 61 L23 68 L12 65" fill="none" strokeWidth="6" />
        <path d="M39 63 L31 75 L23 81" fill="none" strokeWidth="6" />
        <path d="M29 53 Q17 47 9 57" fill="none" strokeWidth="5" />
        <path d="M45 48 L51 31 L58 33 L55 50 Z" strokeWidth="2" />
        <circle cx="57.5" cy="25.5" r="5" strokeWidth="0" />
        <path d="M54 37 L67 42" fill="none" strokeWidth="4" />
      </g>
    </svg>
  );
}
