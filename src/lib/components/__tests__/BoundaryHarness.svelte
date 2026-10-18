<script lang="ts">
  // Test-only: a ViewBoundary around a child that can throw while rendering.
  import ViewBoundary from "../ViewBoundary.svelte";
  import Thrower from "./Thrower.svelte";
</script>

<ViewBoundary name="Library"><Thrower /></ViewBoundary>
