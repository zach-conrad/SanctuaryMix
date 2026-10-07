import { eqTag } from "../lib/aieq";
import { useEqActive, useEqChannel } from "../store/aieq";

/** The small "EQ" label on a strip's name block: blue when AI EQ changed it live, muted when a person took it. */
export function EqTag({ channel }: { channel: number }) {
  const status = useEqChannel(channel);
  const active = useEqActive();
  const state = eqTag(status, active);
  if (!state) return null;
  return (
    <span
      className={`strip-eq${state === "manual" ? " is-manual" : ""}`}
      title={
        state === "manual"
          ? "EQ is yours. AI EQ won't touch it until you hand it back."
          : status?.notch
            ? `AI EQ cut feedback at this mic`
            : "AI EQ is keeping this voice's tone"
      }
    >
      EQ
    </span>
  );
}
