import { useTranslation } from "react-i18next";
import { CopyIconButton } from "../../components/CopyIconButton";
import { ModelVendorIcon } from "../../components/ModelVendorIcon";
import { isGgufHit } from "../../lib/modelHits";
import { splitModelRepo } from "../../lib/providerLogos";
import type { RemoteModelHit } from "../../lib/types";
import extra from "./page.module.css";

export type ModelIdentityVariant = "card" | "compact" | "split";

const VARIANT = {
  card: { avatar: 52, titleClass: extra.hitTitleCard },
  compact: { avatar: 36, titleClass: extra.hitTitleCompact },
  split: { avatar: 32, titleClass: extra.hitTitleSplit },
} as const;

export function ModelIdentity({
  hit,
  variant = "compact",
  avatarSize,
}: {
  hit: RemoteModelHit;
  /** @deprecated Prefer `variant`. Kept so dense callers still compile during transition. */
  dense?: boolean;
  variant?: ModelIdentityVariant;
  avatarSize?: number;
}) {
  const { t } = useTranslation();
  const { owner, repoName } = splitModelRepo(hit.repo);
  const meta = VARIANT[variant];
  const size = avatarSize ?? meta.avatar;
  const dense = variant === "card" || variant === "split";
  const modelId = (hit.repo || hit.name || "").trim();
  const vendor = (hit.author || owner || "").trim();
  const displayName = (hit.name || repoName || hit.repo || "").trim();

  return (
    <div className={dense ? extra.hitModelDense : extra.hitModel}>
      <ModelVendorIcon
        name={hit.name || hit.repo}
        owner={hit.author || owner}
        repoName={repoName}
        size={size}
      />
      <div className={extra.hitCopy}>
        <div className={extra.hitTitleRow}>
          <strong className={`${extra.hitTitle} ${meta.titleClass}`}>{displayName}</strong>
          {isGgufHit(hit) ? <span aria-label="GGUF" className={extra.hitGguf} /> : null}
          <CopyIconButton
            className={extra.hitIdentityCopy}
            label={t("copyModelId")}
            size={variant === "split" ? 12 : 13}
            text={modelId}
          />
        </div>
        {vendor ? (
          <div className={extra.hitIdRow}>
            <span className={extra.hitAuthor} title={vendor}>
              {vendor}
            </span>
          </div>
        ) : null}
      </div>
    </div>
  );
}
