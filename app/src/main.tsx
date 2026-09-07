import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { bootstrapHost } from "./host";
import { PANEL_IDS } from "./panels";

// The host session is bound to this native window, so it is created exactly once
// before React mounts rather than inside an effect that StrictMode would replay.
const { host, error } = await bootstrapHost(PANEL_IDS);

window.addEventListener("beforeunload", () => host?.dispose());

const container = document.getElementById("root");
if (!container) throw new Error("Missing #root container");

createRoot(container).render(
  <StrictMode>
    <App host={host} hostError={error} />
  </StrictMode>,
);
