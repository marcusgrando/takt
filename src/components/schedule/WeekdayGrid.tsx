// src/components/schedule/WeekdayGrid.tsx
import { Button } from '@/components/ui/button';

const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'] as const;

interface WeekdayGridProps {
  selected: number[]; // 0=Sun, ..., 6=Sat
  onChange: (days: number[]) => void;
}

export default function WeekdayGrid({ selected, onChange }: WeekdayGridProps) {
  function toggle(day: number) {
    if (selected.includes(day)) {
      const next = selected.filter((d) => d !== day);
      if (next.length > 0) onChange(next); // keep at least 1
    } else {
      onChange([...selected, day]);
    }
  }

  return (
    <div className="flex gap-1">
      {DAYS.map((label, i) => (
        <Button
          key={i}
          type="button"
          variant={selected.includes(i) ? 'default' : 'outline'}
          size="sm"
          onClick={() => toggle(i)}
          className="h-8 flex-1 p-0 text-xs font-medium"
        >
          {label}
        </Button>
      ))}
    </div>
  );
}
