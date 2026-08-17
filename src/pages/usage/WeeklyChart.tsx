import { Tooltip } from "../../components/Tooltip";
import type { CursorUsageDetails } from "../../lib/types";
import { localDateKey, money } from "./format";
import { heat, weekHeat } from "./heat";
import styles from "./WeeklyChart.module.css";

const weekday = new Intl.DateTimeFormat(undefined, { weekday: "short" });

export function WeeklyChart({ days, unitsLabel }: { days: CursorUsageDetails["weekly"]; unitsLabel: string }) {
  const weeklyMax = Math.max(...days.map((day) => day.requests), 0);
  const yMax = weeklyMax * 1.05 || 1;
  const today = localDateKey();
  return <div className={styles.chart}>
    {days.map((day) => {
      const isToday = day.date === today;
      const zero = day.requests === 0;
      const tooltip = day.isOnDemand ? money(day.onDemandCents) : `${Math.round(day.requests)} ${unitsLabel}`;
      const fill = zero ? "#d5dce4" : heat[weekHeat(day.requests, weeklyMax)];
      return <Tooltip content={tooltip} key={day.date}>
        <button aria-label={tooltip} className={styles.day} type="button">
          <span className={`${styles.value} ${isToday ? styles.valueToday : ""}`}>{zero ? "" : Math.round(day.requests)}</span>
          <span className={styles.plot}>
            <span className={`${styles.bar} ${isToday && !zero ? styles.todayBar : ""} ${zero ? styles.zeroBar : ""}`} style={{ background: fill, height: zero ? undefined : `${Math.max(day.requests / yMax * 100, 4)}%` }} />
          </span>
          <span className={`${styles.label} ${isToday ? styles.today : ""}`}>{weekday.format(new Date(`${day.date}T12:00:00`))}</span>
        </button>
      </Tooltip>;
    })}
  </div>;
}
