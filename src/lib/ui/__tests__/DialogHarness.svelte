<script lang="ts">
  // Test-only: an opener button that calls ask()/askText(), plus the DialogHost that renders it.
  import DialogHost from "../DialogHost.svelte";
  import { ask, askText, type ConfirmOptions, type PromptOptions } from "../dialogs.svelte";

  type Props =
    | { mode: "ask"; opts: ConfirmOptions; onresult: (v: boolean) => void }
    | { mode: "askText"; opts: PromptOptions; onresult: (v: string | null) => void };

  let props: Props = $props();

  async function open() {
    if (props.mode === "ask") props.onresult(await ask(props.opts));
    else props.onresult(await askText(props.opts));
  }
</script>

<button type="button" onclick={open}>Open dialog</button>
<DialogHost />
