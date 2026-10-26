import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import * as Tabs from "@radix-ui/react-tabs";
import { Check, ChevronDown, Play } from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { HomeTabId } from "../../../lib/homeTabs";
import { planTabOverflow, type TabOverflowPlan } from "../lib/tabOverflow";
import styles from "../page.module.css";

type Props = {
  tabs: HomeTabId[];
  value: HomeTabId;
  icons: Record<HomeTabId, string>;
  busy?: boolean;
  canHoverLaunch: (kind: HomeTabId) => boolean;
  labelFor: (kind: HomeTabId) => string;
  onLaunch: (kind: HomeTabId) => void;
  onValueChange: (value: HomeTabId) => void;
};

function TabGlyph({
  kind,
  icon,
  hoverLaunch,
  isActive,
  busy,
  onLaunch,
  launchLabel,
}: {
  kind: HomeTabId;
  icon: string;
  hoverLaunch: boolean;
  isActive: boolean;
  busy?: boolean;
  onLaunch: (kind: HomeTabId) => void;
  launchLabel: string;
}) {
  if (!hoverLaunch) {
    return <img alt="" className="ink" src={icon} />;
  }
  return (
    <span className={styles.appTabIcon}>
      <img alt="" className={`${styles.appTabIconGlyph} ${styles.appTabIdle} ink`} src={icon} />
      <span
        aria-label={launchLabel}
        className={styles.appTabLaunchHit}
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          if (!isActive || busy) return;
          onLaunch(kind);
        }}
        onKeyDown={(event) => {
          if (event.key !== "Enter" && event.key !== " ") return;
          event.preventDefault();
          event.stopPropagation();
          if (!isActive || busy) return;
          onLaunch(kind);
        }}
        onPointerDown={(event) => {
          event.preventDefault();
          event.stopPropagation();
        }}
        role="button"
        tabIndex={isActive ? 0 : -1}
        title={launchLabel}
      >
        <Play aria-hidden="true" className={styles.appTabIconGlyph} size={15} strokeWidth={2.4} />
      </span>
    </span>
  );
}

function horizontalChrome(element: Element): number {
  const style = getComputedStyle(element);
  return (
    (parseFloat(style.borderLeftWidth) || 0) +
    (parseFloat(style.borderRightWidth) || 0) +
    (parseFloat(style.paddingLeft) || 0) +
    (parseFloat(style.paddingRight) || 0)
  );
}

function flexGap(element: Element): number {
  const style = getComputedStyle(element);
  const parsed = parseFloat(style.columnGap || style.gap || "2");
  return Number.isFinite(parsed) ? parsed : 2;
}

function samePlan(a: TabOverflowPlan<HomeTabId>, b: TabOverflowPlan<HomeTabId>): boolean {
  return (
    a.visibleIds.length === b.visibleIds.length &&
    a.overflowIds.length === b.overflowIds.length &&
    a.visibleIds.every((id, i) => id === b.visibleIds[i]) &&
    a.overflowIds.every((id, i) => id === b.overflowIds[i])
  );
}

/** Prefer `.headerModes` content box — that is the flex-allocated middle slot. */
function allocatedSlotWidth(slot: HTMLElement): number {
  const modes = slot.parentElement;
  if (modes) {
    const box = modes.getBoundingClientRect().width;
    const style = getComputedStyle(modes);
    const pad =
      (parseFloat(style.paddingLeft) || 0) + (parseFloat(style.paddingRight) || 0);
    // clientWidth is safer when available (excludes scrollbar); fall back to rect.
    return modes.clientWidth || Math.max(0, box - pad);
  }
  return slot.clientWidth;
}

export function AppTabSwitcher({
  tabs,
  value,
  icons,
  busy,
  canHoverLaunch,
  labelFor,
  onLaunch,
  onValueChange,
}: Props) {
  const { t } = useTranslation();
  const slotRef = useRef<HTMLDivElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const measureRef = useRef<HTMLDivElement>(null);
  const overflowMeasureRef = useRef<HTMLButtonElement>(null);
  const [plan, setPlan] = useState<TabOverflowPlan<HomeTabId>>({
    visibleIds: tabs,
    overflowIds: [],
  });

  const tabsKey = tabs.join("|");
  const labelsKey = tabs.map(labelFor).join("|");

  useLayoutEffect(() => {
    const slot = slotRef.current;
    const viewport = viewportRef.current;
    const measure = measureRef.current;
    if (!slot || !viewport || !measure) return;

    const modes = slot.parentElement;
    let slotContentWidth = allocatedSlotWidth(slot);

    const recompute = () => {
      const widthById = new Map<HomeTabId, number>();
      for (const node of measure.querySelectorAll<HTMLElement>("[data-tab-measure]")) {
        widthById.set(node.dataset.tabMeasure as HomeTabId, node.getBoundingClientRect().width);
      }
      const next = planTabOverflow({
        tabIds: tabs,
        widthById,
        gap: flexGap(measure),
        // Only subtract the pill's border+padding once from the allocated middle slot.
        availableInnerWidth: Math.max(0, slotContentWidth - horizontalChrome(viewport)),
        overflowBtnWidth: overflowMeasureRef.current?.getBoundingClientRect().width ?? 0,
        selectedId: value,
      });
      setPlan((current) => (samePlan(current, next) ? current : next));
    };

    recompute();
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        if (modes && entry.target === modes) {
          const box = entry.contentBoxSize?.[0];
          slotContentWidth = box?.inlineSize ?? entry.contentRect.width ?? modes.clientWidth;
        } else if (entry.target === slot && !modes) {
          const box = entry.contentBoxSize?.[0];
          slotContentWidth = box?.inlineSize ?? entry.contentRect.width ?? slot.clientWidth;
        }
      }
      recompute();
    });
    if (modes) observer.observe(modes);
    observer.observe(slot);
    observer.observe(measure);
    return () => observer.disconnect();
  }, [tabs, tabsKey, labelsKey, value]);

  const { visibleIds, overflowIds } = plan;
  const overflowActive = overflowIds.includes(value);

  return (
    <div className={styles.switcher} ref={slotRef}>
      <Tabs.Root
        onValueChange={(next) => onValueChange(next as HomeTabId)}
        value={value}
      >
        <div aria-hidden="true" className={styles.appTabMeasure} ref={measureRef}>
          {tabs.map((kind) => (
            <button
              className={styles.appTab}
              data-tab-measure={kind}
              key={kind}
              tabIndex={-1}
              type="button"
            >
              <img alt="" className="ink" src={icons[kind]} />
              {labelFor(kind)}
            </button>
          ))}
          <button
            className={styles.appTabOverflowTrigger}
            ref={overflowMeasureRef}
            tabIndex={-1}
            type="button"
          >
            <ChevronDown aria-hidden="true" size={16} />
          </button>
        </div>

        <div className={styles.switcherViewport} ref={viewportRef}>
          <Tabs.List aria-label={t("applications")} className={styles.switcherList}>
            {visibleIds.map((kind) => {
              const hoverLaunch = canHoverLaunch(kind);
              const isActive = value === kind;
              return (
                <Tabs.Trigger
                  className={`${styles.appTab}${hoverLaunch ? ` ${styles.appTabLaunchable}` : ""}`}
                  key={kind}
                  value={kind}
                >
                  <TabGlyph
                    busy={busy}
                    hoverLaunch={hoverLaunch}
                    icon={icons[kind]}
                    isActive={isActive}
                    kind={kind}
                    launchLabel={t("launchApp")}
                    onLaunch={onLaunch}
                  />
                  {labelFor(kind)}
                </Tabs.Trigger>
              );
            })}
          </Tabs.List>

          {overflowIds.length > 0 ? (
            <DropdownMenu.Root>
              <DropdownMenu.Trigger
                aria-label={t("moreApplications")}
                className={`${styles.appTabOverflowTrigger}${overflowActive ? ` ${styles.appTabOverflowTriggerActive}` : ""}`}
                title={t("moreApplications")}
              >
                <ChevronDown aria-hidden="true" size={16} />
              </DropdownMenu.Trigger>
              <DropdownMenu.Portal>
                <DropdownMenu.Content align="end" className={styles.appTabMenu} sideOffset={6}>
                  {overflowIds.map((kind) => (
                    <DropdownMenu.Item
                      className={styles.appTabMenuItem}
                      key={kind}
                      onSelect={() => onValueChange(kind)}
                    >
                      <span className={styles.appTabMenuItemLabel}>
                        <img alt="" className="ink" src={icons[kind]} />
                        {labelFor(kind)}
                      </span>
                      {value === kind ? <Check aria-hidden="true" size={16} /> : null}
                    </DropdownMenu.Item>
                  ))}
                </DropdownMenu.Content>
              </DropdownMenu.Portal>
            </DropdownMenu.Root>
          ) : null}
        </div>
      </Tabs.Root>
    </div>
  );
}
