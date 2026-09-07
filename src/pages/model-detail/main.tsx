import { ArrowLeft } from "lucide-react";
import { useCallback, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { DownloadDock } from "../../components/DownloadDock";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import { WindowDragSurface } from "../../components/WindowDragSurface";
import "../../i18n";
import { modelCenterPath, modelRepoFromQuery, modelSourceFromQuery, type ModelCenterContext } from "../../lib/types";
import "../../styles/global.css";
import styles from "../add/page.module.css";
import extra from "../add-model/page.module.css";
import { ModelDetailView } from "./ModelDetailView";

function ModelDetailPage() {
  const { t } = useTranslation();
  const source = modelSourceFromQuery();
  const repo = modelRepoFromQuery();
  const params = new URLSearchParams(window.location.search);
  const context: ModelCenterContext = {
    query: params.get("query") ?? "",
    source: params.get("listSource") === "huggingface"
      ? "huggingface"
      : params.get("listSource") === "modelscope"
        ? "modelscope"
        : undefined,
    format: (params.get("format") as ModelCenterContext["format"]) ?? "all",
    tab: params.get("tab") === "downloaded" ? "downloaded" : "discover",
  };
  const search = modelCenterPath(context);
  const [notice, setNotice] = useState<string>();
  const [noticeFailed, setNoticeFailed] = useState(false);

  const onNotice = useCallback((message: string, failed = false) => {
    setNoticeFailed(failed);
    setNotice(message);
  }, []);

  if (!source || !repo) {
    window.location.replace(search);
    return null;
  }

  return (
    <Toast.Provider>
      <main className={styles.shell}>
        <WindowDragSurface />
        <header className={styles.header}>
          <a aria-label={t("back")} className={styles.back} href={search}>
            <ArrowLeft aria-hidden="true" size={20} />
          </a>
          <h1>{t("modelDetailTitle")}</h1>
        </header>
        <section className={`${styles.workspace} ${extra.workspaceFluid}`}>
          <div className={extra.pageStack}>
            <div className={`${styles.content} ${extra.detailPanel}`}>
              <ModelDetailView onNotice={onNotice} repo={repo} source={source} />
            </div>
          </div>
        </section>
      </main>
      <ToastMessage
        notice={notice}
        onOpenChange={(open) => {
          if (!open) setNotice(undefined);
        }}
        status={noticeFailed ? "error" : "success"}
      />
      <Toast.Viewport className={styles.toastViewport} />
      <DownloadDock />
    </Toast.Provider>
  );
}

createRoot(document.getElementById("root")!).render(<ModelDetailPage />);
