import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { applyCachedScheme } from "./lib/schemes";

applyCachedScheme();

export default mount(App, { target: document.getElementById("app")! });
