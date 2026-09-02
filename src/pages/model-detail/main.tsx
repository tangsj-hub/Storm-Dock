import { listen } from "@tauri-apps/api/event";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { ArrowLeft, BookOpen, Calendar, CheckCircle2, ChevronDown, CircleX, Cpu, Download, FileText, Heart, HelpCircle, Info, TriangleAlert, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { DownloadDock } from "../../components/DownloadDock";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import { Tooltip } from "../../components/Tooltip";
import { WindowDragSurface } from "../../components/WindowDragSurface";
import "../../i18n";
import { cancelModelDownload, listDownloadJobs, probeRemoteModel, startModelDownloadFast, type ModelPlatform } from "../../lib/api";
import { modelCenterPath, modelRepoFromQuery, modelSourceFromQuery, type DownloadJob, type DownloadSnapshot, type ModelCenterContext, type ModelFit, type RemoteModelProbe, type RemoteModelVariant } from "../../lib/types";
import "../../styles/global.css";
import styles from "../add/page.module.css";
import extra from "../add-model/page.module.css";

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

function formatCount(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
  return String(value);
}

function variantFormat(variant: RemoteModelVariant) {
  if (variant.id.startsWith("gguf:")) return "GGUF";
  if (variant.id === "safetensors") return "Safetensors";
  if (variant.id === "pytorch") return "PyTorch";
  return "Weights";
}

function formatUpdated(value?: string) {
  if (!value) return;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleDateString(undefined, { year: "numeric", month: "short" });
}

function fitMeta(fit: ModelFit) {
  if (fit === "fits") return { Icon: CheckCircle2, tone: extra.toneOk, key: "modelFitFits" as const };
  if (fit === "marginal") return { Icon: TriangleAlert, tone: extra.toneWarn, key: "modelFitMarginal" as const };
  if (fit === "partial") return { Icon: Info, tone: extra.toneInfo, key: "modelFitPartial" as const };
  if (fit === "ram") return { Icon: Cpu, tone: extra.toneInfo, key: "modelFitRam" as const };
  if (fit === "oom") return { Icon: CircleX, tone: extra.toneBad, key: "modelFitOom" as const };
  return { Icon: HelpCircle, tone: extra.toneMuted, key: "modelFitUnknown" as const };
}

function VariantChip({ variant }: { variant: RemoteModelVariant }) {
  const { t } = useTranslation();
  const { Icon, tone, key } = fitMeta(variant.fit);
  return (
    <span className={extra.chip}>
      <Tooltip content={t(key)}>
        <span aria-label={t(key)} className={`${extra.fitIcon} ${tone}`}>
          <Icon aria-hidden="true" size={14} />
        </span>
      </Tooltip>
      <span className={extra.chipText}>{variant.label}</span>
    </span>
  );
}

function invokeMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function diskNotice(t: (key: string, opts: { need: string; free: string }) => string, error: unknown) {
  const raw = invokeMessage(error);
  const match = /^insufficient-disk:(\d+):(\d+)$/.exec(raw);
  if (!match) return raw;
  return t("modelDiskFull", { need: formatBytes(Number(match[1])), free: formatBytes(Number(match[2])) });
}

function ModelDetailPage() {
  const { t } = useTranslation();
  const source = modelSourceFromQuery();
  const repo = modelRepoFromQuery();
  const params = new URLSearchParams(window.location.search);
  const context: ModelCenterContext = { query: params.get("query") ?? "", source: params.get("listSource") === "huggingface" ? "huggingface" : params.get("listSource") === "modelscope" ? "modelscope" : undefined, format: (params.get("format") as ModelCenterContext["format"]) ?? "all", tab: params.get("tab") === "downloaded" ? "downloaded" : "discover" };
  const search = modelCenterPath(context);
  const home = modelCenterPath();
  const [loading, setLoading] = useState(true);
  const [probe, setProbe] = useState<RemoteModelProbe>();
  const [variantId, setVariantId] = useState("");
  const [platform, setPlatform] = useState<ModelPlatform>("generic");
  const [job, setJob] = useState<DownloadJob>();
  const [starting, setStarting] = useState(false);
  const [notice, setNotice] = useState<string>();
  const [noticeFailed, setNoticeFailed] = useState(false);
  const watching = useRef(false);
  const downloading = starting || job?.status === "downloading" || job?.status === "queued" || job?.status === "verifying";
  const selected = useMemo(
    () => probe?.variants.find((variant) => variant.id === variantId) ?? probe?.variants[0],
    [probe, variantId],
  );
  const card = probe?.card;
  const showNotice = (text: string, failed = false) => {
    setNoticeFailed(failed);
    setNotice(text);
  };

  useEffect(() => {
    if (!source || !repo) {
      window.location.replace(search);
      return;
    }
    let cancelled = false;
    setLoading(true);
    void probeRemoteModel(source, repo)
      .then((next) => {
        if (cancelled) return;
        setProbe(next);
        setVariantId(next.defaultVariantId);
        setPlatform("generic");
      })
      .catch((error) => {
        if (!cancelled) showNotice(invokeMessage(error), true);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [repo, search, source]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    const pick = (jobs: DownloadJob[]) => {
      const match = jobs.find((item) => item.repo === repo && (item.status === "downloading" || item.status === "queued" || item.status === "paused" || item.status === "verifying"));
      setJob(match);
      if (match) watching.current = true;
      const done = jobs.find((item) => item.repo === repo && item.status === "completed");
      if (done && watching.current) {
        window.location.assign(modelCenterPath({ tab: "downloaded" }));
      }
    };
    void listDownloadJobs().then(pick).catch(() => undefined);
    void listen<DownloadSnapshot>("model-download-snapshot", ({ payload }) => pick(payload.jobs)).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [repo, t]);

  const download = async () => {
    if (!probe || !selected || starting) return;
    setNotice(undefined);
    setStarting(true);
    watching.current = true;
    try {
      await startModelDownloadFast(probe.source, probe.repo, probe.revision, selected.files.map((path) => probe.files.find((file) => file.path === path)).filter((file): file is NonNullable<typeof file> => Boolean(file)), platform);
      showNotice(t("modelDownloadStarted", { repo: probe.repo }));
    } catch (error) {
      showNotice(diskNotice(t, error), true);
      watching.current = false;
    } finally {
      setStarting(false);
    }
  };

  const cancel = async () => {
    if (!job?.jobId) return;
    try {
      await cancelModelDownload(job.jobId);
    } catch (error) {
      showNotice(invokeMessage(error), true);
    }
  };

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
        <section className={styles.workspace}>
          <div className={styles.addPage}>
            <div className={styles.content}>
              <div className={styles.panel}>
                {loading ? <p className={extra.empty}>{t("loading")}</p> : null}
                {probe && selected && card ? (
                  <div className={extra.modelCard}>
                    <div className={extra.modelHead}>
                      <strong className={extra.modelTitle}>{card.name || probe.repo}</strong>
                      {card.author ? <span className={extra.modelAuthor}>{card.author}</span> : null}
                      {card.tags.length || card.baseModel ? (
                        <div className={extra.tags}>
                          {card.tags.map((tag) => <span className={extra.tag} key={tag}>{tag}</span>)}
                          {card.baseModel ? <span className={`${extra.tag} ${extra.baseTag}`}>{t("modelBase", { repo: card.baseModel })}</span> : null}
                        </div>
                      ) : null}
                    </div>
                    <div className={extra.downloadBar}>
                      <label className={extra.platformPicker}><span>{t("modelPlatform")}</span><select aria-label={t("modelPlatform")} disabled={downloading} onChange={(event) => setPlatform(event.target.value as ModelPlatform)} value={platform}><option value="generic">{t("modelPlatformGeneric")}</option><option value="unsloth">{t("modelPlatformUnsloth")}</option><option value="llama-cpp">{t("modelPlatformLlamaCpp")}</option></select></label>
                      <DropdownMenu.Root>
                        <DropdownMenu.Trigger asChild>
                          <button aria-label={t("modelWeightLabel")} className={extra.picker} disabled={downloading || probe.variants.length < 2} type="button">
                            <VariantChip variant={selected} />
                            <span className={extra.format}><span className={extra.formatDot} />{variantFormat(selected)}</span>
                            <span className={extra.size}>{formatBytes(selected.size)}</span>
                            {probe.variants.length > 1 ? <ChevronDown aria-hidden="true" className={extra.chevron} size={16} /> : null}
                          </button>
                        </DropdownMenu.Trigger>
                        <DropdownMenu.Portal>
                          <DropdownMenu.Content align="start" className={extra.pickerMenu} sideOffset={6}>
                            {probe.variants.map((variant) => (
                              <DropdownMenu.Item className={extra.pickerItem} data-selected={variant.id === selected.id} key={variant.id} onSelect={() => setVariantId(variant.id)}>
                                <VariantChip variant={variant} />
                                <span className={extra.sizePill}>{formatBytes(variant.size)}</span>
                              </DropdownMenu.Item>
                            ))}
                          </DropdownMenu.Content>
                        </DropdownMenu.Portal>
                      </DropdownMenu.Root>
                      {downloading ? (
                        <button className={styles.secondary} onClick={() => void cancel()} type="button">
                          <X aria-hidden="true" size={16} />
                          {t("modelCancelDownload")}
                        </button>
                      ) : (
                        <button className={styles.primary} disabled={selected.files.length === 0} onClick={() => void download()} type="button">
                          <Download aria-hidden="true" size={16} />
                          {t("modelDownload")}
                        </button>
                      )}
                    </div>
                    <div className={extra.stats}>
                      {card.updatedAt ? <span className={extra.stat}><Calendar aria-hidden="true" size={13} />{t("modelStatUpdated")} {formatUpdated(card.updatedAt)}</span> : null}
                      {card.downloads != null ? <span className={extra.stat}><Download aria-hidden="true" size={13} />{formatCount(card.downloads)}</span> : null}
                      {card.likes != null ? <span className={extra.stat}><Heart aria-hidden="true" size={13} />{formatCount(card.likes)}</span> : null}
                      {card.params ? <span className={extra.stat}><Cpu aria-hidden="true" size={13} />{card.params}</span> : null}
                      {card.library ? <span className={extra.stat}><BookOpen aria-hidden="true" size={13} />{card.library}</span> : null}
                      {card.license ? <span className={extra.stat}><FileText aria-hidden="true" size={13} />{card.license}</span> : null}
                    </div>
                    {card.description ? (
                      <div className={extra.intro}>
                        <span className={extra.introLabel}>{t("modelIntro")}</span>
                        <p>{card.description}</p>
                      </div>
                    ) : null}
                  </div>
                ) : null}
                <div className={styles.actions}>
                  {downloading ? null : <a className={styles.secondary} href={home}>{t("cancel")}</a>}
                </div>
              </div>
            </div>
          </div>
        </section>
      </main>
      <ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} status={noticeFailed || job?.status === "failed" ? "error" : "success"} />
      <Toast.Viewport className={styles.toastViewport} />
      <DownloadDock />
    </Toast.Provider>
  );
}

createRoot(document.getElementById("root")!).render(<ModelDetailPage />);
