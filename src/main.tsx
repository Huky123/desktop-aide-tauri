import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import ScreenshotWindowApp from "./components/ui/ScreenshotWindowApp";
import { ErrorBoundary } from "./components/ui/ErrorBoundary";
import "./styles/globals.css";

const rootEl = document.getElementById("root");
if (!rootEl) throw new Error("Root element not found — check index.html");

// 独立截图窗口：URL 带 #screenshot，只渲染截图遮罩 UI
const isScreenshotWindow =
  window.location.hash === "#screenshot" ||
  new URLSearchParams(window.location.search).has("screenshot");

ReactDOM.createRoot(rootEl).render(
  <React.StrictMode>
    <ErrorBoundary>
      {isScreenshotWindow ? <ScreenshotWindowApp /> : <App />}
    </ErrorBoundary>
  </React.StrictMode>,
);
