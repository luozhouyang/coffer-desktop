import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { createMockIpc } from "@/lib/mock-ipc";
import { renderApp, waitForPath } from "@/test/render-app";

describe("unlock page", () => {
  const startLocked = () => {
    const ipc = createMockIpc();
    ipc.__emit("vault://locked");
    return ipc;
  };

  it("validates an empty password before calling the backend", async () => {
    const ipc = startLocked();
    const unlockSpy = vi.spyOn(ipc, "vaultUnlock");
    const { router } = renderApp({ ipc, initialHref: "/unlock" });
    const user = userEvent.setup();

    await waitForPath(router, "/unlock");

    await user.click(screen.getByRole("button", { name: "Unlock" }));

    expect(await screen.findByText("Enter your master password.")).toBeInTheDocument();
    expect(unlockSpy).not.toHaveBeenCalled();
  });

  it("shows an inline error for a wrong password", async () => {
    const ipc = startLocked();
    ipc.__setPassword("correct-horse");
    const { router } = renderApp({ ipc, initialHref: "/unlock" });
    const user = userEvent.setup();
    await waitForPath(router, "/unlock");

    await user.type(screen.getByLabelText(/Master password/), "wrong-password");
    await user.click(screen.getByRole("button", { name: "Unlock" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Incorrect master password.");
    expect(router.state.location.pathname).toBe("/unlock");
  });

  it("unlocks and navigates into the vault", async () => {
    const ipc = startLocked();
    const unlockSpy = vi.spyOn(ipc, "vaultUnlock");
    const { router } = renderApp({ ipc, initialHref: "/unlock" });
    const user = userEvent.setup();
    await waitForPath(router, "/unlock");

    await user.type(screen.getByLabelText(/Master password/), "coffer");
    await user.click(screen.getByRole("button", { name: "Unlock" }));

    await waitForPath(router, "/");
    expect(unlockSpy).toHaveBeenCalledWith("coffer");
    expect(await screen.findByTestId("vault-list", {}, { timeout: 4000 })).toBeInTheDocument();
    expect(ipc.__state.unlocked).toBe(true);
  });

  it("renders the no-recovery notice", async () => {
    const { router } = renderApp({ ipc: startLocked(), initialHref: "/unlock" });
    await waitForPath(router, "/unlock");
    expect(screen.getAllByText(/No recovery\./).length).toBeGreaterThan(0);
  });

  it("hides Touch ID unlock when the platform has no biometric keychain", async () => {
    const ipc = startLocked();
    const statusSpy = vi.spyOn(ipc, "vaultKeychainStatus");
    const { router } = renderApp({ ipc, initialHref: "/unlock" });
    await waitForPath(router, "/unlock");

    await waitFor(() => expect(statusSpy).toHaveBeenCalled());
    expect(screen.queryByRole("button", { name: /Touch ID/ })).not.toBeInTheDocument();
  });

  it("offers Touch ID only after it has been enabled", async () => {
    const ipc = createMockIpc({ keychainSupported: true });
    // Enabling happens from Settings while unlocked; do that before locking.
    await ipc.vaultKeychainEnable("coffer");
    ipc.__emit("vault://locked");
    const { router } = renderApp({ ipc, initialHref: "/unlock" });
    await waitForPath(router, "/unlock");

    const button = await screen.findByRole("button", { name: "Unlock with Touch ID" });
    expect(button).toBeEnabled();

    const unlockSpy = vi.spyOn(ipc, "vaultUnlockKeychain");
    await userEvent.setup().click(button);

    await waitForPath(router, "/");
    expect(unlockSpy).toHaveBeenCalledTimes(1);
    expect(ipc.__state.unlocked).toBe(true);
  });
});

describe("setup page", () => {
  it("requires acknowledging that there is no recovery", async () => {
    const ipc = createMockIpc();
    ipc.__resetToFirstRun();
    const createSpy = vi.spyOn(ipc, "vaultCreate");
    const { router } = renderApp({ ipc, initialHref: "/setup" });
    const user = userEvent.setup();
    await waitForPath(router, "/setup");

    const form = screen.getByRole("button", { name: /Create vault/ }).closest("form") as HTMLElement;
    await user.type(within(form).getByLabelText(/Master password/), "correct-horse-battery");
    await user.type(within(form).getByLabelText(/Confirm password/), "correct-horse-battery");
    await user.click(within(form).getByRole("button", { name: /Create vault/ }));

    expect(await screen.findByText("Confirm that you understand there is no recovery.")).toBeInTheDocument();
    expect(createSpy).not.toHaveBeenCalled();

    // The checkbox is required and unlocks submission.
    await user.click(screen.getByRole("checkbox", { name: /I understand there is no recovery/ }));
    await user.click(within(form).getByRole("button", { name: /Create vault/ }));

    await waitForPath(router, "/");
    expect(createSpy).toHaveBeenCalledWith("correct-horse-battery");
  });

  it("rejects mismatched passwords", async () => {
    const ipc = createMockIpc();
    ipc.__resetToFirstRun();
    const { router } = renderApp({ ipc, initialHref: "/setup" });
    const user = userEvent.setup();
    await waitForPath(router, "/setup");

    const form = screen.getByRole("button", { name: /Create vault/ }).closest("form") as HTMLElement;
    await user.type(within(form).getByLabelText(/Master password/), "correct-horse-battery");
    await user.type(within(form).getByLabelText(/Confirm password/), "different-password");
    await user.click(within(form).getByRole("button", { name: /Create vault/ }));

    expect(await screen.findByText("Passwords do not match.")).toBeInTheDocument();
  });
});
