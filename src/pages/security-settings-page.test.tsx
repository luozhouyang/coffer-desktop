import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { createMockIpc } from "@/lib/mock-ipc";
import { renderApp, waitForPath } from "@/test/render-app";

async function openSettings() {
  const ipc = createMockIpc();
  const rendered = renderApp({ ipc, initialHref: "/settings/security" });
  await waitForPath(rendered.router, "/settings/security");
  await screen.findByRole("heading", { level: 1, name: "Security" });
  return { ...rendered, ipc };
}

describe("security settings", () => {
  it("loads current settings and saves changes via settings_set", async () => {
    const { ipc } = await openSettings();
    const setSpy = vi.spyOn(ipc, "settingsSet");
    const user = userEvent.setup();

    // Auto-lock select reflects the persisted value.
    const autoLock = screen.getByRole("combobox", { name: "Lock after inactivity" });
    await waitFor(() => expect(autoLock).toHaveTextContent("5 minutes"));

    await user.click(autoLock);
    await user.click(await screen.findByRole("option", { name: "15 minutes" }));

    await waitFor(() => expect(setSpy).toHaveBeenCalledWith(expect.objectContaining({ autoLockSecs: 900 })));
    expect(ipc.__state.settings.autoLockSecs).toBe(900);

    const clipboard = screen.getByRole("combobox", { name: "Clear clipboard after copying" });
    await user.click(clipboard);
    await user.click(await screen.findByRole("option", { name: "60 seconds" }));
    await waitFor(() => expect(setSpy).toHaveBeenCalledWith(expect.objectContaining({ clipboardClearSecs: 60 })));

    const deep = screen.getByRole("switch", { name: "Deep search secret values" });
    expect(deep).toHaveAttribute("aria-checked", "false");
    await user.click(deep);
    await waitFor(() => expect(setSpy).toHaveBeenCalledWith(expect.objectContaining({ deepSearch: true })));
    expect(ipc.__state.settings.deepSearch).toBe(true);
  });

  it("shows an error state when settings cannot be loaded", async () => {
    const ipc = createMockIpc();
    vi.spyOn(ipc, "settingsGet").mockRejectedValue({ code: "io", message: "Database unavailable." });
    renderApp({ ipc, initialHref: "/settings/security" });
    expect(await screen.findByText("Could not load settings")).toBeInTheDocument();
    expect(screen.getByText("Database unavailable.")).toBeInTheDocument();
  });

  it("redirects /settings to /settings/security", async () => {
    const { router } = await openSettings();
    void router;
    const ipc = createMockIpc();
    const rendered = renderApp({ ipc, initialHref: "/settings" });
    await waitForPath(rendered.router, "/settings/security");
  });

  it("is reachable from the sidebar and preserves the vault filter for the way back", async () => {
    const ipc = createMockIpc();
    const { router } = renderApp({ ipc, initialHref: "/?project=p-orbit" });
    await waitForPath(router, "/");
    await screen.findByTestId("vault-list");
    const user = userEvent.setup();
    await new Promise((r) => setTimeout(r, 50));

    await user.click(screen.getByRole("link", { name: /Settings/ }));
    await waitForPath(router, "/settings/security");
    expect(router.state.location.search).toMatchObject({ project: "p-orbit" });

    const back = await screen.findByRole("button", { name: /Back to vault/ });

    await user.click(back);
    await waitForPath(router, "/");
    expect(router.state.location.search).toMatchObject({ project: "p-orbit" });
    expect(within(document.querySelector('[data-slot="sidebar-container"]') as HTMLElement).queryByText("Projects")).toBeInTheDocument();
  });
});
