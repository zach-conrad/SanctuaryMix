import { memo } from "react";
import { formatDb } from "../lib/levels";
import { isAutoChanged, MODE_DETAIL, useChannelAuto } from "../store/automix";
import { useMixer, type Strip } from "../store/mixer";
import { EqTag } from "./EqTag";
import { Fader } from "./Fader";
import { Meter } from "./Meter";
import { MuteKey } from "./MuteKey";

export const ChannelStrip = memo(function ChannelStrip({ strip, disabled }: { strip: Strip; disabled: boolean }) {
  const setFader = useMixer((s) => s.setFader);
  const select = useMixer((s) => s.select);
  const selected = useMixer((s) => s.selected === strip.index);
  const auto = useChannelAuto(strip.index);
  const autoOn = auto !== undefined && auto.mode !== "off";
  return (
    <div
      className="sm-strip"
      role="group"
      aria-label={strip.name}
      aria-selected={selected}
      data-muted={strip.muted}
      onPointerDown={() => select(strip.index)}
    >
      {/* No tag stripe yet: channel colors aren't read from the dLive. */}
      <div className="sm-strip__name" title={strip.name}>
        <span className="sm-strip__num">{strip.index + 1}</span>
        <span className="sm-strip__label">{strip.name}</span>
        <EqTag channel={strip.index} />
      </div>
      <div className="sm-strip__bay">
        <Meter channel={strip.index} />
        <Fader
          db={strip.faderDb}
          disabled={disabled}
          label={strip.name}
          assist={isAutoChanged(auto)}
          onChange={(db) => setFader(strip.index, db)}
        />
      </div>
      {autoOn && (
        <span
          className={auto.mode === "heldByOperator" || auto.mode === "undone" ? "strip-auto is-manual" : "sm-assist-badge strip-auto"}
          title={MODE_DETAIL[auto.mode]}
        >
          {auto.mode === "heldByOperator" || auto.mode === "undone" ? "Manual" : "Auto"}
        </span>
      )}
      <div className="sm-strip__value">
        {formatDb(strip.faderDb)} <small>dB</small>
      </div>
      <div className="sm-keys">
        <MuteKey index={strip.index} disabled={disabled} />
      </div>
    </div>
  );
});
