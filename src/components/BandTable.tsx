import { useState } from "react";
import {
  bandCaption,
  bandName,
  cloneEq,
  gainChange,
  hzChange,
  isNotchUsed,
  parseDb,
  parseHz,
  qToWidth,
  RESERVED_BAND,
} from "../lib/aieq";
import { formatGain, formatHz, formatQ } from "../lib/eqCurve";
import type { ChannelEq } from "../lib/types";
import type { HandleId } from "./EqGraph";

type Field = "freq" | "gain" | "q";

interface Props {
  eq: ChannelEq;
  /** The suggestion: changed values read "now → new" in blue. */
  proposed?: ChannelEq | null;
  /** Values can be tapped and typed. */
  editable?: boolean;
  selected?: HandleId | null;
  onSelect?(id: HandleId): void;
  onChange?(eq: ChannelEq): void;
}

const DASH = <span className="val muted">—</span>;

/** H, bands 1 to 3, and band 4 kept free for feedback: one grouped row each. */
export function BandTable({ eq, proposed = null, editable = false, selected = null, onSelect, onChange }: Props) {
  const [editing, setEditing] = useState<{ id: HandleId; field: Field } | null>(null);
  const canEdit = editable && !proposed && !!onChange;

  function commit(id: HandleId, field: Field, text: string) {
    setEditing(null);
    if (!onChange) return;
    const next = cloneEq(eq);
    if (id === "hpf") {
      if (field !== "freq") return;
      const off = /^off$/i.test(text.trim());
      const hz = parseHz(text);
      if (off) next.hpf = { ...next.hpf, on: false };
      else if (hz !== null) next.hpf = { on: true, freqHz: Math.min(hz, 2000) };
      else return;
    } else {
      const band = next.bands[id];
      if (field === "freq") {
        const hz = parseHz(text);
        if (hz === null) return;
        band.freqHz = hz;
      } else if (field === "gain") {
        const db = parseDb(text);
        if (db === null) return;
        band.gainDb = db;
      } else {
        const q = Number(text.trim());
        if (!Number.isFinite(q) || q < 0.4 || q > 16) return;
        band.width = qToWidth(q);
      }
    }
    onChange(next);
  }

  /** A value cell: plain, "now → new" in blue, or a tap-to-type button. */
  function cell(id: HandleId, field: Field, now: string, changed: string | null, raw: string, label: string) {
    if (changed) return <span className="val is-ai">{changed}</span>;
    if (editing && editing.id === id && editing.field === field) {
      return (
        <span className="val">
          <input
            className="val-input"
            defaultValue={raw}
            autoFocus
            aria-label={label}
            onBlur={(e) => commit(id, field, e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
              // Esc drops the typing and still reaches the app-wide Freeze.
              if (e.key === "Escape") setEditing(null);
            }}
          />
        </span>
      );
    }
    if (!canEdit) return <span className="val">{now}</span>;
    return (
      <span className="val">
        <button className="val-btn" title={`Type a new ${field === "q" ? "Q" : field === "freq" ? "frequency" : "gain"}`} aria-label={`${label}: ${now}`} onClick={() => setEditing({ id, field })}>
          {now}
        </button>
      </span>
    );
  }

  const hpf = eq.hpf;
  const pHpf = proposed?.hpf;
  const hpfChanged = pHpf && (pHpf.on !== hpf.on || pHpf.freqHz !== hpf.freqHz);
  const hpfNow = hpf.on ? formatHz(hpf.freqHz) : "Off";

  return (
    <div className="group" role="table" aria-label="Bands">
      <div className="row band-row band-head" role="row">
        <span role="columnheader" />
        <span className="text-label" role="columnheader">
          Band
        </span>
        <span className="val" role="columnheader">
          Freq
        </span>
        <span className="val" role="columnheader">
          Gain
        </span>
        <span className="val" role="columnheader">
          Q
        </span>
      </div>
      <div
        className={`row band-row${hpfChanged ? " is-ai" : ""}${selected === "hpf" ? " is-selected" : ""}`}
        role="row"
        onPointerDown={() => onSelect?.("hpf")}
      >
        <span className="num">H</span>
        <span className="kind">
          <span>Low cut</span>
          <span className="text-caption">12 dB/oct</span>
        </span>
        {cell(
          "hpf",
          "freq",
          hpfNow,
          hpfChanged && pHpf
            ? hpf.on && pHpf.on
              ? hzChange(hpf.freqHz, pHpf.freqHz)
              : `${hpfNow} → ${pHpf.on ? formatHz(pHpf.freqHz) : "Off"}`
            : null,
          hpf.on ? String(hpf.freqHz) : "Off",
          "Low cut frequency",
        )}
        {DASH}
        {DASH}
      </div>
      {eq.bands.map((b, i) => {
        if (i === RESERVED_BAND) return null;
        const p = proposed?.bands[i];
        const freqChanged = p && Math.abs(p.freqHz - b.freqHz) >= 1;
        const gainChanged = p && Math.abs(p.gainDb - b.gainDb) >= 0.05;
        const qChanged = p && Math.abs(p.width - b.width) >= 0.01;
        const shelf = b.kind !== "bell";
        return (
          <div
            key={i}
            className={`row band-row${freqChanged || gainChanged || qChanged ? " is-ai" : ""}${selected === i ? " is-selected" : ""}`}
            role="row"
            onPointerDown={() => onSelect?.(i)}
          >
            <span className="num">{i + 1}</span>
            <span className="kind">
              <span>{bandName(i, b)}</span>
              <span className="text-caption">{bandCaption(b)}</span>
            </span>
            {cell(i, "freq", formatHz(b.freqHz), freqChanged && p ? hzChange(b.freqHz, p.freqHz) : null, String(b.freqHz), `Band ${i + 1} frequency`)}
            {b.kind === "lowPass" || b.kind === "highPass"
              ? DASH
              : cell(i, "gain", formatGain(b.gainDb), gainChanged && p ? gainChange(b.gainDb, p.gainDb) : null, b.gainDb.toFixed(1), `Band ${i + 1} gain`)}
            {shelf
              ? DASH
              : cell(i, "q", formatQ(b.width), qChanged && p ? `${formatQ(b.width)} → ${formatQ(p.width)}` : null, formatQ(b.width), `Band ${i + 1} Q`)}
          </div>
        );
      })}
      <ReservedRow eq={eq} selected={selected === RESERVED_BAND} onSelect={() => onSelect?.(RESERVED_BAND)} />
    </div>
  );
}

/** Band 4: SanctuaryMix keeps it free for feedback notches. */
function ReservedRow({ eq, selected, onSelect }: { eq: ChannelEq; selected: boolean; onSelect(): void }) {
  const b = eq.bands[RESERVED_BAND];
  const used = isNotchUsed(eq);
  return (
    <div
      className={`row band-row is-reserved${selected ? " is-selected" : ""}`}
      role="row"
      title="SanctuaryMix keeps band 4 free so it can cut feedback without touching your EQ."
      onPointerDown={onSelect}
    >
      <span className="num">4</span>
      <span className="kind">
        <span>SanctuaryMix</span>
        <span className="text-caption">{used ? "Feedback notch" : "Kept free for feedback"}</span>
      </span>
      {used ? (
        <>
          <span className="val state-ai">{formatHz(b.freqHz)}</span>
          <span className="val state-ai">{formatGain(b.gainDb)}</span>
          <span className="val">{formatQ(b.width)}</span>
        </>
      ) : (
        <>
          <span className="val muted">Free</span>
          {DASH}
          {DASH}
        </>
      )}
    </div>
  );
}
