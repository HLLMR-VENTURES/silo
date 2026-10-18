// Last-resort error capture: anything thrown outside a try/catch or a view boundary ends up in
// Silo's log file and the activity panel instead of only the devtools console.
// main.ts calls installGlobalErrorHandlers() once, before mounting the app.

import { appVersion } from "../api";
import { rememberAppVersion } from "../errorCopy";
import { reportError } from "./notify.svelte";

/** Browser noise that isn't a Silo problem. */
const NOISE = /ResizeObserver loop/i;

function messageOf(v: unknown): string {
  if (v && typeof v === "object" && typeof (v as { message?: unknown }).message === "string") {
    return (v as { message: string }).message;
  }
  return String(v ?? "");
}

/**
 * Report uncaught errors and unhandled promise rejections (quietly: log + activity, no toast).
 * Also fetches the app version once so "Copy details" can include it.
 * @param target where to listen (default `window`)
 * @returns a function that removes the handlers
 */
export function installGlobalErrorHandlers(target: Window = window): () => void {
  const onError = (ev: ErrorEvent) => {
    const msg = ev.message || messageOf(ev.error);
    if (NOISE.test(msg)) return;
    reportError(ev.error ?? msg, "running Silo", { quiet: true });
  };
  const onRejection = (ev: PromiseRejectionEvent) => {
    if (NOISE.test(messageOf(ev.reason))) return;
    reportError(ev.reason, "running Silo", { quiet: true });
  };
  target.addEventListener("error", onError);
  target.addEventListener("unhandledrejection", onRejection);
  appVersion()
    .then(rememberAppVersion)
    .catch(() => {});
  return () => {
    target.removeEventListener("error", onError);
    target.removeEventListener("unhandledrejection", onRejection);
  };
}
