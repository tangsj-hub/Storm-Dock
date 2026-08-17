import type { CursorUsageDetails } from "../../lib/types";
import { displayModel, number } from "./format";
import styles from "./ModelBars.module.css";

export function ModelBars({ models }: { models: CursorUsageDetails["models"] }) {
  const max = Math.max(...models.map((model) => model.requests), 1);
  return <ul className={styles.list}>
    {models.map((model) => {
      const percent = Math.max(model.requests / max * 100, model.requests > 0 ? 3 : 0);
      return <li className={styles.item} key={model.name}>
        <div className={styles.row}>
          <span className={styles.name} title={model.name}>{displayModel(model.name)}</span>
          <strong className={styles.count}>{number.format(model.requests)}</strong>
        </div>
        <div className={styles.track}>
          <span className={styles.fill} style={{ width: `${percent}%` }} />
        </div>
      </li>;
    })}
  </ul>;
}
