import type { CursorUsageDetails, UsageEvent } from "../../lib/types";
import { displayModel, eventSpend, formatSpend } from "./format";
import styles from "./ModelBars.module.css";

function modelSpend(models: CursorUsageDetails["models"], events: UsageEvent[]) {
  const totals = new Map<string, { amount: number; unit: "credits" | "currency" }>();
  for (const event of events) {
    const spend = eventSpend(event);
    if (!spend || !event.model) continue;
    const prev = totals.get(event.model);
    if (prev) {
      totals.set(event.model, { amount: prev.amount + spend.amount, unit: spend.unit });
    } else {
      totals.set(event.model, { ...spend });
    }
  }
  const names = models.length ? models.map((model) => model.name) : [...totals.keys()];
  return names
    .map((name) => {
      const spend = totals.get(name) ?? { amount: 0, unit: "currency" as const };
      return { name, ...spend };
    })
    .filter((model) => model.amount > 0)
    .sort((left, right) => right.amount - left.amount || left.name.localeCompare(right.name));
}

export function ModelBars({ models, events }: { models: CursorUsageDetails["models"]; events: UsageEvent[] }) {
  const items = modelSpend(models, events);
  const max = Math.max(...items.map((model) => model.amount), 1);
  return <ul className={styles.list}>
    {items.map((model) => {
      const percent = Math.max(model.amount / max * 100, 3);
      return <li className={styles.item} key={model.name}>
        <div className={styles.row}>
          <span className={styles.name} title={model.name}>{displayModel(model.name)}</span>
          <strong className={styles.count}>{formatSpend({ amount: model.amount, unit: model.unit })}</strong>
        </div>
        <div className={styles.track}>
          <span className={styles.fill} style={{ width: `${percent}%` }} />
        </div>
      </li>;
    })}
  </ul>;
}
