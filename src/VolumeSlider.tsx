import { useId } from 'react';

export default function VolumeSlider({ value, disabled, label, onChange }: {
  value: number | null;
  disabled?: boolean;
  label: string;
  onChange: (value: number) => void;
}) {
  const tooltipId = useId();
  const percent = Math.round((value ?? 0) * 100);
  return <div className="volume-slider">
    <input aria-label={label} aria-describedby={tooltipId} type="range" min="0" max="1" step="0.01" disabled={disabled} value={value ?? 0} onChange={e => onChange(Number(e.target.value))} />
    <div id={tooltipId} role="tooltip" className="volume-tooltip" style={{ left: `clamp(22px, ${percent}%, calc(100% - 22px))` }}>{value === null ? '等待手机回传音量' : `${percent}`}</div>
  </div>;
}
