import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createMockIpc, type MockIpc } from "@/lib/mock-ipc";
import { queryKeys } from "@/lib/query-keys";
import type { VaultStatus } from "@/lib/ipc-types";
import { renderApp, spyToasts, waitForPath } from "@/test/render-app";

const dialogMocks = vi.hoisted(() => ({
  pickBackupSavePath: vi.fn<(name: string) => Promise<string | null>>(),
  pickBackupOpenPath: vi.fn<() => Promise<string | null>>(),
}));

vi.mock("@/lib/file-dialog", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/file-dialog")>();
  return { ...actual, ...dialogMocks };
});

beforeEach(() => {
  dialogMocks.pickBackupSavePath.mockReset().mockResolvedValue("/tmp/coffer-backup.coffer");
  dialogMocks.pickBackupOpenPath.mockReset().mockResolvedValue("/tmp/coffer-backup.coffer");
});

/** Exports the seeded vault, maybe changes it, then wipes the device. */
async function firstRunAfterBackup({ mutateBeforeReset = false } = {}) {
  const ipc = createMockIpc();
  await ipc.backupExport("/tmp/coffer-backup.coffer", "coffer");
  if (mutateBeforeReset) {
    const items = await ipc.itemsList({ unassignedOnly: false });
    const keep = items[0];
    for (const item of items.slice(1)) await ipc.itemDelete(item.id);
    void keep;
  }
  ipc.__resetToFirstRun();
  return ipc;
}

async function openRestoreDialog(ipc: MockIpc) {
  ipc.__resetToFirstRun();
  const rendered = renderApp({ ipc, initialHref: "/setup" });
  await waitForPath(rendered.router, "/setup");
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Restore from backup" }));
  const dialog = await screen.findByRole("dialog");
  return { ...rendered, user, dialog };
}

async function pickFileAndEnterPassword(user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement, pw: string) {
  await user.click(within(dialog).getByRole("button", { name: "Choose file…" }));
  await user.type(await within(dialog).findByLabelText(/Master password for this backup/), pw);
  await user.click(within(dialog).getByRole("button", { name: "Continue" }));
}

describe("restore from backup", () => {
  it("offers the restore entry on Setup only while the vault is uninitialized", async () => {
    const firstRunIpc = createMockIpc();
    firstRunIpc.__resetToFirstRun();
    const firstRun = renderApp({ ipc: firstRunIpc, initialHref: "/setup" });
    await waitForPath(firstRun.router, "/setup");
    expect(await screen.findByRole("button", { name: "Restore from backup" })).toBeInTheDocument();
    firstRun.unmount();

    const existing = renderApp({ ipc: createMockIpc(), initialHref: "/setup" });
    await waitForPath(existing.router, "/");
  });

  it("is not shown on the unlock page", async () => {
    const ipc = createMockIpc();
    ipc.__emit("vault://locked");
    const { router } = renderApp({ ipc, initialHref: "/unlock" });
    await waitForPath(router, "/unlock");
    await screen.findByRole("button", { name: "Unlock" });
    expect(screen.queryByRole("button", { name: "Restore from backup" })).not.toBeInTheDocument();
  });

  it("restores an exported backup, updates vault status, and navigates into the vault", async () => {
    const toasts = spyToasts();
    const ipc = await firstRunAfterBackup({ mutateBeforeReset: true });
    const commitSpy = vi.spyOn(ipc, "backupRestoreCommit");
    const { router, queryClient, user, dialog } = await openRestoreDialog(ipc);

    await pickFileAndEnterPassword(user, dialog, "coffer");

    // Preview: date, counts, and app version come from the backend.
    expect(await within(dialog).findByText("Secrets")).toBeInTheDocument();
    expect(within(dialog).getByText(String(ipc.__state.backupArchive?.itemCount))).toBeInTheDocument();
    expect(within(dialog).getByText("0.1.0")).toBeInTheDocument();
    // The password is cleared once the preview opens.
    expect(dialogMocks.pickBackupOpenPath).toHaveBeenCalledTimes(1);

    await user.click(within(dialog).getByRole("button", { name: "Restore vault" }));

    await waitForPath(router, "/");
    expect(commitSpy).toHaveBeenCalledTimes(1);
    const status = queryClient.getQueryData<VaultStatus>(queryKeys.vaultStatus);
    expect(status).toMatchObject({ initialized: true, unlocked: true });
    expect(status?.counts.all).toBe(ipc.__state.backupArchive?.itemCount);
    expect(await screen.findByTestId("vault-list", {}, { timeout: 4000 })).toBeInTheDocument();
    expect(toasts.success).toHaveBeenCalledWith("Vault restored");
  });

  it("shows the normal wrong-password error and stays open", async () => {
    const ipc = await firstRunAfterBackup();
    const beginSpy = vi.spyOn(ipc, "backupRestoreBegin");
    const { router, user, dialog } = await openRestoreDialog(ipc);

    await pickFileAndEnterPassword(user, dialog, "not-the-password");

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Incorrect master password.");
    expect(beginSpy).toHaveBeenCalledWith("/tmp/coffer-backup.coffer", "not-the-password");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/setup");
    expect(ipc.__state.initialized).toBe(false);
    expect(within(dialog).getByLabelText(/Master password for this backup/)).toHaveValue("");
  });

  it("reports an unsupported file with the shared message", async () => {
    const ipc = createMockIpc();
    dialogMocks.pickBackupOpenPath.mockResolvedValueOnce("/tmp/notes.txt");
    const { user, dialog } = await openRestoreDialog(ipc);
    await pickFileAndEnterPassword(user, dialog, "coffer");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("This backup file is not supported.");
  });

  it("cancels the pending session when the dialog is closed after a preview", async () => {
    const ipc = await firstRunAfterBackup();
    const cancelSpy = vi.spyOn(ipc, "backupRestoreCancel");
    const { user, dialog } = await openRestoreDialog(ipc);

    await pickFileAndEnterPassword(user, dialog, "coffer");
    await within(dialog).findByText("Secrets");
    expect(ipc.__state.restoreSessions).toBe(1);

    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(cancelSpy).toHaveBeenCalledTimes(1));
    expect(ipc.__state.restoreSessions).toBe(0);
    expect(ipc.__state.initialized).toBe(false);
  });
});
