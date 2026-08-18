import type { CursorUsageDetails, UsageEvent } from "../../lib/types";
import { displayModel, money, spendCents } from "./format";
import styles from "./ModelBars.module.css";

function modelSpend(models: CursorUsageDetails["models"], events: UsageEvent[]) {
  const totals = new Map<string, number>();
  for (const event of events) {
    const cents = spendCents(event);
    if (cents === undefined || !event.model) continue;
    totals.set(event.model, (totals.get(event.model) ?? 0) + cents);
  }
  const names = models.length ? models.map((model) => model.name) : [...totals.keys()];
  return names
    .map((name) => ({ name, cents: totals.get(name) ?? 0 }))
    .filter((model) => model.cents > 0)
    .sort((left, right) => right.cents - left.cents || left.name.localeCompare(right.name));
}

export function ModelBars({ models, events }: { models: CursorUsageDetails["models"]; events: UsageEvent[] }) {
  const items = modelSpend(models, events);
  const max = Math.max(...items.map((model) => model.cents), 1);
  return <ul className={styles.list}>
    {items.map((model) => {
      const percent = Math.max(model.cents / max * 100, 3);
      return <li className={styles.item} key={model.name}>
        <div className={styles.row}>
          <span className={styles.name} title={model.name}>{displayModel(model.name)}</span>
          <strong className={styles.count}>{money(model.cents)}</strong>
        </div>
        <div className={styles.track}>
          <span className={styles.fill} style={{ width: `${percent}%` }} />
        </div>
      </li>;
    })}
  </ul>;
}
