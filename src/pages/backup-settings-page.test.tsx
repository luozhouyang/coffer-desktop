import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createMockIpc } from "@/lib/mock-ipc";
import { renderApp, spyToasts, waitForPath } from "@/test/render-app";

const dialogMocks = vi.hoisted(() => ({
  pickBackupSavePath: vi.fn<(name: string) => Promise<string | null>>(),
  pickBackupOpenPath: vi.fn<() => Promise<string | null>>(),
}));

// Keep `fileNameFromPath` real; stub only the native dialogs.
vi.mock("@/lib/file-dialog", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/file-dialog")>();
  return { ...actual, ...dialogMocks };
});

beforeEach(() => {
  dialogMocks.pickBackupSavePath.mockReset().mockResolvedValue("/tmp/coffer-backup-20261029-101500.coffer");
  dialogMocks.pickBackupOpenPath.mockReset().mockResolvedValue("/tmp/coffer-backup.coffer");
});

function settingsNav() {
  const nav = document.querySelector('[data-slot="sidebar-container"][aria-label="Settings navigation"]');
  expect(nav).not.toBeNull();
  return nav as HTMLElement;
}

async function openBackupSettings(ipc = createMockIpc()) {
  const rendered = renderApp({ ipc, initialHref: "/settings/backup" });
  await waitForPath(rendered.router, "/settings/backup");
  await screen.findByRole("heading", { level: 1, name: "Backup" });
  return { ...rendered, ipc };
}

describe("backup settings", () => {
  it("renders the page and lists Backup in the settings sidebar", async () => {
    await openBackupSettings();
    expect(screen.getByRole("heading", { level: 2, name: "Encrypted backup" })).toBeInTheDocument();
    expect(screen.getByText(/Treat the backup like your master password/)).toBeInTheDocument();
    const nav = settingsNav();
    expect(within(nav).getByRole("link", { name: /Backup/ })).toHaveAttribute("href", "/settings/backup");
  });

  it("navigates to Backup from the sidebar", async () => {
    const { router } = renderApp({ ipc: createMockIpc(), initialHref: "/settings/security" });
    await waitForPath(router, "/settings/security");
    await screen.findByRole("heading", { level: 1, name: "Security" });
    const nav = settingsNav();
    await userEvent.setup().click(within(nav).getByRole("link", { name: /Backup/ }));
    await waitForPath(router, "/settings/backup");
    await screen.findByRole("heading", { level: 1, name: "Backup" });
  });

  it("opens the export dialog and asks for the location first", async () => {
    const { ipc } = await openBackupSettings();
    const exportSpy = vi.spyOn(ipc, "backupExport");
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Export encrypted backup…" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByLabelText(/Master password/)).not.toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Choose location…" }));
    await waitFor(() => expect(dialogMocks.pickBackupSavePath).toHaveBeenCalledTimes(1));
    expect(dialogMocks.pickBackupSavePath.mock.calls[0][0]).toMatch(/^coffer-backup-\d{8}-\d{6}\.coffer$/);

    expect(await within(dialog).findByLabelText(/Master password/)).toBeInTheDocument();
    expect(within(dialog).getByText("coffer-backup-20261029-101500.coffer")).toBeInTheDocument();
    expect(exportSpy).not.toHaveBeenCalled();
  });

  it("exports with the picked path and master password", async () => {
    const toasts = spyToasts();
    const { ipc } = await openBackupSettings();
    const exportSpy = vi.spyOn(ipc, "backupExport");
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Export encrypted backup…" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Choose location…" }));
    await user.type(await within(dialog).findByLabelText(/Master password/), "coffer");
    await user.click(within(dialog).getByRole("button", { name: "Export" }));

    await waitFor(() =>
      expect(exportSpy).toHaveBeenCalledWith("/tmp/coffer-backup-20261029-101500.coffer", "coffer"),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(toasts.success).toHaveBeenCalledWith("Backup exported", {
      description: "coffer-backup-20261029-101500.coffer",
    });
    expect(ipc.__state.backupArchive?.itemCount).toBeGreaterThan(0);
  });

  it("keeps the dialog open with an inline error for a wrong password and clears the field", async () => {
    const { ipc } = await openBackupSettings();
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Export encrypted backup…" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Choose location…" }));
    const password = await within(dialog).findByLabelText(/Master password/);
    await user.type(password, "wrong-password");
    await user.click(within(dialog).getByRole("button", { name: "Export" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Incorrect master password.");
    expect(within(dialog).getByLabelText(/Master password/)).toHaveValue("");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(ipc.__state.backupArchive).toBeNull();
  });

  it("does nothing when the save dialog is cancelled", async () => {
    dialogMocks.pickBackupSavePath.mockResolvedValueOnce(null);
    await openBackupSettings();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Export encrypted backup…" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Choose location…" }));
    await waitFor(() => expect(dialogMocks.pickBackupSavePath).toHaveBeenCalled());
    expect(within(dialog).queryByLabelText(/Master password/)).not.toBeInTheDocument();
  });

  it("offers Touch ID after biometric unlock is enabled and exports without a password", async () => {
    const ipc = createMockIpc({ keychainSupported: true, keychainAvailable: true });
    await ipc.vaultKeychainEnable("coffer");
    const exportSpy = vi.spyOn(ipc, "backupExportKeychain");
    await openBackupSettings(ipc);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Export encrypted backup…" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Choose location…" }));
    const touchId = await within(dialog).findByRole("button", { name: "Use Touch ID" });
    // The password path stays available as a fallback.
    expect(within(dialog).getByLabelText(/Master password/)).toBeInTheDocument();

    await user.click(touchId);
    await waitFor(() =>
      expect(exportSpy).toHaveBeenCalledWith("/tmp/coffer-backup-20261029-101500.coffer"),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(ipc.__state.backupArchive?.itemCount).toBeGreaterThan(0);
  });

  it("keeps the password form as the only option when Touch ID is disabled", async () => {
    await openBackupSettings();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Export encrypted backup…" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Choose location…" }));
    await within(dialog).findByLabelText(/Master password/);
    expect(within(dialog).queryByRole("button", { name: "Use Touch ID" })).not.toBeInTheDocument();
  });
});
