import { useEffect } from "react";
import { describeEntry, voiceDrift } from "../lib/aieq";
import { formatGain } from "../lib/eqCurve";
import { allows, can } from "../lib/plans";
import { timeOfDay } from "../lib/recordings";
import type { EqIdea } from "../lib/types";
import { useAiEq } from "../store/aieq";
import { useMixer } from "../store/mixer";

/** How far a voice may move and still count as close to its profile. */
const CLOSE_DB = 1.5;

/** The EQ part of a service report: ideas for next week, what changed live, and how close voices stayed. */
export function ReportEq({ recordingId }: { recordingId: string }) {
  const allowed = useMixer((s) => allows(s.session, "aiEq"));
  const report = useAiEq((s) => (s.report?.recordingId === recordingId ? s.report : null));
  const loadReport = useAiEq((s) => s.loadReport);

  useEffect(() => {
    if (allowed) void loadReport(recordingId);
  }, [allowed, recordingId, loadReport]);

  if (!allowed || !report) return null;
  const entries = report.audit.entries;
  const voices = voiceDrift(entries);
  return (
    <div className="page-split report-eq">
      <div className="page-main">
        <Ideas ideas={report.ideas} />
        <section className="section" aria-labelledby="eq-live-title">
          <div className="section-head">
            <h2 id="eq-live-title">What changed live</h2>
          </div>
          <div className="group">
            {entries.length === 0 ? (
              <div className="row empty">AI EQ didn't change anything during this service.</div>
            ) : (
              entries.map((e, i) => (
                <div key={`${e.atMs}-${i}`} className="row" title={e.reason ?? undefined}>
                  <span className="text-readout muted report-time">{timeOfDay(e.atMs).toLowerCase()}</span>
                  <span className="row-text">
                    <span>{e.channelName}</span>
                    <span className="text-caption">{describeEntry(e)}</span>
                  </span>
                  <span className="state-word">
                    {e.by.kind === "person" ? (e.by.where === "desk" ? "At the desk" : "In the app") : "AI EQ"}
                  </span>
                </div>
              ))
            )}
          </div>
        </section>
      </div>
      <aside className="page-side">
        <section className="section" aria-labelledby="eq-voices-title">
          <div className="section-head">
            <h2 id="eq-voices-title">How close voices stayed</h2>
          </div>
          <div className="group">
            {voices.length === 0 ? (
              <div className="row empty">No speech tone keeping in this service.</div>
            ) : (
              voices.map((v) => (
                <div key={v.channel} className="row" title="The furthest tone keeping moved this voice from its profile.">
                  <span className="row-text">{v.name}</span>
                  <span className={`state-word${v.rangeDb > CLOSE_DB ? " is-warn" : ""}`}>
                    {v.rangeDb > CLOSE_DB ? "Drifted" : "Close to profile"}
                  </span>
                  <span className="limit-value">±{formatGain(v.rangeDb).replace("+", "")}</span>
                </div>
              ))
            )}
          </div>
        </section>
      </aside>
    </div>
  );
}

function Ideas({ ideas }: { ideas: EqIdea[] }) {
  const engineer = useMixer((s) => can(s.session, "changeAutoMixSetup"));
  const setIdea = useAiEq((s) => s.setIdea);
  const waiting = ideas.filter((i) => i.state === "waiting").length;
  return (
    <section className="section" aria-labelledby="eq-ideas-title">
      <div className="section-head">
        <h2 id="eq-ideas-title">EQ for next week</h2>
        <span className="text-caption">{waiting > 0 ? waiting : ""}</span>
      </div>
      <div className="group">
        {ideas.length === 0 ? (
          <div className="row empty">No EQ ideas from this service.</div>
        ) : (
          ideas.map((idea) => (
            <div key={idea.id} className="row eq-ch-row">
              <span className="row-text">
                <span>{idea.title}</span>
                <span className="text-caption">
                  <span className="text-readout state-ai">{idea.change}</span> · {idea.reason}
                </span>
              </span>
              {idea.state === "waiting" ? (
                <>
                  <button
                    className="sm-btn"
                    disabled={!engineer}
                    title={engineer ? "Adds it to next Sunday's soundcheck" : "Engineers and admins can keep ideas."}
                    onClick={() => void setIdea(idea.id, "kept")}
                  >
                    Keep
                  </button>
                  <button
                    className="sm-btn sm-btn--ghost"
                    disabled={!engineer}
                    title={engineer ? undefined : "Engineers and admins can dismiss ideas."}
                    onClick={() => void setIdea(idea.id, "dismissed")}
                  >
                    Dismiss
                  </button>
                </>
              ) : (
                <span className="state-word">{idea.state === "kept" ? "Kept" : "Dismissed"}</span>
              )}
            </div>
          ))
        )}
      </div>
      <p className="section-foot">Kept changes go into next Sunday's soundcheck.</p>
    </section>
  );
}
