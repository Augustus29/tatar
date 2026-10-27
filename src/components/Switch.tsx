interface SwitchProps {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
}

/** Yaylanarak esneyen açma/kapama anahtarı (42×24). */
export function Switch({ checked, onChange, label }: SwitchProps) {
  return (
    <button type="button" role="switch" aria-checked={checked} aria-label={label} className="sw" onClick={() => onChange(!checked)}>
      <span className="knob" />
    </button>
  );
}
