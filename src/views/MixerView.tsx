import { ChannelStrip } from "../components/ChannelStrip";
import { Inspector } from "../components/Inspector";
import { useMixer } from "../store/mixer";

export function MixerView() {
  const strips = useMixer((s) => s.strips);
  const consoleOn = useMixer((s) => s.consoleStatus === "on");
  const setView = useMixer((s) => s.setView);

  return (
    <div className="mixer">
      <div className="mixer-main">
        {!consoleOn && (
          <div className="banner">
            <span>Connect to your console to move faders and mutes. Meters work as soon as Dante audio is running.</span>
            <button className="sm-btn sm-btn--sm" onClick={() => setView("setup")}>
              Open Setup
            </button>
          </div>
        )}
        <div className="bay" role="group" aria-label="Inputs">
          {strips.map((s) => (
            <ChannelStrip key={s.index} strip={s} disabled={!consoleOn} />
          ))}
        </div>
      </div>
      <Inspector />
    </div>
  );
}
