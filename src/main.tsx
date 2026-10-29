import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "./index.css";
import { createQueryClient } from "@/app/query";
import { resolveIpc } from "@/lib/ipc-client";
import { AppProviders, createAppRouter } from "@/app/providers";

const queryClient = createQueryClient();
const ipc = resolveIpc();
const router = createAppRouter({ queryClient, ipc });

// The app ships dark-only; keep `dark` on <html> so shadcn variants match.
document.documentElement.classList.add("dark");
document.documentElement.style.colorScheme = "dark";

createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <AppProviders context={{ queryClient, ipc }}>
      <RouterProvider router={router} />
    </AppProviders>
  </StrictMode>,
);
