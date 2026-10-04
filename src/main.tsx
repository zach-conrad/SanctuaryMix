import React from "react";
import ReactDOM from "react-dom/client";
// Fonts are bundled so the app works with no internet in the booth.
import "@fontsource/figtree/400.css";
import "@fontsource/figtree/600.css";
import "@fontsource/figtree/700.css";
import "@fontsource/ibm-plex-mono/500.css";
import "./styles/tokens.css"; // copy of design/tokens.css
import "./styles/components.css"; // copy of design/components.css (sm- classes)
import "./styles/global.css"; // app layout
import App from "./App";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
