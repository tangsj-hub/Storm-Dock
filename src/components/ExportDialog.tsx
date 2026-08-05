import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { Check, Clipboard, Download, X } from "lucide-react";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import styles from "./ExportDialog.module.css";
import { Tooltip } from "./Tooltip";

type ExportDialogProps = { data: unknown; filename: string; onOpenChange: (open: boolean) => void; open: boolean };

export function ExportDialog({ data, filename, onOpenChange, open }: ExportDialogProps) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const copyButtonRef = useRef<HTMLButtonElement>(null);
  const json = JSON.stringify(data, null, 2);
  const copy = async () => {
    await navigator.clipboard.writeText(json);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };
  const download = () => {
    const url = URL.createObjectURL(new Blob([json], { type: "application/json" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = filename;
    link.click();
    URL.revokeObjectURL(url);
  };
  return <AlertDialog.Root onOpenChange={onOpenChange} open={open}><AlertDialog.Portal>
    <AlertDialog.Overlay className={styles.overlay} />
    <AlertDialog.Content className={styles.content} onOpenAutoFocus={(event) => { event.preventDefault(); copyButtonRef.current?.focus(); }}>
      <div className={styles.heading}><div><AlertDialog.Title>{t("exportTitle")}</AlertDialog.Title><AlertDialog.Description>{t("exportDescription")}</AlertDialog.Description></div><Tooltip content={t("close")}><AlertDialog.Cancel asChild><button aria-label={t("close")} className={styles.close} onPointerDown={(event) => event.preventDefault()} type="button"><X aria-hidden="true" size={18} /></button></AlertDialog.Cancel></Tooltip></div>
      <pre className={styles.preview}>{json}</pre>
      <div className={styles.actions}><button className={styles.secondary} onClick={() => void copy()} ref={copyButtonRef} type="button">{copied ? <Check aria-hidden="true" size={16} /> : <Clipboard aria-hidden="true" size={16} />}{t(copied ? "copied" : "copy")}</button><button className={styles.primary} onClick={download} type="button"><Download aria-hidden="true" size={16} />{t("download")}</button></div>
    </AlertDialog.Content>
  </AlertDialog.Portal></AlertDialog.Root>;
}
