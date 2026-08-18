import { Tooltip } from "../../components/Tooltip";
import type { CursorUsageDetails, UsageEvent } from "../../lib/types";
import { localDateKey, money, spendCents } from "./format";
import { heat, weekHeat } from "./heat";
import styles from "./WeeklyChart.module.css";

const weekday = new Intl.DateTimeFormat(undefined, { weekday: "short" });

function dailySpend(days: CursorUsageDetails["weekly"], events: UsageEvent[]) {
  const totals = new Map<string, number>();
  for (const event of events) {
    const cents = spendCents(event);
    if (cents === undefined) continue;
    const key = localDateKey(new Date(event.timestamp));
    totals.set(key, (totals.get(key) ?? 0) + cents);
  }
  return days.map((day) => totals.get(day.date) ?? day.onDemandCents);
}

export function WeeklyChart({ days, events }: { days: CursorUsageDetails["weekly"]; events: UsageEvent[] }) {
  const spends = dailySpend(days, events);
  const weeklyMax = Math.max(...spends, 0);
  const yMax = weeklyMax * 1.05 || 1;
  const today = localDateKey();
  return <div className={styles.chart}>
    {days.map((day, index) => {
      const isToday = day.date === today;
      const cents = spends[index];
      const zero = cents <= 0;
      const tooltip = money(cents);
      const fill = zero ? "#d5dce4" : heat[weekHeat(cents, weeklyMax)];
      return <Tooltip content={tooltip} key={day.date}>
        <button aria-label={tooltip} className={styles.day} type="button">
          <span className={`${styles.value} ${isToday ? styles.valueToday : ""}`}>{zero ? "" : money(cents)}</span>
          <span className={styles.plot}>
            <span className={`${styles.bar} ${isToday && !zero ? styles.todayBar : ""} ${zero ? styles.zeroBar : ""}`} style={{ background: fill, height: zero ? undefined : `${Math.max(cents / yMax * 100, 4)}%` }} />
          </span>
          <span className={`${styles.label} ${isToday ? styles.today : ""}`}>{weekday.format(new Date(`${day.date}T12:00:00`))}</span>
        </button>
      </Tooltip>;
    })}
  </div>;
}
