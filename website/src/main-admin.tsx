import "@fontsource/figtree/400.css";
import "@fontsource/figtree/600.css";
import "@fontsource/figtree/700.css";
import "@fontsource/ibm-plex-mono/500.css";
import "./styles/tokens.css";
import "./styles/components.css";
import "./styles/site.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Admin } from "./Admin";


createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Admin />
  </StrictMode>,
);
