import styles from "./Switch.module.css";

/** Settings-style on/off switch (same look as the settings page `.switch`). */
export function Switch({ checked, disabled, label, onChange }: { checked: boolean; disabled?: boolean; label: string; onChange: (checked: boolean) => void }) {
  return (
    <button aria-checked={checked} aria-label={label} className={styles.switch} disabled={disabled} onClick={() => onChange(!checked)} role="switch" type="button">
      <span />
    </button>
  );
}
