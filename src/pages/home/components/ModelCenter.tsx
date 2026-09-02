import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors } from "@dnd-kit/core";
import { SortableContext, arrayMove, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { listen } from "@tauri-apps/api/event";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { ArrowRightLeft, FolderOpen, GripVertical, HardDrive, Trash2 } from "lucide-react";
import { memo, useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { deleteLocalModel, listLocalModels, migrateLocalModel, openLocalModelDir, refreshLocalModels, reorderLocalModels } from "../../../lib/api";
import type { DownloadJob, DownloadSnapshot, LocalLlm, ModelSource } from "../../../lib/types";
import { Tooltip } from "../../../components/Tooltip";
import { useLatestRequest } from "../hooks/useLatestRequest";
import styles from "../page.module.css";

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

function sameModels(left: LocalLlm[], right: LocalLlm[]) {
  return left.length === right.length && left.every((model, index) => {
    const other = right[index];
    return model.id === other.id && model.path === other.path && model.size === other.size && model.files === other.files && model.revision === other.revision;
  });
}

function ModelCardBody({
  model,
  onNotice,
  onRemove,
  onMigrate,
}: {
  model: LocalLlm;
  onNotice: (message: string, status?: "success" | "error") => void;
  onRemove?: (model: LocalLlm) => void;
  onMigrate?: (model: LocalLlm, target: ModelSource) => void;
}) {
  const { t } = useTranslation();
  return (
    <>
      <div className={styles.accountCopy}>
        <strong>{model.repo}</strong>
        <div className={styles.accountMeta}>
          <span className={`${styles.kindBadge} ${styles.kindAccount}`}>
            {t(model.source === "modelscope" ? "modelSourceModelScope" : "modelSourceHuggingFace")}
          </span>
          {model.revision ? <span className={styles.metaBadge}>{model.revision}</span> : null}
          <span className={styles.metaBadge}>{t("modelFiles", { count: model.files, size: formatBytes(model.size) })}</span>
        </div>
      </div>
      <div className={styles.accountActions}>
        {model.path ? (
          <>
            <Tooltip content={t("modelOpenFolder")}>
              <button aria-label={t("modelOpenFolder")} className={styles.iconButton} onClick={() => void openLocalModelDir(model.id).catch((error) => onNotice(String(error), "error"))} type="button">
                <FolderOpen aria-hidden="true" size={18} />
              </button>
            </Tooltip>
            {onMigrate ? (
              <DropdownMenu.Root>
                <Tooltip content={t("modelMigrate")}>
                  <DropdownMenu.Trigger asChild>
                    <button aria-label={t("modelMigrate")} className={styles.iconButton} onClick={(event) => event.currentTarget.blur()} type="button">
                      <ArrowRightLeft aria-hidden="true" size={18} />
                    </button>
                  </DropdownMenu.Trigger>
                </Tooltip>
                <DropdownMenu.Portal>
                  <DropdownMenu.Content align="end" className={styles.modelMenu} onCloseAutoFocus={(event) => event.preventDefault()} sideOffset={6}>
                    {(["huggingface", "modelscope"] as ModelSource[]).filter((target) => target !== model.source).map((target) => (
                      <DropdownMenu.Item className={styles.modelMenuItem} key={target} onSelect={() => onMigrate(model, target)}>
                        {t("modelMigrateTo", { target: t(target === "huggingface" ? "modelSourceHuggingFace" : "modelSourceModelScope") })}
                      </DropdownMenu.Item>
                    ))}
                  </DropdownMenu.Content>
                </DropdownMenu.Portal>
              </DropdownMenu.Root>
            ) : null}
            {onRemove ? (
              <Tooltip content={t("delete")}>
                <button aria-label={t("remove", { account: model.repo })} className={styles.iconButton} onClick={() => onRemove(model)} type="button">
                  <Trash2 aria-hidden="true" size={19} />
                </button>
              </Tooltip>
            ) : null}
          </>
        ) : null}
      </div>
    </>
  );
}

function SortableModel(props: {
  model: LocalLlm;
  onNotice: (message: string, status?: "success" | "error") => void;
  onRemove: (model: LocalLlm) => void;
  onMigrate: (model: LocalLlm, target: ModelSource) => void;
}) {
  const { t } = useTranslation();
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: props.model.id });
  return (
    <article className={`${styles.accountCard} ${isDragging ? styles.dragging : ""}`} ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition }}>
      <GripVertical aria-label={t("drag", { account: props.model.repo })} className={styles.dragHandle} size={24} {...attributes} {...listeners} />
      <ModelCardBody {...props} />
    </article>
  );
}

export const ModelCenter = memo(function ModelCenter({
  refreshKey,
  onBusyChange,
  onNotice,
}: {
  refreshKey: number;
  onBusyChange?: (busy: boolean) => void;
  onNotice: (message: string, status?: "success" | "error") => void;
}) {
  const { t } = useTranslation();
  const [models, setModels] = useState<LocalLlm[]>([]);
  const [pendingDelete, setPendingDelete] = useState<LocalLlm>();
  const [pendingMigration, setPendingMigration] = useState<{ model: LocalLlm; target: ModelSource }>();
  const onNoticeRef = useRef(onNotice);
  const onBusyRef = useRef(onBusyChange);
  const beginRequest = useLatestRequest();
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }), useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }));
  onNoticeRef.current = onNotice;
  onBusyRef.current = onBusyChange;

  const applyModels = useCallback((next: LocalLlm[]) => {
    setModels((current) => (sameModels(current, next) ? current : next));
  }, []);

  const noticeError = useCallback((error: unknown) => {
    onNoticeRef.current(error instanceof Error ? error.message : String(error), "error");
  }, []);

  useEffect(() => {
    const isCurrent = beginRequest();
    onBusyRef.current?.(true);
    void (async () => {
      try {
        applyModels(await listLocalModels());
        if (!isCurrent()) return;
        applyModels(await refreshLocalModels());
      } catch (error) {
        if (isCurrent()) noticeError(error);
      } finally {
        if (isCurrent()) onBusyRef.current?.(false);
      }
    })();
  }, [applyModels, beginRequest, noticeError, refreshKey]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<DownloadSnapshot>("model-download-snapshot", ({ payload }) => {
      if (payload.jobs.some((job) => job.status === "completed")) {
        void refreshLocalModels().then(applyModels).catch(noticeError);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [applyModels, noticeError]);

  const confirmDelete = async () => {
    if (!pendingDelete) return;
    const target = pendingDelete;
    setPendingDelete(undefined);
    try {
      await deleteLocalModel(target.id);
      onNotice(t("modelDeleted", { repo: target.repo }));
      applyModels(await listLocalModels());
    } catch (error) {
      noticeError(error);
    }
  };

  const confirmMigration = async () => {
    if (!pendingMigration) return;
    const { model, target } = pendingMigration;
    setPendingMigration(undefined);
    try {
      await migrateLocalModel(model.id, target);
      onNotice(t("modelMigrated", { repo: model.repo, target: t(target === "huggingface" ? "modelSourceHuggingFace" : "modelSourceModelScope") }));
      applyModels(await refreshLocalModels());
    } catch (error) {
      noticeError(error);
    }
  };

  const reorder = async (activeId: string, targetId?: string) => {
    (document.activeElement as HTMLElement | null)?.blur();
    if (!targetId || activeId === targetId) return;
    const from = models.findIndex((model) => model.id === activeId);
    const to = models.findIndex((model) => model.id === targetId);
    if (from < 0 || to < 0) return;
    const next = arrayMove(models, from, to);
    applyModels(next);
    try {
      await reorderLocalModels(next.map((model) => model.id));
      onNotice(t("modelReordered"));
    } catch (error) {
      noticeError(error);
      try {
        applyModels(await listLocalModels());
      } catch (reloadError) {
        noticeError(reloadError);
      }
    }
  };

  return (
    <>
      {models.length === 0 ? (
        <div className={styles.empty}>
          <HardDrive aria-hidden="true" size={32} />
          <h2>{t("modelLocalEmptyTitle")}</h2>
          <p>{t("modelLocalEmptyDescription")}</p>
        </div>
      ) : (
        <div className={styles.accountList}>
          <DndContext collisionDetection={closestCenter} onDragEnd={({ active, over }) => void reorder(String(active.id), over ? String(over.id) : undefined)} sensors={sensors}>
            <SortableContext items={models.map((model) => model.id)} strategy={verticalListSortingStrategy}>
              {models.map((model) => (
                <SortableModel key={model.id} model={model} onMigrate={(candidate, target) => setPendingMigration({ model: candidate, target })} onNotice={onNotice} onRemove={setPendingDelete} />
              ))}
            </SortableContext>
          </DndContext>
        </div>
      )}
      <AlertDialog.Root onOpenChange={(open) => { if (!open) setPendingDelete(undefined); }} open={Boolean(pendingDelete)}>
        <AlertDialog.Portal>
          <AlertDialog.Overlay className={styles.dialogOverlay} />
          <AlertDialog.Content className={styles.dialogContent}>
            <AlertDialog.Title>{t("modelDeleteConfirmTitle")}</AlertDialog.Title>
            <AlertDialog.Description>{t("modelDeleteConfirm", { repo: pendingDelete?.repo })}</AlertDialog.Description>
            <div className={styles.dialogActions}>
              <AlertDialog.Cancel asChild>
                <button className={styles.dialogCancel} type="button">{t("cancel")}</button>
              </AlertDialog.Cancel>
              <AlertDialog.Action asChild>
                <button autoFocus className={styles.dialogConfirm} onClick={() => void confirmDelete()} type="button">{t("delete")}</button>
              </AlertDialog.Action>
            </div>
          </AlertDialog.Content>
        </AlertDialog.Portal>
      </AlertDialog.Root>
      <AlertDialog.Root onOpenChange={(open) => { if (!open) setPendingMigration(undefined); }} open={Boolean(pendingMigration)}>
        <AlertDialog.Portal>
          <AlertDialog.Overlay className={styles.dialogOverlay} />
          <AlertDialog.Content className={styles.dialogContent}>
            <AlertDialog.Title>{t("modelMigrateConfirmTitle")}</AlertDialog.Title>
            <AlertDialog.Description>{t("modelMigrateConfirm", { repo: pendingMigration?.model.repo, target: pendingMigration ? t(pendingMigration.target === "huggingface" ? "modelSourceHuggingFace" : "modelSourceModelScope") : "" })}</AlertDialog.Description>
            <div className={styles.dialogActions}>
              <AlertDialog.Cancel asChild><button className={styles.dialogCancel} type="button">{t("cancel")}</button></AlertDialog.Cancel>
              <AlertDialog.Action asChild><button autoFocus className={styles.dialogPrimary} onClick={() => void confirmMigration()} type="button">{t("modelMigrate")}</button></AlertDialog.Action>
            </div>
          </AlertDialog.Content>
        </AlertDialog.Portal>
      </AlertDialog.Root>
    </>
  );
});
