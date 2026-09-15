import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { EngineProvider } from "./lib/engine";
import "./styles.css";

// Native context menu and text drag feel out of place in a desktop app.
document.addEventListener("contextmenu", (e) => e.preventDefault());

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <EngineProvider>
      <App />
    </EngineProvider>
  </React.StrictMode>,
);
