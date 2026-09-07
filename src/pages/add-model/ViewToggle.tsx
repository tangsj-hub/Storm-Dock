import { Columns2, LayoutList, PanelLeft } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Tooltip } from "../../components/Tooltip";
import { DISCOVER_VIEW_ORDER, type DiscoverListView } from "./discoverListView";
import extra from "./page.module.css";

const VIEW_META: Record<DiscoverListView, { icon: typeof Columns2; labelKey: string }> = {
  cards: { icon: Columns2, labelKey: "modelViewCards" },
  split: { icon: PanelLeft, labelKey: "modelViewSplit" },
  compact: { icon: LayoutList, labelKey: "modelViewCompact" },
};

export function ViewToggle({
  value,
  onChange,
}: {
  value: DiscoverListView;
  onChange: (view: DiscoverListView) => void;
}) {
  const { t } = useTranslation();
  const activeIndex = Math.max(0, DISCOVER_VIEW_ORDER.indexOf(value));

  return (
    <div aria-label={t("modelViewLayout")} className={extra.viewToggle} role="group">
      <span
        aria-hidden="true"
        className={extra.viewTogglePill}
        style={{ transform: `translateX(${activeIndex * 100}%)` }}
      />
      {DISCOVER_VIEW_ORDER.map((view) => {
        const meta = VIEW_META[view];
        const Icon = meta.icon;
        const label = t(meta.labelKey);
        const active = value === view;
        return (
          <Tooltip key={view} content={label}>
            <button
              aria-label={label}
              aria-pressed={active}
              className={`${extra.viewToggleBtn} ${active ? extra.viewToggleBtnActive : ""}`}
              onClick={() => onChange(view)}
              type="button"
            >
              <Icon aria-hidden="true" size={14} strokeWidth={1.75} />
            </button>
          </Tooltip>
        );
      })}
    </div>
  );
}
