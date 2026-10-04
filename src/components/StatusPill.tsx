export type PillTone = "ok" | "warn" | "error" | "neutral";

interface Props {
  tone: PillTone;
  subject: string;
  meta?: string;
  onClick?: () => void;
}

/** Top-bar status: always a word plus a dot. Clicking opens the matching setup. */
export function StatusPill({ tone, subject, meta, onClick }: Props) {
  const cls = `sm-pill ${tone === "neutral" ? "" : `sm-pill--${tone}`}`;
  const body = (
    <>
      <span className="sm-pill__dot" aria-hidden />
      <span>{subject}</span>
      {meta && <span className="sm-pill__meta">{meta}</span>}
    </>
  );
  return onClick ? (
    <button className={`${cls} pill-button`} onClick={onClick}>
      {body}
    </button>
  ) : (
    <span className={cls}>{body}</span>
  );
}
