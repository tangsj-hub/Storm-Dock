import { listen } from "@tauri-apps/api/event";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as Progress from "@radix-ui/react-progress";
import { ChevronDown, ChevronUp, Download, Pause, Play, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { cancelModelDownload, dismissDownloadJob, listDownloadJobs, resumeDownloadJob } from "../lib/api";
import type { DownloadJob, DownloadSnapshot } from "../lib/types";
import styles from "./DownloadDock.module.css";

const RESUME_SEEN = "download-resume-seen";
const ACTIVE = new Set(["queued", "downloading", "verifying", "retry_wait"]);

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

function statusLabel(job: DownloadJob, t: (key: string) => string) {
  if (job.status === "queued") return t("downloadQueued");
  if (job.status === "paused") return t("downloadPaused");
  if (job.status === "retry_wait") return t("downloadRetryWait");
  if (job.status === "verifying") return t("downloadVerifying");
  if (job.status === "cancelled") return t("modelDownloadCancelled");
  if (job.status === "failed") return job.error || t("downloadError");
  if (job.status === "completed") return job.weaklyVerified ? t("downloadWeaklyVerified") : t("downloadVerified");
  return t("modelDownloading");
}

export function DownloadDock() {
  const { t } = useTranslation();
  const [jobs, setJobs] = useState<DownloadJob[]>([]);
  const [expanded, setExpanded] = useState(false);
  const [prompt, setPrompt] = useState<DownloadJob[]>([]);
  const dockRef = useRef<HTMLDivElement>(null);
  const resumeButtonRef = useRef<HTMLButtonElement>(null);
  const openedRef = useRef(false);

  const apply = useCallback((next: DownloadJob[]) => {
    setJobs(next);
    const live = next.filter((job) => ACTIVE.has(job.status) || job.status === "failed");
    if (live.length > 0 && !openedRef.current) {
      openedRef.current = true;
      setExpanded(true);
    }
    if (live.length === 0) openedRef.current = false;
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let poll: number | undefined;
    let polling = false;
    const refresh = async () => {
      if (polling) return;
      polling = true;
      try { apply(await listDownloadJobs()); } catch { /* event stream remains authoritative */ } finally { polling = false; }
    };
    void listDownloadJobs()
      .then((next) => {
        apply(next);
        const paused = next.filter((job) => job.status === "paused");
        if (paused.length > 0 && sessionStorage.getItem(RESUME_SEEN) !== "1") {
          setPrompt(paused);
        }
      })
      .catch(() => undefined);
    void listen<DownloadSnapshot>("model-download-snapshot", ({ payload }) => apply(payload.jobs)).then((fn) => {
      unlisten = fn;
    });
    poll = window.setInterval(() => void refresh(), 10000);
    return () => { unlisten?.(); if (poll !== undefined) window.clearInterval(poll); };
  }, [apply]);

  useEffect(() => {
    if (jobs.length === 0) {
      document.documentElement.style.setProperty("--download-dock-offset", "0px");
      return;
    }
    const node = dockRef.current;
    if (node) {
      document.documentElement.style.setProperty("--download-dock-offset", `${node.offsetHeight + 12}px`);
    }
    return () => document.documentElement.style.setProperty("--download-dock-offset", "0px");
  }, [jobs.length, expanded]);

  useEffect(() => {
    const done = jobs.filter((job) => job.status === "completed");
    if (done.length === 0) return;
    const timer = window.setTimeout(() => {
      for (const job of done) void dismissDownloadJob(job.jobId).catch(() => undefined);
    }, 4000);
    return () => window.clearTimeout(timer);
  }, [jobs]);

  if (jobs.length === 0 && prompt.length === 0) return null;

  const active = jobs.filter((job) => ACTIVE.has(job.status));
  const downloaded = jobs.reduce((sum, job) => sum + job.downloadedBytes, 0);
  const total = jobs.reduce((sum, job) => sum + job.totalBytes, 0);
  const percent = total === 0 ? 0 : Math.round((downloaded * 100) / total);

  const closePrompt = () => {
    sessionStorage.setItem(RESUME_SEEN, "1");
    setPrompt([]);
  };

  const resumeAll = async () => {
    const ids = prompt.map((job) => job.jobId);
    closePrompt();
    for (const id of ids) await resumeDownloadJob(id).catch(() => undefined);
  };

  return (
    <>
      {jobs.length > 0 ? (
        <div className={styles.dock} ref={dockRef}>
          <button aria-expanded={expanded} className={styles.pill} onClick={() => setExpanded((open) => !open)} type="button">
            <Download aria-hidden="true" size={15} />
            <span>
              {active.length > 0
                ? t("downloadDockActive", { count: active.length })
                : t("downloadDockTitle")}
            </span>
            {total > 0 ? <span className={styles.pillMeta}>{percent}%</span> : null}
            {expanded ? <ChevronDown aria-hidden="true" size={14} /> : <ChevronUp aria-hidden="true" size={14} />}
          </button>
          {expanded ? (
            <ul className={styles.list}>
              {jobs.map((job) => {
                const percent = job.totalBytes === 0 ? 0 : Math.min(100, (job.downloadedBytes / job.totalBytes) * 100);
                return (
                <li className={styles.row} key={job.jobId}>
                  <div className={styles.rowCopy}>
                    <strong>{job.repo}</strong>
                    <span>
                      {job.phase === "assembling" ? t("downloadAssembling") : job.phase === "verifying" ? `${t("downloadVerifying")} · ${formatBytes(job.phaseBytes)} / ${formatBytes(job.phaseTotalBytes)}` : statusLabel(job, t)}
                      {job.status === "downloading" && job.speedBps > 0 ? ` · ${t("modelSpeed", { speed: formatBytes(job.speedBps) })}` : ""}
                    </span>
                    {(job.totalBytes > 0 || job.phaseTotalBytes > 0) ? (
                      <Progress.Root aria-label={t("modelDownloading")} className={styles.bar} value={percent}>
                        <Progress.Indicator className={job.status === "failed" ? styles.barError : styles.barFill} style={{ transform: `translateX(-${100 - percent}%)` }} />
                      </Progress.Root>
                    ) : null}
                    <span className={styles.bytes}>{t("modelBytes", { downloaded: formatBytes(job.downloadedBytes), total: formatBytes(job.totalBytes) })}</span>
                  </div>
                  <div className={styles.rowActions}>
                    {job.status === "downloading" || job.status === "queued" ? (
                      <button aria-label={t("downloadPause")} className={styles.icon} onClick={() => void cancelModelDownload(job.jobId)} type="button">
                        <Pause aria-hidden="true" size={14} />
                      </button>
                    ) : null}
                    {job.status === "paused" || job.status === "cancelled" || job.status === "failed" || job.status === "retry_wait" ? (
                      <button aria-label={t("downloadResume")} className={styles.icon} onClick={() => void resumeDownloadJob(job.jobId)} type="button">
                        <Play aria-hidden="true" size={14} />
                      </button>
                    ) : null}
                    {job.status !== "downloading" && job.status !== "queued" && job.status !== "verifying" ? (
                      <button aria-label={t("downloadRemove")} className={styles.icon} onClick={() => void dismissDownloadJob(job.jobId)} type="button">
                        <X aria-hidden="true" size={14} />
                      </button>
                    ) : null}
                  </div>
                </li>
                );
              })}
            </ul>
          ) : null}
        </div>
      ) : null}
      <AlertDialog.Root onOpenChange={(open) => { if (!open) closePrompt(); }} open={prompt.length > 0}>
        <AlertDialog.Portal>
          <AlertDialog.Overlay className={styles.overlay} />
          <AlertDialog.Content className={styles.dialog} onOpenAutoFocus={(event) => { event.preventDefault(); const button = resumeButtonRef.current; if (button) { button.dataset.programmaticFocus = "true"; button.focus(); } }} onKeyDownCapture={() => { delete resumeButtonRef.current?.dataset.programmaticFocus; }}>
            <AlertDialog.Title>{t("downloadResumeTitle")}</AlertDialog.Title>
            <AlertDialog.Description>{t("downloadResumeDescription", { count: prompt.length })}</AlertDialog.Description>
            <div className={styles.dialogActions}>
              <AlertDialog.Cancel asChild>
                <button className={styles.secondary} type="button">{t("downloadResumeLater")}</button>
              </AlertDialog.Cancel>
              <AlertDialog.Action asChild>
                <button className={styles.primary} onClick={() => void resumeAll()} ref={resumeButtonRef} type="button">{t("downloadResumeAll")}</button>
              </AlertDialog.Action>
            </div>
          </AlertDialog.Content>
        </AlertDialog.Portal>
      </AlertDialog.Root>
    </>
  );
}
