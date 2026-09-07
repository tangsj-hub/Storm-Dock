import { Search, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ModelFormat, ModelSource } from "../../lib/types";
import styles from "../add/page.module.css";
import extra from "./page.module.css";

const FORMATS: ModelFormat[] = ["all", "gguf", "safetensors", "mlx", "finetune"];

function formatLabelKey(format: ModelFormat) {
  if (format === "gguf") return "modelFormatGguf";
  if (format === "safetensors") return "modelFormatSafetensors";
  if (format === "mlx") return "modelFormatMlx";
  if (format === "finetune") return "modelFormatFinetune";
  return "modelFormatAll";
}

export function DiscoverToolbar({
  draft,
  source,
  format,
  busy,
  onDraftChange,
  onSourceChange,
  onFormatChange,
  onClearSearch,
}: {
  draft: string;
  source: ModelSource;
  format: ModelFormat;
  busy: boolean;
  onDraftChange: (value: string) => void;
  onSourceChange: (value: ModelSource) => void;
  onFormatChange: (value: ModelFormat) => void;
  onClearSearch: () => void;
}) {
  const { t } = useTranslation();
  const canClear = draft.trim().length > 0;

  return (
    <div className={extra.field}>
      <div className={extra.idRow}>
        <div className={extra.searchField}>
          <input
            aria-label={t("modelRepoPlaceholder")}
            autoComplete="off"
            className={extra.modelIdInput}
            disabled={busy}
            onChange={(event) => onDraftChange(event.target.value)}
            placeholder={t("modelRepoPlaceholder")}
            spellCheck={false}
            value={draft}
          />
          {canClear ? (
            <button
              aria-label={t("modelSearchClear")}
              className={extra.searchClear}
              disabled={busy}
              onClick={onClearSearch}
              type="button"
            >
              <X aria-hidden="true" size={14} strokeWidth={2} />
            </button>
          ) : null}
        </div>
        <select
          aria-label={t("modelFormatLabel")}
          className={extra.sourceSelect}
          disabled={busy}
          onChange={(event) => onFormatChange(event.target.value as ModelFormat)}
          value={format}
        >
          {FORMATS.map((item) => (
            <option key={item} value={item}>{t(formatLabelKey(item))}</option>
          ))}
        </select>
        <select
          aria-label={t("modelSourceLabel")}
          className={extra.sourceSelect}
          onChange={(event) => onSourceChange(event.target.value as ModelSource)}
          value={source}
        >
          <option value="modelscope">{t("modelSourceModelScope")}</option>
          <option value="huggingface">{t("modelSourceHuggingFace")}</option>
        </select>
        <button className={styles.primary} disabled={busy} type="submit">
          <Search aria-hidden="true" size={16} />
          {busy ? t("modelProbing") : t("modelProbe")}
        </button>
      </div>
      {source === "huggingface" ? (
        <p className={extra.sourceHint}>
          {t("modelUseHuggingFaceHint")}{" "}
          <a href="/settings.html?tab=huggingface">{t("hfOpenSettings")}</a>
        </p>
      ) : null}
    </div>
  );
}
