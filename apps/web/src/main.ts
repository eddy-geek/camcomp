import { mount } from "svelte";
import { registerSW } from "virtual:pwa-register";
import App from "./App.svelte";
import "./app.css";

registerSW({ immediate: true });

const target = document.getElementById("app");
if (!target) throw new Error("Application mount point is missing.");

mount(App, { target });
