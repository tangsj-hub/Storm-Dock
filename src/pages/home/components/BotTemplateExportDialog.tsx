import { Tooltip } from "../../../components/Tooltip";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { save } from "@tauri-apps/plugin-dialog";
import { Check, CheckSquare, Clipboard, Download, Square, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { CopyIconButton } from "../../../components/CopyIconButton";
import exportStyles from "../../../components/ExportDialog.module.css";
import { exportBotTemplates, saveBotTemplatesFile } from "../../../lib/api";
import type { BotTemplate } from "../../../lib/botTemplates";
import styles from "./BotTemplateWorkspace.module.css";

type Props = {
  /** One template = single export (with its link); undefined = multi-select over `templates`. */
  single?: BotTemplate;
  templates: BotTemplate[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onError: (error: unknown) => void;
  onNotice: (notice: string) => void;
};

export function BotTemplateExportDialog({ single, templates, open, onOpenChange, onError, onNotice }: Props) {
  const { t } = useTranslation();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [json, setJson] = useState("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (open) setSelected(new Set(single ? [single.url] : templates.map((item) => item.url)));
  }, [open, single, templates]);

  // Keep list order regardless of click order.
  const urls = useMemo(() => templates.filter((item) => selected.has(item.url)).map((item) => item.url), [templates, selected]);

  useEffect(() => {
    if (!open || urls.length === 0) {
      setJson("");
      return;
    }
    let live = true;
    void exportBotTemplates(urls).then((text) => { if (live) setJson(text); }).catch(onError);
    return () => { live = false; };
  }, [open, urls, onError]);

  const allSelected = templates.length > 0 && selected.size === templates.length;
  const toggle = (url: string) => setSelected((current) => {
    const next = new Set(current);
    if (next.has(url)) next.delete(url);
    else next.add(url);
    return next;
  });

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(json);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch (error) {
      onError(error);
    }
  };

  const saveFile = async () => {
    try {
      const file = await save({
        defaultPath: single ? `bot-template-${single.name}.json` : "bot-templates.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
        title: t("botTemplateExportTitle"),
      });
      if (!file) return;
      await saveBotTemplatesFile(file, urls);
      onNotice(t("exported"));
    } catch (error) {
      onError(error);
    }
  };

  return (
    <AlertDialog.Root onOpenChange={onOpenChange} open={open}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay className={exportStyles.overlay} />
        <AlertDialog.Content className={exportStyles.content} onOpenAutoFocus={(event) => event.preventDefault()}>
          <div className={exportStyles.heading}>
            <div>
              <AlertDialog.Title>{single ? t("botTemplateExportOneTitle", { name: single.name }) : t("botTemplateExportTitle")}</AlertDialog.Title>
              <AlertDialog.Description>{t("botTemplateExportDescription")}</AlertDialog.Description>
            </div>
            <Tooltip content={t("close")}>
              <AlertDialog.Cancel asChild>
                <button aria-label={t("close")} className={exportStyles.close} type="button"><X aria-hidden="true" size={18} /></button>
              </AlertDialog.Cancel>
            </Tooltip>
          </div>
          {single ? (
            <div className={exportStyles.workosRow}>
              <span className={styles.exportLink}>{single.url}</span>
              <CopyIconButton label={t("botTemplateCopyLink")} onCopied={() => onNotice(t("copied"))} onError={onError} size={14} text={single.url} />
            </div>
          ) : (
            <div className={styles.exportPicker}>
              <button className={styles.exportPickRow} onClick={() => setSelected(allSelected ? new Set() : new Set(templates.map((item) => item.url)))} type="button">
                {allSelected ? <CheckSquare aria-hidden="true" size={16} /> : <Square aria-hidden="true" size={16} />}
                <strong>{t("sessionsSelectAll")}</strong>
                <span className={styles.exportCount}>{t("botTemplateExportSelected", { count: urls.length, total: templates.length })}</span>
              </button>
              <div className={styles.exportList}>
                {templates.map((item) => (
                  <button aria-pressed={selected.has(item.url)} className={styles.exportPickRow} key={item.url} onClick={() => toggle(item.url)} type="button">
                    {selected.has(item.url) ? <CheckSquare aria-hidden="true" size={16} /> : <Square aria-hidden="true" size={16} />}
                    <span>{item.name}</span>
                  </button>
                ))}
              </div>
            </div>
          )}
          <pre className={styles.exportJson}>{json || t("botTemplateExportNothing")}</pre>
          <div className={exportStyles.actions}>
            <button className={exportStyles.secondary} disabled={!json} onClick={() => void copy()} type="button">
              {copied ? <Check aria-hidden="true" size={16} /> : <Clipboard aria-hidden="true" size={16} />}
              {t(copied ? "copied" : "botTemplateCopyJson")}
            </button>
            <button className={exportStyles.primary} disabled={!json} onClick={() => void saveFile()} type="button">
              <Download aria-hidden="true" size={16} />
              {t("botTemplateSaveFile")}
            </button>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
