import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { applyCachedScheme } from "./lib/schemes";

applyCachedScheme();

// Focus rings only while the keyboard moves focus (app.css): a click turns
// them off, Tab, arrows or F6 back on.
const root = document.documentElement;
addEventListener("pointerdown", () => (root.dataset.input = "pointer"), true);
addEventListener(
  "keydown",
  (e) => {
    if (e.key === "Tab" || e.key === "F6" || e.key.startsWith("Arrow")) root.dataset.input = "keys";
  },
  true,
);

export default mount(App, { target: document.getElementById("app")! });
