import { Eye, Image, Volume2 } from "lucide-react";
import { memo } from "react";
import { useTranslation } from "react-i18next";
import { Tooltip } from "../../components/Tooltip";
import { hitCapabilities, type HitCapability } from "../../lib/modelHits";
import type { RemoteModelHit } from "../../lib/types";
import extra from "./discover.module.css";

const CAP_ICON = {
  vision: Eye,
  image: Image,
  audio: Volume2,
} as const;

const CAP_CLASS = {
  vision: extra.hitCapVision,
  image: extra.hitCapImage,
  audio: extra.hitCapAudio,
} as const;

const CAP_LABEL = {
  vision: "modelCapVision",
  image: "modelCapImage",
  audio: "modelCapAudio",
} as const;

function CapabilityIcon({ kind }: { kind: HitCapability }) {
  const { t } = useTranslation();
  const Icon = CAP_ICON[kind];
  return (
    <Tooltip content={t(CAP_LABEL[kind])}>
      <span className={`${extra.hitCap} ${CAP_CLASS[kind]}`}>
        <Icon aria-hidden="true" size={12} />
      </span>
    </Tooltip>
  );
}

export const CapabilityIcons = memo(function CapabilityIcons({ hit }: { hit: RemoteModelHit }) {
  const caps = hitCapabilities(hit);
  if (caps.length === 0) return <div className={extra.hitCaps} />;
  return (
    <div className={extra.hitCaps}>
      {caps.map((kind) => <CapabilityIcon key={kind} kind={kind} />)}
    </div>
  );
});
