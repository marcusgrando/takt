import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "./lib/queryClient";
import Editor from "./Editor";
import "./index.css";

const applyTheme = (dark: boolean) =>
  document.documentElement.classList.toggle("dark", dark);

const mq = window.matchMedia("(prefers-color-scheme: dark)");
applyTheme(mq.matches);
mq.addEventListener("change", (e) => applyTheme(e.matches));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <Editor />
    </QueryClientProvider>
  </React.StrictMode>,
);
