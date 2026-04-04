// src/components/schedule/TimeField.tsx
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

interface TimeFieldProps {
  hour: number;
  minute: number;
  onChange: (hour: number, minute: number) => void;
  label?: string;
}

export default function TimeField({ hour, minute, onChange, label = 'Time' }: TimeFieldProps) {
  function handleHourChange(value: string) {
    const h = parseInt(value);
    if (!isNaN(h) && h >= 0 && h <= 23) onChange(h, minute);
  }

  function handleMinuteChange(value: string) {
    const m = parseInt(value);
    if (!isNaN(m) && m >= 0 && m <= 59) onChange(hour, m);
  }

  return (
    <div className="flex items-center justify-between">
      <Label className="text-sm font-medium">{label}</Label>
      <div className="flex items-center gap-1">
        <Input
          type="number"
          min={0}
          max={23}
          value={hour.toString().padStart(2, '0')}
          onChange={(e) => handleHourChange(e.target.value)}
          className="w-12 text-center font-medium tabular-nums px-1 [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
        />
        <span className="text-sm font-medium">:</span>
        <Input
          type="number"
          min={0}
          max={59}
          value={minute.toString().padStart(2, '0')}
          onChange={(e) => handleMinuteChange(e.target.value)}
          className="w-12 text-center font-medium tabular-nums px-1 [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
        />
      </div>
    </div>
  );
}
