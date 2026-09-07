import { avatarTone } from "../lib/modelHits";
import { resolveProviderLogo, type ProviderLogo } from "../lib/providerLogos";
import styles from "./ModelVendorIcon.module.css";

const TONE_CLASS = [styles.tone0, styles.tone1, styles.tone2] as const;

type ModelVendorIconProps = {
  owner: string;
  /** Repo name after `owner/` (not full `owner/name`). */
  repoName?: string;
  /** Accessible name; defaults to provider name or owner. */
  name?: string;
  size?: number;
  className?: string;
};

function radiusFor(size: number) {
  return Math.max(6, Math.round(size * 0.22));
}

export function ModelVendorIcon({
  owner,
  repoName,
  name,
  size = 36,
  className,
}: ModelVendorIconProps) {
  const logo = resolveProviderLogo(owner, repoName);
  if (logo) {
    return <ProviderTile logo={logo} size={size} className={className} label={name} />;
  }
  const initial = (owner.trim().slice(0, 1) || "?").toUpperCase();
  const label = name?.trim() || owner.trim() || "model";
  return (
    <span
      aria-label={label}
      className={`${styles.tile} ${TONE_CLASS[avatarTone(owner || "?")]} ${className ?? ""}`.trim()}
      role="img"
      style={{
        borderRadius: radiusFor(size),
        fontSize: Math.max(11, Math.round(size * 0.36)),
        height: size,
        width: size,
      }}
    >
      {initial}
    </span>
  );
}

function ProviderTile({
  logo,
  size,
  className,
  label,
}: {
  logo: ProviderLogo;
  size: number;
  className?: string;
  label?: string;
}) {
  const treatment = logo.treatment ?? "original";
  const background = logo.background ?? "transparent";
  const bgClass = background === "white" ? styles.logoBgWhite : styles.logoBgTransparent;
  const alt = `${label?.trim() || logo.name} logo`;

  return (
    <span
      aria-label={alt}
      className={`${styles.tile} ${bgClass} ${className ?? ""}`.trim()}
      role="img"
      style={{ borderRadius: radiusFor(size), height: size, width: size }}
    >
      {treatment === "mono-theme" ? (
        <span
          aria-hidden="true"
          className={styles.mono}
          style={{
            WebkitMaskImage: `url(${logo.src})`,
            maskImage: `url(${logo.src})`,
          }}
        />
      ) : (
        <img
          alt=""
          className={logo.fit === "cover" ? styles.imgCover : styles.img}
          decoding="async"
          draggable={false}
          src={logo.src}
        />
      )}
    </span>
  );
}
