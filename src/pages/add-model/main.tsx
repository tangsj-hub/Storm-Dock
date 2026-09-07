import { ArrowLeft } from "lucide-react";
import { useCallback, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { DownloadDock } from "../../components/DownloadDock";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import { WindowDragSurface } from "../../components/WindowDragSurface";
import "../../i18n";
import { ModelCenter } from "../home/components/ModelCenter";
import { modelCenterPath, modelsHomePath, type ModelCenterTab, type ModelFormat, type ModelSource } from "../../lib/types";
import "../../styles/global.css";
import styles from "../add/page.module.css";
import { DiscoverPanel } from "./DiscoverPanel";
import extra from "./page.module.css";

const FORMATS: ModelFormat[] = ["all", "gguf", "safetensors", "mlx", "finetune"];

function AddModelPage() {
  const { t } = useTranslation();
  const home = modelsHomePath();
  const params = new URLSearchParams(window.location.search);
  const [query, setQuery] = useState(params.get("query") ?? "");
  const [source, setSource] = useState<ModelSource>(params.get("source") === "huggingface" ? "huggingface" : "modelscope");
  const [format, setFormat] = useState<ModelFormat>((FORMATS.includes(params.get("format") as ModelFormat) ? params.get("format") : "all") as ModelFormat);
  const [tab, setTab] = useState<ModelCenterTab>(params.get("tab") === "downloaded" ? "downloaded" : "discover");
  const [notice, setNotice] = useState<string>();
  const [noticeFailed, setNoticeFailed] = useState(false);

  const onNotice = useCallback((message: string, failed = false) => {
    setNotice(message);
    setNoticeFailed(failed);
  }, []);

  return (
    <Toast.Provider>
      <main className={styles.shell}>
        <WindowDragSurface />
        <header className={styles.header}>
          <a aria-label={t("back")} className={styles.back} href={home}>
            <ArrowLeft aria-hidden="true" size={20} />
          </a>
          <h1>{t("addModelTitle")}</h1>
        </header>
        <section className={`${styles.workspace} ${extra.workspaceFluid}`}>
          <div className={extra.pageStack}>
            <div className={`${styles.content} ${extra.modelCenterContent}`}>
              <div className={extra.centerTabs} role="tablist">
                <button
                  className={`${styles.tab} ${tab === "discover" ? styles.tabActive : ""}`}
                  onClick={() => {
                    setTab("discover");
                    window.history.replaceState({}, "", modelCenterPath({ query, source, format }));
                  }}
                  role="tab"
                  type="button"
                >
                  {t("modelCenterDiscover")}
                </button>
                <button
                  className={`${styles.tab} ${tab === "downloaded" ? styles.tabActive : ""}`}
                  onClick={() => {
                    setTab("downloaded");
                    window.history.replaceState({}, "", modelCenterPath({ query, source, format, tab: "downloaded" }));
                  }}
                  role="tab"
                  type="button"
                >
                  {t("modelCenterDownloaded")}
                </button>
              </div>
              {tab === "downloaded" ? (
                <div className={extra.downloadedPane}>
                  <ModelCenter onNotice={(message) => onNotice(message, false)} refreshKey={0} />
                </div>
              ) : (
                <DiscoverPanel
                  format={format}
                  onFormatChange={setFormat}
                  onNotice={onNotice}
                  onQueryChange={setQuery}
                  onSourceChange={setSource}
                  query={query}
                  source={source}
                  tab={tab}
                />
              )}
            </div>
          </div>
        </section>
      </main>
      <ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} status={noticeFailed ? "error" : "success"} />
      <Toast.Viewport className={styles.toastViewport} />
      <DownloadDock />
    </Toast.Provider>
  );
}

createRoot(document.getElementById("root")!).render(<AddModelPage />);
