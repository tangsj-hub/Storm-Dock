import { Tooltip } from "../../components/Tooltip";
import type { CursorUsageDetails, UsageEvent } from "../../lib/types";
import { eventSpend, formatSpend, localDateKey } from "./format";
import { heat, weekHeat } from "./heat";
import styles from "./WeeklyChart.module.css";

const weekday = new Intl.DateTimeFormat(undefined, { weekday: "short" });

function dailySpend(days: CursorUsageDetails["weekly"], events: UsageEvent[]) {
  const totals = new Map<string, { amount: number; unit: "credits" | "currency" }>();
  for (const event of events) {
    const spend = eventSpend(event);
    if (!spend) continue;
    const key = localDateKey(new Date(event.timestamp));
    const prev = totals.get(key);
    if (prev) {
      totals.set(key, { amount: prev.amount + spend.amount, unit: spend.unit });
    } else {
      totals.set(key, { ...spend });
    }
  }
  // Infer unit from events; WorkBuddy weekly fallback stores credits in onDemandCents.
  const unit: "credits" | "currency" = [...totals.values()][0]?.unit
    ?? (events.some((event) => event.credits !== undefined) ? "credits" : "currency");
  return days.map((day) => {
    const fromEvents = totals.get(day.date);
    if (fromEvents) return fromEvents;
    return { amount: day.onDemandCents, unit };
  });
}

export function WeeklyChart({ days, events }: { days: CursorUsageDetails["weekly"]; events: UsageEvent[] }) {
  const spends = dailySpend(days, events);
  const weeklyMax = Math.max(...spends.map((spend) => spend.amount), 0);
  const yMax = weeklyMax * 1.05 || 1;
  const today = localDateKey();
  return <div className={styles.chart}>
    {days.map((day, index) => {
      const isToday = day.date === today;
      const spend = spends[index];
      const zero = spend.amount <= 0;
      const tooltip = formatSpend(spend);
      const fill = zero ? "var(--chart-zero)" : heat[weekHeat(spend.amount, weeklyMax)];
      return <Tooltip content={tooltip} key={day.date}>
        <button aria-label={tooltip} className={styles.day} type="button">
          <span className={`${styles.value} ${isToday ? styles.valueToday : ""}`}>{zero ? "" : formatSpend(spend)}</span>
          <span className={styles.plot}>
            <span className={`${styles.bar} ${isToday && !zero ? styles.todayBar : ""} ${zero ? styles.zeroBar : ""}`} style={{ background: fill, height: zero ? undefined : `${Math.max(spend.amount / yMax * 100, 4)}%` }} />
          </span>
          <span className={`${styles.label} ${isToday ? styles.today : ""}`}>{weekday.format(new Date(`${day.date}T12:00:00`))}</span>
        </button>
      </Tooltip>;
    })}
  </div>;
}
