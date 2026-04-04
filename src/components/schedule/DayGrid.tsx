// src/components/schedule/DayGrid.tsx
import { Button } from '@/components/ui/button';

interface DayGridProps {
  selected: number[];
  onChange: (days: number[]) => void;
}

export default function DayGrid({ selected, onChange }: DayGridProps) {
  function toggle(day: number) {
    if (selected.includes(day)) {
      const next = selected.filter((d) => d !== day);
      if (next.length > 0) onChange(next); // keep at least 1
    } else {
      onChange([...selected, day]);
    }
  }

  return (
    <div className="grid grid-cols-7 gap-1">
      {Array.from({ length: 31 }, (_, i) => i + 1).map((day) => (
        <Button
          key={day}
          type="button"
          variant={selected.includes(day) ? 'default' : 'outline'}
          size="sm"
          onClick={() => toggle(day)}
          className="h-7 w-7 p-0 text-xs font-medium"
        >
          {day}
        </Button>
      ))}
    </div>
  );
}
