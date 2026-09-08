import type { DiscoverListView } from "./discoverListView";
import { ViewToggle } from "./ViewToggle";
import extra from "./page.module.css";

export function DiscoverListChrome({
  view,
  onViewChange,
}: {
  view: DiscoverListView;
  onViewChange: (view: DiscoverListView) => void;
}) {
  return (
    <div className={extra.listChrome}>
      <div className={extra.listChromeTop}>
        <ViewToggle onChange={onViewChange} value={view} />
      </div>
    </div>
  );
}
