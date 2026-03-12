import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { installDebugSnapshot } from "./lib/debug";

// Installs window.__spatia_debug_snapshot() in DEV builds only.
installDebugSnapshot();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
