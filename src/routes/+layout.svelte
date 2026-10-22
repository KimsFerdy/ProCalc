<script lang="ts">
  import "../app.css";
  import "mathlive/fonts.css";
  import "mathlive/static.css";

  let { children } = $props();
</script>

<svelte:head>
  <title>ProCalc</title>
</svelte:head>

{@render children()}
