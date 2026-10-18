import "./lib/styles/global.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { parseUiScale } from "./lib/uiScale";

// Apply the stored UI size before first paint so the app doesn't flash at 100%.
// App.svelte keeps it in sync afterwards.
try {
  document.documentElement.style.setProperty(
    "--ui-scale",
    String(parseUiScale(localStorage.getItem("silo.uiScale"))),
  );
} catch {
  // localStorage unavailable; App.svelte falls back to 100%.
}

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
