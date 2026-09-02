import { ArrowLeft, Download, Eye, Heart, Image, Search, Volume2 } from "lucide-react";
import { useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { DownloadDock } from "../../components/DownloadDock";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import { Tooltip } from "../../components/Tooltip";
import { WindowDragSurface } from "../../components/WindowDragSurface";
import "../../i18n";
import { searchRemoteModels } from "../../lib/api";
import { agoParts, avatarTone, formatCount, hitCapabilities, isGgufHit, type HitCapability } from "../../lib/modelHits";
import { modelsHomePath, modelDetailPath, type ModelFormat, type ModelSource, type RemoteModelHit } from "../../lib/types";
import "../../styles/global.css";
import styles from "../add/page.module.css";
import extra from "./page.module.css";

const FORMATS: ModelFormat[] = ["all", "gguf", "safetensors", "mlx", "finetune"];

const CAP_ICON = {
  vision: Eye,
  image: Image,
  audio: Volume2,
} as const;

const CAP_CLASS = {
  vision: extra.hitCapVision,
  image: extra.hitCapImage,
  audio: extra.hitCapAudio,
} as const;

const CAP_LABEL = {
  vision: "modelCapVision",
  image: "modelCapImage",
  audio: "modelCapAudio",
} as const;

const AVATAR_CLASS = [extra.hitAvatar0, extra.hitAvatar1, extra.hitAvatar2];

function formatLabelKey(format: ModelFormat) {
  if (format === "gguf") return "modelFormatGguf";
  if (format === "safetensors") return "modelFormatSafetensors";
  if (format === "mlx") return "modelFormatMlx";
  if (format === "finetune") return "modelFormatFinetune";
  return "modelFormatAll";
}

function invokeMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function agoLabel(value: string | undefined, t: (key: string, opts?: { count: number }) => string) {
  const ago = agoParts(value);
  if (!ago) return;
  if (ago.key === "justNow") return t("modelAgoJustNow");
  if (ago.key === "minutes") return t("modelAgoMinutes", { count: ago.count });
  if (ago.key === "hours") return t("modelAgoHours", { count: ago.count });
  return t("modelAgoDays", { count: ago.count });
}

function CapabilityIcon({ kind }: { kind: HitCapability }) {
  const { t } = useTranslation();
  const Icon = CAP_ICON[kind];
  return (
    <Tooltip content={t(CAP_LABEL[kind])}>
      <span className={`${extra.hitCap} ${CAP_CLASS[kind]}`}>
        <Icon aria-hidden="true" size={12} />
      </span>
    </Tooltip>
  );
}

function HitRow({ hit }: { hit: RemoteModelHit }) {
  const { t } = useTranslation();
  const caps = hitCapabilities(hit);
  return (
    <a className={extra.hitCard} href={modelDetailPath(hit.source, hit.repo)}>
      <div className={extra.hitModel}>
        <span className={`${extra.hitAvatar} ${AVATAR_CLASS[avatarTone(hit.author)]}`}>{hit.author.slice(0, 1).toUpperCase() || "?"}</span>
        <div className={extra.hitCopy}>
          <div className={extra.hitTitleRow}>
            <strong className={extra.hitTitle}>{hit.name || hit.repo}</strong>
            {isGgufHit(hit) ? <span aria-label="GGUF" className={extra.hitGguf} /> : null}
          </div>
          <span className={extra.hitAuthor}>{hit.author}</span>
        </div>
      </div>
      <div className={extra.hitCaps}>{caps.map((kind) => <CapabilityIcon key={kind} kind={kind} />)}</div>
      <span className={extra.hitMetric}>{hit.params ?? ""}</span>
      <span className={extra.hitMetric}>{agoLabel(hit.updatedAt, t) ?? ""}</span>
      <span className={extra.hitStat}>{hit.downloads != null ? <><Download aria-hidden="true" size={13} />{formatCount(hit.downloads)}</> : null}</span>
      <span className={extra.hitStat}>{hit.likes != null ? <><Heart aria-hidden="true" size={13} />{formatCount(hit.likes)}</> : null}</span>
      <span aria-hidden="true" className={extra.hitDownload}><Download size={16} /></span>
    </a>
  );
}

function AddModelPage() {
  const { t } = useTranslation();
  const home = modelsHomePath();
  const [query, setQuery] = useState("");
  const [source, setSource] = useState<ModelSource>("modelscope");
  const [format, setFormat] = useState<ModelFormat>("all");
  const [searching, setSearching] = useState(false);
  const [hits, setHits] = useState<RemoteModelHit[]>();
  const [notice, setNotice] = useState<string>();
  const [noticeFailed, setNoticeFailed] = useState(false);

  const search = async () => {
    const q = query.trim();
    if (!q) return;
    setSearching(true);
    setNotice(undefined);
    try {
      setHits(await searchRemoteModels(source, q, format));
    } catch (error) {
      setHits(undefined);
      setNoticeFailed(true);
      setNotice(invokeMessage(error));
    } finally {
      setSearching(false);
    }
  };

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
        <section className={styles.workspace}>
          <div className={styles.addPage}>
            <div className={styles.content}>
              <form
                className={styles.panel}
                onSubmit={(event) => {
                  event.preventDefault();
                  if (!searching) void search();
                }}
              >
                <div className={extra.field}>
                  <span>{t("modelIdLabel")}</span>
                  <div className={extra.idRow}>
                    <input autoFocus autoComplete="off" disabled={searching} onChange={(event) => { setQuery(event.target.value); setHits(undefined); }} placeholder={t("modelRepoPlaceholder")} spellCheck={false} value={query} />
                    <select aria-label={t("modelFormatLabel")} className={extra.sourceSelect} disabled={searching} onChange={(event) => { setFormat(event.target.value as ModelFormat); setHits(undefined); }} value={format}>
                      {FORMATS.map((item) => <option key={item} value={item}>{t(formatLabelKey(item))}</option>)}
                    </select>
                    <select aria-label={t("modelSourceLabel")} className={extra.sourceSelect} disabled={searching} onChange={(event) => { setSource(event.target.value as ModelSource); setHits(undefined); }} value={source}>
                      <option value="modelscope">{t("modelSourceModelScope")}</option>
                      <option value="huggingface">{t("modelSourceHuggingFace")}</option>
                    </select>
                    <button className={styles.primary} disabled={searching || !query.trim()} type="submit">
                      <Search aria-hidden="true" size={16} />
                      {searching ? t("modelProbing") : t("modelProbe")}
                    </button>
                  </div>
                  {source === "huggingface" ? (
                    <p className={extra.sourceHint}>
                      {t("modelUseHuggingFaceHint")}{" "}
                      <a href="/settings.html?tab=huggingface">{t("hfOpenSettings")}</a>
                    </p>
                  ) : null}
                </div>
                {hits ? (
                  hits.length === 0 ? <p className={extra.empty}>{t("modelSearchEmpty")}</p> : (
                    <div className={extra.hitList}>
                      <div className={extra.hitHead}>
                        <span>{t("modelColModel")}</span>
                        <span>{t("modelColCapabilities")}</span>
                        <span>{t("modelColSize")}</span>
                        <span>{t("modelStatUpdated")}</span>
                        <span>{t("modelStatDownloads")}</span>
                        <span>{t("modelStatLikes")}</span>
                        <span />
                      </div>
                      {hits.map((hit) => <HitRow hit={hit} key={`${hit.source}:${hit.repo}`} />)}
                    </div>
                  )
                ) : null}
                <div className={styles.actions}>
                  <a className={styles.secondary} href={home}>{t("cancel")}</a>
                </div>
              </form>
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
