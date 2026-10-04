import { useCallback, useEffect, useRef, useState } from "react";

export interface Playback {
  ready: boolean;
  playing: boolean;
  positionMs: number;
  error: string | null;
  toggle(): void;
  seek(ms: number): void;
}

/**
 * Plays one audio file in the browser. The position follows the audio every
 * frame while playing so the timeline playhead and mixer keep up with it.
 * With no `src` (a moves-only service) a silent clock drives the same position.
 */
export function usePlayback(src: string | null, durationMs: number): Playback {
  const audio = useRef<HTMLAudioElement | null>(null);
  const [ready, setReady] = useState(src === null);
  const [playing, setPlaying] = useState(false);
  const [positionMs, setPositionMs] = useState(0);
  const [error, setError] = useState<string | null>(null);
  // Silent clock for moves-only playback: wall time when position 0 would have been.
  const clockStart = useRef(0);
  const pos = useRef(0);

  useEffect(() => {
    if (!src) return;
    const el = new Audio();
    el.preload = "metadata";
    el.src = src;
    audio.current = el;
    const onReady = () => setReady(true);
    const onPlay = () => {
      setError(null);
      setPlaying(true);
    };
    const onPause = () => setPlaying(false);
    const onTime = () => setPositionMs(el.currentTime * 1000);
    const onError = () => setError("The audio couldn't be loaded. Check your connection and try again.");
    el.addEventListener("loadedmetadata", onReady);
    el.addEventListener("play", onPlay);
    el.addEventListener("pause", onPause);
    el.addEventListener("ended", onPause);
    el.addEventListener("seeked", onTime);
    el.addEventListener("error", onError);
    return () => {
      el.pause();
      el.removeAttribute("src");
      el.load();
      audio.current = null;
    };
  }, [src]);

  useEffect(() => {
    if (!playing) return;
    let frame = 0;
    const tick = () => {
      if (audio.current) {
        pos.current = audio.current.currentTime * 1000;
      } else {
        pos.current = Math.min(durationMs, performance.now() - clockStart.current);
        if (pos.current >= durationMs) setPlaying(false);
      }
      setPositionMs(pos.current);
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [playing, durationMs]);

  useEffect(() => {
    pos.current = positionMs;
  }, [positionMs]);

  const toggle = useCallback(() => {
    const el = audio.current;
    if (el) {
      if (el.paused)
        el.play().catch((e: unknown) =>
          setError(
            e instanceof DOMException && e.name === "NotAllowedError"
              ? "The browser blocked playback. Press Play again."
              : "This browser can't play the audio. Try another browser.",
          ),
        );
      else el.pause();
      return;
    }
    setPlaying((p) => {
      if (!p) {
        const from = pos.current >= durationMs ? 0 : pos.current;
        clockStart.current = performance.now() - from;
      }
      return !p;
    });
  }, [durationMs]);

  const seek = useCallback(
    (ms: number) => {
      const t = Math.max(0, Math.min(durationMs, ms));
      pos.current = t;
      clockStart.current = performance.now() - t;
      if (audio.current) audio.current.currentTime = t / 1000;
      setPositionMs(t);
    },
    [durationMs],
  );

  return { ready, playing, positionMs, error, toggle, seek };
}
