/* eslint-disable react-refresh/only-export-components */
import { createContext, useContext, type ReactNode } from "react";
import { defaultIpc } from "@/lib/ipc-client";
import type { IpcClient } from "@/lib/ipc";

interface IpcProviderProps {
  client: IpcClient;
  children: ReactNode;
}

const IpcContext = createContext<IpcClient | null>(null);

/**
 * Injects the IPC client. Production passes the auto-resolved client
 * (Tauri in the webview, mock in the browser); tests pass a deterministic
 * mock, which is how no test depends on a real Tauri runtime.
 */
export function IpcProvider({ client, children }: IpcProviderProps) {
  return <IpcContext.Provider value={client}>{children}</IpcContext.Provider>;
}

export function useIpc(): IpcClient {
  const ctx = useContext(IpcContext);
  if (ctx) return ctx;
  // Non-Tauri previews and stories: fall back to the process-wide client.
  return defaultIpc();
}

export type { IpcProviderProps };
