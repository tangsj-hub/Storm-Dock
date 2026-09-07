import { ExternalLink } from "lucide-react";
import { useTranslation } from "react-i18next";
import { CopyIconButton } from "../../components/CopyIconButton";
import { ModelVendorIcon } from "../../components/ModelVendorIcon";
import { Tooltip } from "../../components/Tooltip";
import { isGgufHit } from "../../lib/modelHits";
import { splitModelRepo } from "../../lib/providerLogos";
import type { RemoteModelHit } from "../../lib/types";
import extra from "./page.module.css";

export type ModelIdentityVariant = "card" | "compact" | "split" | "detail";

const VARIANT = {
  card: { avatar: 52, titleClass: extra.hitTitleCard },
  compact: { avatar: 36, titleClass: extra.hitTitleCompact },
  split: { avatar: 32, titleClass: extra.hitTitleSplit },
  detail: { avatar: 40, titleClass: extra.hitTitleDetail },
} as const;

export function ModelIdentity({
  hit,
  variant = "compact",
  avatarSize,
  hubLink,
}: {
  hit: RemoteModelHit;
  /** @deprecated Prefer `variant`. Kept so dense callers still compile during transition. */
  dense?: boolean;
  variant?: ModelIdentityVariant;
  avatarSize?: number;
  /** Detail-only: icon that opens the official Hub page. */
  hubLink?: { href: string; label: string };
}) {
  const { t } = useTranslation();
  const { owner, repoName } = splitModelRepo(hit.repo);
  const meta = VARIANT[variant];
  const size = avatarSize ?? meta.avatar;
  const dense = variant === "card" || variant === "split" || variant === "detail";
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
          <div className={extra.hitTitleCluster}>
            <strong className={`${extra.hitTitle} ${meta.titleClass}`} title={displayName}>{displayName}</strong>
            {isGgufHit(hit) ? <span aria-label="GGUF" className={extra.hitGguf} /> : null}
          </div>
          <CopyIconButton
            className={variant === "detail" ? extra.hitIdentityCopyVisible : extra.hitIdentityCopy}
            label={t("copyModelId")}
            size={variant === "split" ? 12 : 13}
            text={modelId}
          />
          {hubLink ? (
            <Tooltip content={hubLink.label}>
              <a
                aria-label={hubLink.label}
                className={extra.hubLink}
                href={hubLink.href}
                rel="noreferrer"
                target="_blank"
              >
                <ExternalLink aria-hidden="true" size={14} />
              </a>
            </Tooltip>
          ) : null}
        </div>
        {vendor ? (
          <div className={extra.hitIdRow}>
            <span className={extra.hitAuthor} title={vendor || undefined}>
              {vendor}
            </span>
          </div>
        ) : null}
      </div>
    </div>
  );
}
