import { useEffect, useState } from "react";
import { call } from "./api";

const KEY = "kae.presence";
const EVT = "kae-presence";

export function presenceOn(): boolean {
  try {
    return localStorage.getItem(KEY) !== "0";
  } catch {
    return true;
  }
}

export function setPresenceOn(on: boolean) {
  try {
    localStorage.setItem(KEY, on ? "1" : "0");
  } catch {
    /* storage unavailable: the choice lasts until restart */
  }
  window.dispatchEvent(new Event(EVT));
}

export function usePresenceOn(): boolean {
  const [on, setOn] = useState(presenceOn);
  useEffect(() => {
    const h = () => setOn(presenceOn());
    window.addEventListener(EVT, h);
    return () => window.removeEventListener(EVT, h);
  }, []);
  return on;
}

/** Whether a Discord application id is configured; the switch in settings is hidden otherwise. */
export function usePresenceAvailable(): boolean {
  const [ok, setOk] = useState(false);
  useEffect(() => {
    void call<boolean>("presence_available").then(setOk, () => setOk(false));
  }, []);
  return ok;
}

let playing: string | null = null;
const PLAY_EVT = "kae-playing";

/** The pack that is being played now, or null. While it is set the status says so, with a timer. */
export function setPlaying(name: string | null) {
  if (playing === name) return;
  playing = name;
  window.dispatchEvent(new Event(PLAY_EVT));
}

function usePlaying(): string | null {
  const [v, setV] = useState(playing);
  useEffect(() => {
    const h = () => setV(playing);
    window.addEventListener(PLAY_EVT, h);
    return () => window.removeEventListener(PLAY_EVT, h);
  }, []);
  return v;
}

/** Keeps the Discord status in step with `details`. Errors are ignored: presence is optional. */
export function usePresence(details: string, state: string | undefined, playingText: (name: string) => string) {
  const on = usePresenceOn();
  const name = usePlaying();
  useEffect(() => {
    const body = name ? { details: "Minecraft", detailState: playingText(name), timer: true } : { details, detailState: state ?? null, timer: false };
    void call("presence_set", { ...body, details: on ? body.details : null }).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [on, details, state, name]);
}
