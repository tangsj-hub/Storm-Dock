import { listen } from "@tauri-apps/api/event";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { BookOpen, Calendar, CheckCircle2, ChevronDown, CircleX, Cpu, Download, FileText, Heart, HelpCircle, Info, TriangleAlert, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ModelIdentity } from "../add-model/ModelIdentity";
import { Tooltip } from "../../components/Tooltip";
import { cancelModelDownload, listDownloadJobs, probeRemoteModel, startModelDownloadFast } from "../../lib/api";
import {
  modelCenterPath,
  type DownloadJob,
  type DownloadSnapshot,
  type ModelFit,
  type ModelSource,
  type RemoteModelProbe,
  type RemoteModelVariant,
} from "../../lib/types";
import styles from "../add/page.module.css";
import extra from "../add-model/page.module.css";
import { DetailSkeleton } from "./DetailSkeleton";
import { ModelReadme } from "./ModelReadme";
import { useModelReadme } from "./useModelReadme";

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

export type ModelDetailViewProps = {
  source: ModelSource;
  repo: string;
  /** When true, skip full-page redirect after download completes. */
  embedded?: boolean;
  onNotice?: (message: string, failed?: boolean) => void;
};

/** Shared model detail body: meta, download, README-only scroll. */
export function ModelDetailView({ source, repo, embedded = false, onNotice }: ModelDetailViewProps) {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(true);
  const [probe, setProbe] = useState<RemoteModelProbe>();
  const [variantId, setVariantId] = useState("");
  const [job, setJob] = useState<DownloadJob>();
  const [starting, setStarting] = useState(false);
  const watching = useRef(false);
  const downloading = starting || job?.status === "downloading" || job?.status === "queued" || job?.status === "verifying";
  const selected = useMemo(
    () => probe?.variants.find((variant) => variant.id === variantId) ?? probe?.variants[0],
    [probe, variantId],
  );
  const card = probe?.card;
  const readme = useModelReadme(source, repo, probe?.revision, Boolean(probe));

  const notify = (text: string, failed = false) => {
    onNotice?.(text, failed);
  };

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setProbe(undefined);
    setVariantId("");
    void probeRemoteModel(source, repo)
      .then((next) => {
        if (cancelled) return;
        setProbe(next);
        setVariantId(next.defaultVariantId);
      })
      .catch((error) => {
        if (!cancelled) notify(invokeMessage(error), true);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [source, repo]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    const pick = (jobs: DownloadJob[]) => {
      const match = jobs.find((item) => item.repo === repo && (item.status === "downloading" || item.status === "queued" || item.status === "paused" || item.status === "verifying"));
      setJob(match);
      if (match) watching.current = true;
      const done = jobs.find((item) => item.repo === repo && item.status === "completed");
      if (done && watching.current) {
        watching.current = false;
        if (!embedded) {
          window.location.assign(modelCenterPath({ tab: "downloaded" }));
        }
      }
    };
    void listDownloadJobs().then(pick).catch(() => undefined);
    void listen<DownloadSnapshot>("model-download-snapshot", ({ payload }) => pick(payload.jobs)).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [repo, embedded, t]);

  const download = async () => {
    if (!probe || !selected || starting) return;
    setStarting(true);
    watching.current = true;
    try {
      await startModelDownloadFast(
        probe.source,
        probe.repo,
        probe.revision,
        selected.files
          .map((path) => probe.files.find((file) => file.path === path))
          .filter((file): file is NonNullable<typeof file> => Boolean(file)),
      );
      notify(t("modelDownloadStarted", { repo: probe.repo }));
    } catch (error) {
      notify(diskNotice(t, error), true);
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
      notify(invokeMessage(error), true);
    }
  };

  if (loading) {
    return <DetailSkeleton />;
  }

  if (!probe || !selected || !card) {
    return <p className={extra.readmeStatus}>{t("modelReadmeFailed")}</p>;
  }

  return (
    <div className={extra.detailCard}>
      <div className={extra.detailMeta}>
        <div className={extra.modelHead}>
          <ModelIdentity
            hit={{
              source: probe.source,
              repo: probe.repo,
              name: card.name || probe.repo,
              author: card.author,
              tags: card.tags,
              downloads: card.downloads,
              likes: card.likes,
              library: card.library,
              pipeline: card.pipeline,
              params: card.params,
              updatedAt: card.updatedAt,
            }}
            hubLink={{
              href: probe.source === "huggingface"
                ? `https://huggingface.co/${probe.repo}`
                : `https://www.modelscope.cn/models/${probe.repo}`,
              label: probe.source === "huggingface"
                ? t("modelOpenOnHuggingFace")
                : t("modelOpenOnModelScope"),
            }}
            variant="detail"
          />
          {card.tags.length || card.baseModel ? (
            <div className={extra.tags}>
              {card.tags.map((tag) => (
                <span className={extra.tag} key={tag}>{tag}</span>
              ))}
              {card.baseModel ? (
                <span className={`${extra.tag} ${extra.baseTag}`}>{t("modelBase", { repo: card.baseModel })}</span>
              ) : null}
            </div>
          ) : null}
        </div>
        <div className={extra.downloadBar}>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <button
                aria-label={t("modelWeightLabel")}
                className={extra.picker}
                disabled={downloading || probe.variants.length < 2}
                type="button"
              >
                <span className={extra.pickerMain}>
                  <VariantChip variant={selected} />
                </span>
                <span className={extra.pickerMeta}>
                  <span className={extra.format}>{variantFormat(selected)}</span>
                  <span className={extra.size}>{formatBytes(selected.size)}</span>
                  {probe.variants.length > 1 ? <ChevronDown aria-hidden="true" className={extra.chevron} size={16} /> : null}
                </span>
              </button>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content align="start" className={extra.pickerMenu} sideOffset={6}>
                {probe.variants.map((variant) => (
                  <DropdownMenu.Item
                    className={extra.pickerItem}
                    data-selected={variant.id === selected.id}
                    key={variant.id}
                    onSelect={() => setVariantId(variant.id)}
                  >
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
          {card.updatedAt ? (
            <span className={extra.stat}>
              <Calendar aria-hidden="true" size={13} />
              {t("modelStatUpdated")} {formatUpdated(card.updatedAt)}
            </span>
          ) : null}
          {card.downloads != null ? (
            <span className={extra.stat}>
              <Download aria-hidden="true" size={13} />
              {formatCount(card.downloads)}
            </span>
          ) : null}
          {card.likes != null ? (
            <span className={extra.stat}>
              <Heart aria-hidden="true" size={13} />
              {formatCount(card.likes)}
            </span>
          ) : null}
          {card.params ? (
            <span className={extra.stat}>
              <Cpu aria-hidden="true" size={13} />
              {card.params}
            </span>
          ) : null}
          {card.library ? (
            <span className={extra.stat}>
              <BookOpen aria-hidden="true" size={13} />
              {card.library}
            </span>
          ) : null}
          {card.license ? (
            <span className={extra.stat}>
              <FileText aria-hidden="true" size={13} />
              {card.license}
            </span>
          ) : null}
        </div>
      </div>
      <div className={extra.detailReadme}>
        <span className={extra.introLabel}>{t("modelReadme")}</span>
        {readme.loading ? <DetailSkeleton readmeOnly /> : null}
        {!readme.loading && readme.error ? <p className={extra.readmeStatus}>{t("modelReadmeFailed")}</p> : null}
        {!readme.loading && !readme.error && !readme.markdown ? (
          card.description?.trim() ? (
            <p className={extra.readmeStatus}>{card.description}</p>
          ) : (
            <p className={extra.readmeStatus}>{t("modelReadmeMissing")}</p>
          )
        ) : null}
        {!readme.loading && readme.markdown ? (
          <ModelReadme
            markdown={readme.markdown}
            repo={probe.repo}
            revision={probe.revision}
            source={probe.source}
            titleHint={card.name || probe.repo}
          />
        ) : null}
      </div>
    </div>
  );
}
