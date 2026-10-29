/**
 * Tiny external store for cross-tree UI panels (command palette, editor sheet,
 * project dialogs) that are hosted in `__root` but opened from anywhere.
 * Kept outside React context so components in any subtree can open them.
 */
import { useSyncExternalStore } from "react";
import type { ItemKind } from "@/lib/ipc-types";

export interface EditorRequest {
  mode: "create" | "edit";
  itemId?: string;
}

export interface ProjectDialogRequest {
  mode: "create" | "rename" | "delete";
  projectId?: string;
}

interface UiState {
  paletteOpen: boolean;
  editor: EditorRequest | null;
  /** Default kind for a new item (from the list's kind filter). */
  defaultKind: ItemKind | null;
  projectDialog: ProjectDialogRequest | null;
  /** Editor draft is dirty; used by route blockers. */
  editorDirty: boolean;
}

const initial: UiState = {
  paletteOpen: false,
  editor: null,
  defaultKind: null,
  projectDialog: null,
  editorDirty: false,
};

let state: UiState = { ...initial };
const listeners = new Set<() => void>();

function set(partial: Partial<UiState>) {
  state = { ...state, ...partial };
  for (const fn of listeners) fn();
}

function subscribe(fn: () => void) {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

export const uiStore = {
  getState: () => state,
  subscribe,
  openPalette: () => set({ paletteOpen: true }),
  setPaletteOpen: (open: boolean) => set({ paletteOpen: open }),
  openCreate: (kind: ItemKind | null = null) => set({ editor: { mode: "create" }, defaultKind: kind }),
  openEdit: (itemId: string) => set({ editor: { mode: "edit", itemId } }),
  closeEditor: () => set({ editor: null, defaultKind: null, editorDirty: false }),
  setEditorDirty: (dirty: boolean) => set({ editorDirty: dirty }),
  openProjectDialog: (request: ProjectDialogRequest) => set({ projectDialog: request }),
  closeProjectDialog: () => set({ projectDialog: null }),
};

export function useUiStore<T>(selector: (s: UiState) => T): T {
  return useSyncExternalStore(
    subscribe,
    () => selector(state),
    () => selector(initial),
  );
}

/** Resets cross-tree panels (used between tests and on full lock). */
export function resetUiStore() {
  state = { ...initial };
  for (const fn of listeners) fn();
}

export function useUiNavigator() {
  return uiStore;
}

export type { UiState };
