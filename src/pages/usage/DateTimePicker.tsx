import * as Popover from "@radix-ui/react-popover";
import { ChevronDown, ChevronLeft, ChevronRight } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { localDateKey, localHourKey } from "./format";
import styles from "./DateTimePicker.module.css";

const HOURS = Array.from({ length: 24 }, (_, hour) => hour);
const WEEKDAYS = Array.from({ length: 7 }, (_, day) => {
  const date = new Date(Date.UTC(2021, 0, 3 + day));
  return new Intl.DateTimeFormat(undefined, { weekday: "narrow", timeZone: "UTC" }).format(date);
});
const monthLabel = new Intl.DateTimeFormat(undefined, { year: "numeric", month: "long" });
const valueLabel = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", hour: "2-digit" });

function parseValue(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? new Date() : date;
}

function daysInMonth(year: number, month: number) {
  return new Date(year, month + 1, 0).getDate();
}

function startWeekday(year: number, month: number) {
  return new Date(year, month, 1).getDay();
}

export function DateTimePicker({
  label,
  max,
  min,
  onChange,
  value
}: {
  label: string;
  max?: string;
  min?: string;
  onChange: (value: string) => void;
  value: string;
}) {
  const { t } = useTranslation();
  const selected = useMemo(() => parseValue(value), [value]);
  const [view, setView] = useState({ year: selected.getFullYear(), month: selected.getMonth() });
  useEffect(() => {
    setView({ year: selected.getFullYear(), month: selected.getMonth() });
  }, [selected]);
  const minDate = min?.slice(0, 10);
  const maxDate = max?.slice(0, 10);
  const blanks = startWeekday(view.year, view.month);
  const days = daysInMonth(view.year, view.month);
  const selectedKey = localDateKey(selected);
  const selectedHour = selected.getHours();
  const canPrev = !minDate || localDateKey(new Date(view.year, view.month, 0)) >= minDate;
  const canNext = !maxDate || localDateKey(new Date(view.year, view.month + 1, 1)) <= maxDate;
  const setDay = (day: number) => {
    const next = new Date(selected);
    next.setFullYear(view.year, view.month, day);
    onChange(localHourKey(next));
  };
  const setHour = (hour: number) => {
    const next = new Date(selected);
    next.setHours(hour, 0, 0, 0);
    onChange(localHourKey(next));
  };
  const disabledDay = (day: number) => {
    const key = localDateKey(new Date(view.year, view.month, day));
    return Boolean((minDate && key < minDate) || (maxDate && key > maxDate));
  };
  return <div className={styles.field}>
    <span>{label}</span>
    <Popover.Root>
      <Popover.Trigger className={styles.trigger} type="button">
        <span>{valueLabel.format(selected)}</span>
        <ChevronDown aria-hidden="true" size={14} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content align="end" className={styles.panel} sideOffset={6}>
          <div className={styles.calendar}>
            <div className={styles.monthRow}>
              <button aria-label={t("usagePrevMonth")} className={styles.nav} disabled={!canPrev} onClick={() => setView({ year: view.month === 0 ? view.year - 1 : view.year, month: view.month === 0 ? 11 : view.month - 1 })} type="button">
                <ChevronLeft aria-hidden="true" size={16} />
              </button>
              <strong>{monthLabel.format(new Date(view.year, view.month, 1))}</strong>
              <button aria-label={t("usageNextMonth")} className={styles.nav} disabled={!canNext} onClick={() => setView({ year: view.month === 11 ? view.year + 1 : view.year, month: view.month === 11 ? 0 : view.month + 1 })} type="button">
                <ChevronRight aria-hidden="true" size={16} />
              </button>
            </div>
            <div className={styles.weekdays}>{WEEKDAYS.map((day, index) => <span key={index}>{day}</span>)}</div>
            <div className={styles.days}>
              {Array.from({ length: blanks }, (_, index) => <span key={`blank-${index}`} />)}
              {Array.from({ length: days }, (_, index) => {
                const day = index + 1;
                const key = localDateKey(new Date(view.year, view.month, day));
                return <button
                  className={`${styles.day} ${key === selectedKey ? styles.dayActive : ""}`}
                  disabled={disabledDay(day)}
                  key={key}
                  onClick={() => setDay(day)}
                  type="button"
                >{day}</button>;
              })}
            </div>
          </div>
          <div className={styles.hours} role="listbox" aria-label={t("usageHourList")}>
            {HOURS.map((hour) => (
              <button
                aria-selected={hour === selectedHour}
                className={`${styles.hour} ${hour === selectedHour ? styles.hourActive : ""}`}
                key={hour}
                onClick={() => setHour(hour)}
                role="option"
                type="button"
              >{t("usageHour", { hour: String(hour).padStart(2, "0") })}</button>
            ))}
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  </div>;
}
