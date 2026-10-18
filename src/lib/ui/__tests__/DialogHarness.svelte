<script lang="ts">
  // Test-only: an opener button that calls ask()/askWithOption()/askText(), plus the DialogHost.
  import DialogHost from "../DialogHost.svelte";
  import {
    ask,
    askText,
    askWithOption,
    type ConfirmOptions,
    type ConfirmWithOption,
    type PromptOptions,
  } from "../dialogs.svelte";

  type Props =
    | { mode: "ask"; opts: ConfirmOptions; onresult: (v: boolean) => void }
    | { mode: "askWithOption"; opts: ConfirmOptions; onresult: (v: ConfirmWithOption) => void }
    | { mode: "askText"; opts: PromptOptions; onresult: (v: string | null) => void };

  let props: Props = $props();

  async function open() {
    if (props.mode === "ask") props.onresult(await ask(props.opts));
    else if (props.mode === "askWithOption") props.onresult(await askWithOption(props.opts));
    else props.onresult(await askText(props.opts));
  }
</script>

<button type="button" onclick={open}>Open dialog</button>
<DialogHost />
