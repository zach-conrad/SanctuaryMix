import { useMixer } from "../store/mixer";

/** Shows the console's confirmed mute; a dashed outline while a change is in flight. */
export function MuteKey({ index, disabled, large }: { index: number; disabled: boolean; large?: boolean }) {
  const muted = useMixer((s) => s.strips[index]?.muted ?? false);
  const pending = useMixer((s) => s.pendingMutes[index] !== undefined);
  const toggleMute = useMixer((s) => s.toggleMute);
  return (
    <button
      className={`sm-key sm-key--mute ${large ? "key-large" : ""}`}
      aria-pressed={muted}
      data-pending={pending || undefined}
      disabled={disabled}
      onClick={() => toggleMute(index)}
    >
      Mute
    </button>
  );
}
