import { Check, Copy } from "lucide-react";
import { useEffect, useRef, useState, type MouseEvent } from "react";
import { useTranslation } from "react-i18next";
import { Tooltip } from "./Tooltip";
import styles from "./CopyIconButton.module.css";

type CopyIconButtonProps = {
  text: string;
  /** Idle tooltip / aria-label. Defaults to t("copy"). */
  label?: string;
  /** Success tooltip / aria-label. Defaults to t("copied"). */
  copiedLabel?: string;
  /** Lucide icon size in px. */
  size?: number;
  className?: string;
  /** Called after a successful clipboard write. */
  onCopied?: () => void;
  /** Called when clipboard write fails. */
  onError?: (error: unknown) => void;
};

export function CopyIconButton({
  text,
  label,
  copiedLabel,
  size = 14,
  className,
  onCopied,
  onError,
}: CopyIconButtonProps) {
  const { t } = useTranslation();
  const idle = label ?? t("copy");
  const done = copiedLabel ?? t("copied");
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => () => {
    if (timer.current) clearTimeout(timer.current);
  }, []);

  const copy = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      onCopied?.();
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1500);
    } catch (error) {
      setCopied(false);
      onError?.(error);
    }
  };

  return (
    <Tooltip content={copied ? done : idle}>
      <button
        aria-label={copied ? done : idle}
        className={`${styles.button}${className ? ` ${className}` : ""}`}
        data-copied={copied || undefined}
        onClick={(event) => void copy(event)}
        onPointerDown={(event) => event.stopPropagation()}
        type="button"
      >
        {copied ? <Check aria-hidden="true" size={size} /> : <Copy aria-hidden="true" size={size} />}
      </button>
    </Tooltip>
  );
}
