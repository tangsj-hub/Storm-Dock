import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors } from "@dnd-kit/core";
import { SortableContext, arrayMove, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { ExternalLink, FileOutput, FileUp, GripVertical, Import, Pencil, LayoutTemplate, LoaderCircle, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Switch } from "../../../components/Switch";
import { Tooltip } from "../../../components/Tooltip";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { BotTemplateExportDialog } from "./BotTemplateExportDialog";
import { addBotTemplate, importBotTemplatesJson, previewBotTemplatesImport, readBotTemplatesFile, deleteBotTemplate, fetchBotTemplateMeta, getBotImportGate, importBotTemplateInClient, listBotTemplates, openExternalUrl, reorderBotTemplates, updateBotTemplate } from "../../../lib/api";
import { isBotTemplateUrl, isDescriptionTruncated, isExportable, truncateDescription, type BotTemplate } from "../../../lib/botTemplates";
import pageStyles from "../page.module.css";
import styles from "./BotTemplateWorkspace.module.css";

type Props = {
  exportOpen: boolean;
  onExportOpenChange: (open: boolean) => void;
  addOpen: boolean;
  onAddOpenChange: (open: boolean) => void;
  onError: (error: unknown) => void;
  onNotice: (notice: string) => void;
};

function SortableTemplate({ template, onImport, onOpen, onEdit, onRemove, onExport }: { template: BotTemplate; onImport: () => void; onOpen: () => void; onEdit: () => void; onRemove: () => void; onExport: () => void }) {
  const { t } = useTranslation();
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: template.url });
  const description = template.description?.trim() ?? "";
  const truncated = isDescriptionTruncated(description);
  return (
    <article
      className={`${pageStyles.accountCard} ${isDragging ? pageStyles.dragging : ""}`}
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
    >
      <GripVertical aria-label={t("drag", { account: template.name })} className={pageStyles.dragHandle} size={24} {...attributes} {...listeners} />
      <div className={pageStyles.accountCopy}>
        <div className={styles.titleRow}>
          <strong>{template.name}{template.note ? <span className={styles.note}> · {template.note}</span> : null}</strong>
          {template.builtinId ? <span className={`${pageStyles.metaBadge} ${pageStyles.planDefault} ${styles.autoTag}`}>{t("botTemplateBuiltinTag")}</span> : null}
          {template.autoImport ? <span className={`${pageStyles.metaBadge} ${pageStyles["plan-free"]} ${styles.autoTag}`}>{t("botTemplateAutoTag")}</span> : null}
        </div>
        <div className={pageStyles.accountMeta}>
          {!description ? (
            <span className={styles.description} data-empty="true">{t("botTemplateNoDescription")}</span>
          ) : truncated ? (
            <Tooltip content={description}>
              <span className={styles.description} tabIndex={0}>{truncateDescription(description)}</span>
            </Tooltip>
          ) : (
            <span className={styles.description}>{description}</span>
          )}
        </div>
      </div>
      <div className={pageStyles.accountActions}>
        <Tooltip content={t("botTemplateImport")}>
          <button aria-label={t("botTemplateImport")} className={pageStyles.iconButton} onClick={onImport} type="button"><Import aria-hidden="true" size={18} /></button>
        </Tooltip>
        {!template.builtinId && (
          <Tooltip content={t("botTemplateExport")}>
            <button aria-label={t("botTemplateExport")} className={pageStyles.iconButton} onClick={onExport} type="button"><FileOutput aria-hidden="true" size={18} /></button>
          </Tooltip>
        )}
        <Tooltip content={t("botTemplateOpen")}>
          <button aria-label={t("botTemplateOpen")} className={pageStyles.iconButton} onClick={onOpen} type="button"><ExternalLink aria-hidden="true" size={18} /></button>
        </Tooltip>
        <Tooltip content={t("botTemplateEdit")}>
          <button aria-label={t("botTemplateEdit")} className={pageStyles.iconButton} onClick={onEdit} type="button"><Pencil aria-hidden="true" size={17} /></button>
        </Tooltip>
        <Tooltip content={t("delete")}>
          <button aria-label={t("remove", { account: template.name })} className={pageStyles.iconButton} onClick={onRemove} type="button"><Trash2 aria-hidden="true" size={19} /></button>
        </Tooltip>
      </div>
    </article>
  );
}

export function BotTemplateWorkspace({ addOpen, exportOpen, onAddOpenChange, onExportOpenChange, onError, onNotice }: Props) {
  const { t } = useTranslation();
  const [templates, setTemplates] = useState<BotTemplate[]>([]);
  const [url, setUrl] = useState("");
  const [manual, setManual] = useState<{ reason: string } | undefined>();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [saving, setSaving] = useState(false);
  const [editing, setEditing] = useState<BotTemplate | undefined>();
  const [note, setNote] = useState("");
  const [autoImport, setAutoImport] = useState(false);
  const dialogOpen = addOpen || editing !== undefined;
  const [mode, setMode] = useState<"link" | "json">("link");
  const [jsonText, setJsonText] = useState("");
  const [jsonPreview, setJsonPreview] = useState<{ total: number; newCount: number; existing: number } | { error: string } | undefined>();
  const [exportOne, setExportOne] = useState<BotTemplate | undefined>();
  const exportableTemplates = useMemo(() => templates.filter(isExportable), [templates]);

  useEffect(() => {
    if (mode !== "json" || !jsonText.trim()) {
      setJsonPreview(undefined);
      return;
    }
    let live = true;
    const timer = window.setTimeout(() => {
      void previewBotTemplatesImport(jsonText)
        .then((preview) => { if (live) setJsonPreview(preview); })
        .catch((error) => { if (live) setJsonPreview({ error: String(error) }); });
    }, 250);
    return () => { live = false; window.clearTimeout(timer); };
  }, [mode, jsonText, templates]);

  const pickJsonFile = async () => {
    try {
      const file = await openFileDialog({ filters: [{ name: "JSON", extensions: ["json"] }], multiple: false, title: t("botTemplateJsonPick") });
      if (typeof file === "string") setJsonText(await readBotTemplatesFile(file));
    } catch (error) {
      onError(error);
    }
  };

  const startEdit = (template: BotTemplate) => {
    setEditing(template);
    setUrl(template.url);
    setName(template.name);
    setDescription(template.description ?? "");
    setNote(template.note ?? "");
    setAutoImport(Boolean(template.autoImport));
    setManual(undefined);
  };
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  useEffect(() => {
    void listBotTemplates().then(setTemplates).catch(onError);
  }, [onError]);

  const reset = () => {
    setUrl("");
    setName("");
    setDescription("");
    setNote("");
    setAutoImport(false);
    setManual(undefined);
    setEditing(undefined);
    setMode("link");
    setJsonText("");
    setJsonPreview(undefined);
  };

  const close = () => {
    if (saving) return;
    reset();
    onAddOpenChange(false);
  };

  const save = async (templateName: string, templateDescription?: string) => {
    setTemplates(await addBotTemplate(url, templateName, templateDescription, note.trim() || undefined, autoImport));
    onNotice(t("botTemplateAdded", { name: templateName }));
    reset();
    onAddOpenChange(false);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (saving) return;
    if (!editing && mode === "json") {
      setSaving(true);
      try {
        const result = await importBotTemplatesJson(jsonText);
        setTemplates(result.templates);
        onNotice(t("botTemplateJsonImported", { added: result.added, skipped: result.skipped }));
        reset();
        onAddOpenChange(false);
      } catch (error) {
        onError(error);
      } finally {
        setSaving(false);
      }
      return;
    }
    if (editing) {
      if (!name.trim()) {
        onError(t("botTemplateNameRequired"));
        return;
      }
      setSaving(true);
      try {
        setTemplates(await updateBotTemplate(editing.url, { name: name.trim(), description: description.trim() || undefined, note: note.trim() || undefined, autoImport }));
        onNotice(t("botTemplateUpdated", { name: name.trim() }));
        reset();
      } catch (error) {
        onError(error);
      } finally {
        setSaving(false);
      }
      return;
    }
    if (!isBotTemplateUrl(url)) {
      onError(t("botTemplateInvalidUrl"));
      return;
    }
    setSaving(true);
    try {
      if (manual) {
        if (!name.trim()) {
          onError(t("botTemplateNameRequired"));
          return;
        }
        await save(name.trim(), description.trim() || undefined);
        return;
      }
      let meta: { name?: string | null; description?: string | null };
      try {
        meta = await fetchBotTemplateMeta(url);
      } catch (error) {
        setManual({ reason: String(error) });
        return;
      }
      if (!meta.name) {
        setManual({ reason: t("botTemplateMetaMissing") });
        return;
      }
      await save(meta.name, meta.description ?? undefined);
    } catch (error) {
      onError(error);
    } finally {
      setSaving(false);
    }
  };

  const reorder = async (activeUrl: string, overUrl?: string) => {
    if (!overUrl || activeUrl === overUrl) return;
    const from = templates.findIndex((item) => item.url === activeUrl);
    const to = templates.findIndex((item) => item.url === overUrl);
    if (from < 0 || to < 0) return;
    const previous = templates;
    const next = arrayMove(templates, from, to);
    setTemplates(next);
    try {
      setTemplates(await reorderBotTemplates(next.map((item) => item.url)));
    } catch (error) {
      setTemplates(previous);
      onError(error);
    }
  };

  const remove = async (template: BotTemplate) => {
    try {
      setTemplates(await deleteBotTemplate(template.url));
    } catch (error) {
      onError(error);
    }
  };

  const importToClient = async (template: BotTemplate) => {
    try {
      const gate = await getBotImportGate();
      if (!gate.supported) {
        onError(t("botImportNeedsVersion", { required: gate.required, current: gate.current || t("botImportVersionUnknown") }));
        return;
      }
      const openedIn = await importBotTemplateInClient(template.url);
      onNotice(t(openedIn === "client" ? "botTemplateImportOpened" : "botTemplateImportFallback", { name: template.name }));
    } catch (error) {
      onError(error);
    }
  };

  const open = async (template: BotTemplate) => {
    try {
      await openExternalUrl(template.url);
    } catch (error) {
      onError(error);
    }
  };

  return (
    <>
      {templates.length === 0 ? (
        <div className={pageStyles.empty}>
          <LayoutTemplate aria-hidden="true" size={32} />
          <h2>{t("botTemplatesEmptyTitle")}</h2>
          <p>{t("botTemplatesEmptyDescription")}</p>
        </div>
      ) : (
        <DndContext collisionDetection={closestCenter} onDragEnd={({ active, over }) => void reorder(String(active.id), over ? String(over.id) : undefined)} sensors={sensors}>
          <SortableContext items={templates.map((item) => item.url)} strategy={verticalListSortingStrategy}>
            <div className={pageStyles.accountList}>
              {templates.map((template) => (
                <SortableTemplate key={template.url} onImport={() => void importToClient(template)} onEdit={() => startEdit(template)} onExport={() => setExportOne(template)} onOpen={() => void open(template)} onRemove={() => void remove(template)} template={template} />
              ))}
            </div>
          </SortableContext>
        </DndContext>
      )}
      <AlertDialog.Root onOpenChange={(next) => { if (!next) close(); }} open={dialogOpen}>
        <AlertDialog.Portal>
          <AlertDialog.Overlay className={pageStyles.dialogOverlay} />
          <AlertDialog.Content className={pageStyles.dialogContent}>
            <form className={styles.form} onSubmit={(event) => void submit(event)}>
              <AlertDialog.Title>{t(editing ? "botTemplateEditTitle" : "botTemplateAddTitle")}</AlertDialog.Title>
              <AlertDialog.Description className={styles.hint}>{editing ? t("botTemplateEditDescription") : manual ? t("botTemplateManualHint", { reason: manual.reason }) : t("botTemplateAddDescription")}</AlertDialog.Description>
              {!editing && (
                <div className={styles.modeTabs} role="group">
                  <button aria-pressed={mode === "link"} disabled={saving} onClick={() => setMode("link")} type="button">{t("botTemplateModeLink")}</button>
                  <button aria-pressed={mode === "json"} disabled={saving} onClick={() => setMode("json")} type="button">{t("botTemplateModeJson")}</button>
                </div>
              )}
              {!editing && mode === "json" && <>
                <textarea aria-label={t("botTemplateModeJson")} className={styles.jsonInput} disabled={saving} onChange={(event) => setJsonText(event.target.value)} placeholder={t("botTemplateJsonPlaceholder")} spellCheck={false} value={jsonText} />
                <div className={styles.jsonRow}>
                  <span className={styles.jsonPreview} data-error={jsonPreview && "error" in jsonPreview ? "true" : undefined}>
                    {!jsonPreview ? t("botTemplateJsonHint") : "error" in jsonPreview ? jsonPreview.error : t("botTemplateJsonPreview", { total: jsonPreview.total, added: jsonPreview.newCount, skipped: jsonPreview.existing })}
                  </span>
                  <button className={styles.fileButton} disabled={saving} onClick={() => void pickJsonFile()} type="button"><FileUp aria-hidden="true" size={14} />{t("botTemplateJsonPick")}</button>
                </div>
              </>}
              {(editing || mode === "link") && <label className={styles.field}>{t("botTemplateUrl")}
                <input autoFocus={!editing} disabled={saving} readOnly={Boolean(editing)} onChange={(event) => { setUrl(event.target.value); if (manual) setManual(undefined); }} placeholder="https://x.ai/bot/xxxx" spellCheck={false} value={url} />
              </label>}
              {editing && <>
                <label className={styles.field}>{t("botTemplateName")}
                  <input autoFocus disabled={saving} onChange={(event) => setName(event.target.value)} value={name} />
                </label>
                <label className={styles.field}>{t("botTemplateDescriptionOptional")}
                  <textarea disabled={saving} onChange={(event) => setDescription(event.target.value)} rows={3} value={description} />
                </label>
                <label className={styles.field}>{t("botTemplateNoteOptional")}
                  <input disabled={saving} onChange={(event) => setNote(event.target.value)} value={note} />
                </label>
                <div className={styles.switchField}>
                  <span>{t("botTemplateAutoImport")}<small>{t("botTemplateAutoImportHint")}</small></span>
                  <Switch checked={autoImport} disabled={saving} label={t("botTemplateAutoImport")} onChange={setAutoImport} />
                </div>
              </>}
              {!editing && mode === "link" && <>
                {manual && <>
                <label className={styles.field}>{t("botTemplateName")}
                  <input disabled={saving} onChange={(event) => setName(event.target.value)} value={name} />
                </label>
                <label className={styles.field}>{t("botTemplateDescriptionOptional")}
                  <input disabled={saving} onChange={(event) => setDescription(event.target.value)} value={description} />
                </label>
              </>}
                <label className={styles.field}>{t("botTemplateNoteOptional")}
                  <input disabled={saving} onChange={(event) => setNote(event.target.value)} value={note} />
                </label>
                <div className={styles.switchField}>
                  <span>{t("botTemplateAutoImport")}<small>{t("botTemplateAutoImportHint")}</small></span>
                  <Switch checked={autoImport} disabled={saving} label={t("botTemplateAutoImport")} onChange={setAutoImport} />
                </div>
              </>}
              <div className={pageStyles.dialogActions}>
                <button className={pageStyles.dialogCancel} disabled={saving} onClick={close} type="button">{t("cancel")}</button>
                <button className={pageStyles.dialogPrimary} disabled={saving || (!editing && mode === "json" ? !jsonPreview || "error" in jsonPreview || jsonPreview.newCount === 0 : !url.trim() || ((manual !== undefined || editing !== undefined) && !name.trim()))} type="submit">
                  {saving ? <LoaderCircle aria-hidden="true" className={pageStyles.spinning} size={14} /> : null}
                  {!editing && mode === "json" ? t("botTemplateJsonImport") : manual || editing ? t("botTemplateSave") : t("botTemplateAdd")}
                </button>
              </div>
            </form>
          </AlertDialog.Content>
        </AlertDialog.Portal>
      </AlertDialog.Root>
      <BotTemplateExportDialog
        onError={onError}
        onNotice={onNotice}
        onOpenChange={(next) => { if (!next) { setExportOne(undefined); onExportOpenChange(false); } }}
        open={exportOne !== undefined || exportOpen}
        single={exportOne}
        templates={exportableTemplates}
      />
    </>
  );
}
