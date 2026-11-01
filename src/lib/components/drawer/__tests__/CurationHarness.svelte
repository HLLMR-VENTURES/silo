<script lang="ts">
  // Test-only: ModCuration wired as App wires it (#358): the row map, a SaveLedger of what the
  // database holds, and saveCuration putting a row back when its save fails.
  import ModCuration from "../../ModCuration.svelte";
  import { setCuration } from "../../../api";
  import { SaveLedger, withEntry } from "../../../state/optimistic";
  import type { CurationRow } from "../../../types";

  let { techName, stored }: { techName: string; stored: CurationRow } = $props();

  let map = $state<Record<string, CurationRow>>({});
  const ledger = new SaveLedger<CurationRow>();
  // Seeded once, as App's startup load does.
  // svelte-ignore state_referenced_locally
  map = { [stored.techName]: stored };
  // svelte-ignore state_referenced_locally
  ledger.load([[stored.techName, stored]]);

  async function saveCuration(next: CurationRow): Promise<void> {
    const token = ledger.begin(next.techName);
    map = withEntry(map, next.techName, next);
    try {
      await setCuration(next);
      ledger.saved(next.techName, next);
    } catch (e) {
      const back = ledger.failed(next.techName, token);
      if (back) map = withEntry(map, next.techName, back.value);
      throw e;
    }
  }

  /** For the test: the row App would show now. */
  export function shown(): CurationRow | undefined {
    return map[techName];
  }
  /** For the test: another part of the app replaced the row (a flag set from the list). */
  export function replace(row: CurationRow): void {
    map = withEntry(map, row.techName, row);
  }
</script>

<ModCuration
  {techName}
  curation={map[techName]}
  tags={[]}
  onCurationChange={() => {}}
  {saveCuration}
  onTagsChange={() => {}}
  onFilterTag={() => {}}
/>
