// "Open ↗" for one of Silo's folders, with the error shown under that folder's row:
//   const folders = folderOpener();
//   <Button onclick={() => folders.open("archive")}>Open</Button>
//   {#if folders.error?.which === "archive"}<p class="hint">{folders.error.msg}</p>{/if}
import { openKnownFolder } from "../../api";
import { inlineError } from "../../state/notify.svelte";

export type OpenTarget = "modsFolder" | "archive" | "gameInstall";

/** A per-component opener; `error` is the last failure (cleared by the next open). */
export function folderOpener() {
  let error = $state<{ which: OpenTarget; msg: string } | null>(null);
  return {
    get error() {
      return error;
    },
    open(which: OpenTarget): void {
      error = null;
      openKnownFolder(which).catch((e) => (error = { which, msg: inlineError(e, "Opening the folder") }));
    },
  };
}
