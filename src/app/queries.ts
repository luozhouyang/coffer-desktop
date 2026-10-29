import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useIpc } from "@/app/ipc-context";
import {
  itemPrefix,
  itemsPrefix,
  projectsPrefix,
  queryKeys,
} from "@/lib/query-keys";
import type {
  ItemFilter,
  ItemInput,
  ItemUpdate,
  ProjectInput,
  ProjectUpdate,
  Settings,
} from "@/lib/ipc-types";

/* ------------------------------------------------------------- queries */

export function useVaultStatus() {
  const ipc = useIpc();
  return useQuery({ queryKey: queryKeys.vaultStatus, queryFn: () => ipc.vaultStatus() });
}

export function useSidebarCounts(enabled = true) {
  const ipc = useIpc();
  return useQuery({
    queryKey: queryKeys.sidebarCounts,
    queryFn: () => ipc.sidebarCounts(),
    enabled,
  });
}

export function useProjects(includeArchived = false, enabled = true) {
  const ipc = useIpc();
  return useQuery({
    queryKey: queryKeys.projects(includeArchived),
    queryFn: () => ipc.projectsList(includeArchived),
    enabled,
  });
}

export function useItems(filter: ItemFilter, enabled = true) {
  const ipc = useIpc();
  return useQuery({
    queryKey: queryKeys.items(filter),
    queryFn: () => ipc.itemsList(filter),
    enabled,
  });
}

export function useItem(id: string | undefined) {
  const ipc = useIpc();
  return useQuery({
    queryKey: queryKeys.item(id ?? ""),
    queryFn: () => ipc.itemGet(id as string),
    enabled: Boolean(id),
  });
}

export function useSettings() {
  const ipc = useIpc();
  return useQuery({ queryKey: queryKeys.settings, queryFn: () => ipc.settingsGet() });
}

export function useKeychainStatus(enabled = true) {
  const ipc = useIpc();
  return useQuery({
    queryKey: queryKeys.keychainStatus,
    queryFn: () => ipc.vaultKeychainStatus(),
    enabled,
    staleTime: 30_000,
  });
}

export function useAudit(itemId: string | undefined) {
  const ipc = useIpc();
  return useQuery({
    queryKey: queryKeys.audit(itemId ?? ""),
    queryFn: () => ipc.auditList(itemId as string),
    enabled: Boolean(itemId),
    retry: false,
    staleTime: 15_000,
  });
}

/* ----------------------------------------------------------- invalidation */

export function useInvalidateVaultData() {
  const qc = useQueryClient();
  return () => {
    void qc.invalidateQueries({ queryKey: itemsPrefix });
    void qc.invalidateQueries({ queryKey: itemPrefix });
    void qc.invalidateQueries({ queryKey: queryKeys.sidebarCounts });
    void qc.invalidateQueries({ queryKey: projectsPrefix });
  };
}

/* ------------------------------------------------------------ mutations */

export function useCreateItem() {
  const ipc = useIpc();
  const invalidate = useInvalidateVaultData();
  return useMutation({
    mutationFn: (input: ItemInput) => ipc.itemCreate(input),
    onSuccess: invalidate,
  });
}

export function useUpdateItem() {
  const ipc = useIpc();
  const invalidate = useInvalidateVaultData();
  return useMutation({
    mutationFn: (input: ItemUpdate) => ipc.itemUpdate(input),
    onSuccess: invalidate,
  });
}

export function useDeleteItem() {
  const ipc = useIpc();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => ipc.itemDelete(id),
    onSuccess: (_data, id) => {
      qc.removeQueries({ queryKey: queryKeys.item(id) });
      void qc.invalidateQueries({ queryKey: itemsPrefix });
      void qc.invalidateQueries({ queryKey: queryKeys.sidebarCounts });
      void qc.invalidateQueries({ queryKey: projectsPrefix });
    },
  });
}

export function useSetItemCategory() {
  const ipc = useIpc();
  const invalidate = useInvalidateVaultData();
  return useMutation({
    mutationFn: ({ id, category }: { id: string; category: string | null }) =>
      ipc.itemSetCategory(id, category),
    onSuccess: invalidate,
  });
}

export function useSetItemProject() {
  const ipc = useIpc();
  const invalidate = useInvalidateVaultData();
  return useMutation({
    mutationFn: ({ id, projectId }: { id: string; projectId: string | null }) =>
      ipc.itemSetProject(id, projectId),
    onSuccess: invalidate,
  });
}

export function useToggleFavorite() {
  const ipc = useIpc();
  const invalidate = useInvalidateVaultData();
  return useMutation({
    mutationFn: (id: string) => ipc.itemToggleFavorite(id),
    onSuccess: invalidate,
  });
}

export function useCreateProject() {
  const ipc = useIpc();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: ProjectInput) => ipc.projectCreate(input),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: projectsPrefix });
      void qc.invalidateQueries({ queryKey: queryKeys.sidebarCounts });
    },
  });
}

export function useUpdateProject() {
  const ipc = useIpc();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: ProjectUpdate) => ipc.projectUpdate(input),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: projectsPrefix });
      void qc.invalidateQueries({ queryKey: queryKeys.sidebarCounts });
      void qc.invalidateQueries({ queryKey: itemsPrefix });
    },
  });
}

export function useDeleteProject() {
  const ipc = useIpc();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => ipc.projectDelete(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: projectsPrefix });
      void qc.invalidateQueries({ queryKey: queryKeys.sidebarCounts });
      void qc.invalidateQueries({ queryKey: itemsPrefix });
    },
  });
}

export function useSaveSettings() {
  const ipc = useIpc();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (settings: Settings) => ipc.settingsSet(settings),
    onSuccess: (saved) => {
      qc.setQueryData(queryKeys.settings, saved);
      void qc.invalidateQueries({ queryKey: queryKeys.vaultStatus });
    },
  });
}

export function useUnlock() {
  const ipc = useIpc();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (password: string) => ipc.vaultUnlock(password),
    onSuccess: (status) => {
      qc.setQueryData(queryKeys.vaultStatus, status);
    },
  });
}

export function useUnlockKeychain() {
  const ipc = useIpc();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => ipc.vaultUnlockKeychain(),
    onSuccess: (status) => {
      qc.setQueryData(queryKeys.vaultStatus, status);
    },
  });
}

export function useEnableKeychain() {
  const ipc = useIpc();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (password: string) => ipc.vaultKeychainEnable(password),
    onSuccess: (status) => {
      qc.setQueryData(queryKeys.keychainStatus, status);
    },
  });
}

export function useDisableKeychain() {
  const ipc = useIpc();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => ipc.vaultKeychainDisable(),
    onSuccess: (status) => {
      qc.setQueryData(queryKeys.keychainStatus, status);
    },
  });
}

export function useCreateVault() {
  const ipc = useIpc();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (password: string) => ipc.vaultCreate(password),
    onSuccess: (status) => {
      qc.setQueryData(queryKeys.vaultStatus, status);
    },
  });
}

