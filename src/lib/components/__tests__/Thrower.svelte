<script lang="ts">
  // Test-only: throws while rendering when `failing.now` is true.
  import { failing } from "./thrower";

  if (failing.now) throw new Error("render blew up");
</script>

<p>Library content</p>
