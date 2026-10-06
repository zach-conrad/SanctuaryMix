import { Check, Copy, Link2, Link2Off } from "lucide-react";
import { useEffect, useState } from "react";
import { canShare, isLive, shareUrl, type RecordingsCloud, type ShareLink } from "../cloud";

interface Props {
  cloud: RecordingsCloud;
  recordingId: string;
  /** False when nothing touched EQ in this service; the option is then disabled. */
  hasEq: boolean;
  root: string;
}

const EXPIRY: { label: string; days: number | null }[] = [
  { label: "7 days", days: 7 },
  { label: "30 days", days: 30 },
  { label: "No end date", days: null },
];

const dateFormat = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", year: "numeric" });

function linkStatus(l: ShareLink): string {
  if (l.revokedAt !== null) return `Turned off ${dateFormat.format(l.revokedAt)}`;
  if (l.expiresAt !== null && l.expiresAt <= Date.now()) return `Ended ${dateFormat.format(l.expiresAt)}`;
  const until = l.expiresAt === null ? "until you turn it off" : `until ${dateFormat.format(l.expiresAt)}`;
  const parts = [l.showMoves ? "moves" : null, l.showEq ? "EQ" : null].filter(Boolean);
  return `Works ${until}, ${parts.length ? `with ${parts.join(" and ")}` : "audio only"}`;
}

/** Make a link anyone can open to listen, and turn links off again. */
export function SharePanel({ cloud, recordingId, hasEq, root }: Props) {
  const allowed = canShare(cloud.session().role);
  const [links, setLinks] = useState<ShareLink[]>([]);
  const [days, setDays] = useState<number | null>(7);
  const [showMoves, setShowMoves] = useState(true);
  // Off by default: EQ notes and reasons are for the church unless someone chooses to share them.
  const [showEq, setShowEq] = useState(false);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    cloud.listShareLinks(recordingId).then(setLinks);
  }, [cloud, recordingId]);

  const create = async () => {
    setBusy(true);
    try {
      const link = await cloud.createShareLink(recordingId, { expiresInDays: days, showMoves, showEq: hasEq && showEq });
      setLinks(await cloud.listShareLinks(recordingId));
      copy(link);
    } finally {
      setBusy(false);
    }
  };

  const copy = (link: ShareLink) => {
    navigator.clipboard
      ?.writeText(shareUrl(root, link.token))
      .then(() => {
        setCopied(link.id);
        window.setTimeout(() => setCopied((c) => (c === link.id ? null : c)), 2000);
      })
      .catch(() => undefined);
  };

  const revoke = async (link: ShareLink) => {
    await cloud.revokeShareLink(link.id);
    setLinks(await cloud.listShareLinks(recordingId));
  };

  const live = links.filter((l) => isLive(l));
  const ended = links.filter((l) => !isLive(l)).slice(0, 3);

  return (
    <section className="share" aria-labelledby="share-heading">
      <div className="share__head">
        <h2 id="share-heading" className="text-heading">Share this mix</h2>
        <p className="text-caption mix-muted">Anyone with the link can listen in a browser. They can't change anything.</p>
      </div>

      {allowed ? (
        <>
          <div className="group">
            <div className="row">
              <span className="row-text" id="expiry-label">Link works for</span>
              <div className="sm-seg" role="group" aria-labelledby="expiry-label">
                {EXPIRY.map((o) => (
                  <button key={o.label} type="button" aria-pressed={days === o.days} onClick={() => setDays(o.days)}>
                    {o.label}
                  </button>
                ))}
              </div>
            </div>
          </div>
          <div className="grouped share__include">
            <div className="section-head">
              <h3>They can see</h3>
            </div>
            <div className="group">
              <div className="row">
                <span className="row-text">Audio</span>
                <span className="row-value">Always</span>
              </div>
              <label className="row">
                <span className="row-text">Fader and mute moves</span>
                <input
                  type="checkbox"
                  className="row-checkbox"
                  checked={showMoves}
                  onChange={(e) => setShowMoves(e.currentTarget.checked)}
                />
              </label>
              <label
                className="row"
                aria-disabled={!hasEq}
                title={hasEq ? undefined : "Nothing changed EQ in this service"}
              >
                <span className="row-text">EQ changes</span>
                <input
                  type="checkbox"
                  className="row-checkbox"
                  checked={hasEq && showEq}
                  disabled={!hasEq}
                  onChange={(e) => setShowEq(e.currentTarget.checked)}
                />
              </label>
              <div className="row row--actions">
                <button className="sm-btn sm-btn--primary" onClick={create} disabled={busy}>
                  <Link2 aria-hidden="true" />
                  Create link
                </button>
              </div>
            </div>
          </div>
        </>
      ) : (
        <p className="section-foot">Ask an Engineer or Admin at your church to make a share link.</p>
      )}

      {live.length + ended.length > 0 ? (
        <div className="grouped">
          <div className="section-head">
            <h3>Links</h3>
          </div>
          <ul className="group">
            {live.map((l) => (
              <li key={l.id} className="row share__link">
                <span className="row-text">
                  <input
                    className="share__url text-readout"
                    readOnly
                    value={shareUrl(root, l.token)}
                    aria-label="Share link"
                    onFocus={(e) => e.currentTarget.select()}
                  />
                  <span className="text-caption">{linkStatus(l)}</span>
                </span>
                <button className="sm-btn" onClick={() => copy(l)}>
                  {copied === l.id ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
                  {copied === l.id ? "Copied" : "Copy"}
                </button>
                {allowed ? (
                  <button
                    className="sm-btn sm-btn--danger share__off"
                    onClick={() => revoke(l)}
                    aria-label="Turn off link"
                    title="Turn off link"
                  >
                    <Link2Off aria-hidden="true" />
                  </button>
                ) : null}
              </li>
            ))}
            {ended.map((l) => (
              <li key={l.id} className="row share__link share__link--ended">
                <span className="row-text">
                  <span className="text-readout">…{l.token.slice(-6)}</span>
                  <span className="text-caption">{linkStatus(l)}</span>
                </span>
              </li>
            ))}
          </ul>
          <p className="section-foot">Preview: links are saved in this browser until accounts are connected.</p>
        </div>
      ) : (
        <p className="section-foot">Preview: links are saved in this browser until accounts are connected.</p>
      )}
    </section>
  );
}
