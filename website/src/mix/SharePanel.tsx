import { Check, Copy, Link2, Link2Off } from "lucide-react";
import { useEffect, useState } from "react";
import { canShare, isLive, shareUrl, type RecordingsCloud, type ShareLink } from "../cloud";

interface Props {
  cloud: RecordingsCloud;
  recordingId: string;
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
  return `Works ${until}${l.showMoves ? ", with moves" : ", audio only"}`;
}

/** Make a link anyone can open to listen, and turn links off again. */
export function SharePanel({ cloud, recordingId, root }: Props) {
  const allowed = canShare(cloud.session().role);
  const [links, setLinks] = useState<ShareLink[]>([]);
  const [days, setDays] = useState<number | null>(7);
  const [showMoves, setShowMoves] = useState(true);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    cloud.listShareLinks(recordingId).then(setLinks);
  }, [cloud, recordingId]);

  const create = async () => {
    setBusy(true);
    try {
      const link = await cloud.createShareLink(recordingId, { expiresInDays: days, showMoves });
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
    <section className="panel share" aria-labelledby="share-heading">
      <div className="panel__head">
        <Link2 aria-hidden="true" />
        <h2 id="share-heading" className="text-heading">Share this mix</h2>
      </div>
      <p className="feature__body">
        Anyone with the link can listen in a browser without an account. They can't change anything or send it to a
        console.
      </p>

      {allowed ? (
        <div className="share__form">
          <div className="share__field">
            <span className="text-label share__label" id="expiry-label">Link works for</span>
            <div className="sm-seg" role="group" aria-labelledby="expiry-label">
              {EXPIRY.map((o) => (
                <button key={o.label} type="button" aria-pressed={days === o.days} onClick={() => setDays(o.days)}>
                  {o.label}
                </button>
              ))}
            </div>
          </div>
          <label className="share__check">
            <input type="checkbox" checked={showMoves} onChange={(e) => setShowMoves(e.currentTarget.checked)} />
            Show fader and mute moves
          </label>
          <button className="sm-btn sm-btn--primary" onClick={create} disabled={busy}>
            <Link2 aria-hidden="true" />
            Create link
          </button>
        </div>
      ) : (
        <p className="text-caption mix-muted">Ask an Engineer or Admin at your church to make a share link.</p>
      )}

      {live.length > 0 ? (
        <ul className="share__links" aria-label="Active links">
          {live.map((l) => (
            <li key={l.id} className="share__link">
              <input
                className="sm-input share__url text-readout"
                readOnly
                value={shareUrl(root, l.token)}
                aria-label="Share link"
                onFocus={(e) => e.currentTarget.select()}
              />
              <div className="share__actions">
                <button className="sm-btn" onClick={() => copy(l)}>
                  {copied === l.id ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
                  {copied === l.id ? "Copied" : "Copy"}
                </button>
                {allowed ? (
                  <button className="sm-btn sm-btn--danger" onClick={() => revoke(l)}>
                    <Link2Off aria-hidden="true" />
                    Turn off
                  </button>
                ) : null}
              </div>
              <span className="text-caption mix-muted">{linkStatus(l)}</span>
            </li>
          ))}
        </ul>
      ) : null}

      {ended.length > 0 ? (
        <ul className="share__ended text-caption" aria-label="Links that no longer work">
          {ended.map((l) => (
            <li key={l.id}>
              <span className="text-readout">…{l.token.slice(-6)}</span> {linkStatus(l)}
            </li>
          ))}
        </ul>
      ) : null}

      <p className="text-caption mix-subtle" role="note">
        Preview: links are saved in this browser until accounts are connected.
      </p>
    </section>
  );
}
