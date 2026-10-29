import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { createMockIpc } from "@/lib/mock-ipc";
import { renderApp, waitForPath } from "@/test/render-app";

async function openVault() {
  const ipc = createMockIpc();
  const rendered = renderApp({ ipc, initialHref: "/" });
  await waitForPath(rendered.router, "/");
  await screen.findByTestId("vault-list");
  return { ...rendered, ipc };
}

describe("command palette", () => {
  it("opens with ⌘K and debounces the search while discarding stale responses", async () => {
    const { ipc } = await openVault();
    const searchSpy = vi.spyOn(ipc, "search");
    const user = userEvent.setup();

    await user.keyboard("{Meta>}k{/Meta}");
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByPlaceholderText("Search vault…");

    // Initial (recent) query fires once the palette opens.
    await waitFor(() => expect(searchSpy).toHaveBeenCalled());

    searchSpy.mockClear();
    const input = dialog.querySelector<HTMLInputElement>('input[cmdk-input]')!;
    await user.type(input, "str");

    // Debounced: nothing fired synchronously for keystrokes.
    expect(searchSpy).not.toHaveBeenCalledWith("s", expect.anything());
    // After the 120ms debounce only the final query runs.
    await waitFor(() => expect(searchSpy).toHaveBeenCalledWith("str", expect.objectContaining({ deep: false })));
    expect(searchSpy.mock.calls.some(([q]) => q === "s" || q === "st")).toBe(false);

    expect(await within(dialog).findByText("Stripe live")).toBeInTheDocument();
  });

  it("copying activates copy_secret and shows a toast with the clear delay", async () => {
    const { ipc } = await openVault();
    const copySpy = vi.spyOn(ipc, "copySecret");
    const revealSpy = vi.spyOn(ipc, "itemRevealSecret");
    const user = userEvent.setup();

    await user.keyboard("{Meta>}k{/Meta}");
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByPlaceholderText("Search vault…");
    await user.type(within(dialog).getByPlaceholderText("Search vault…"), "stripe");

    const result = await within(dialog).findByTestId("palette-result");
    // cmdk selects on click / ↵ of the active option; clicking is the
    // deterministic equivalent of ↑/↓ + ↵ in jsdom.
    await user.click(result);

    await waitFor(() => expect(copySpy).toHaveBeenCalledTimes(1));
    expect(copySpy).toHaveBeenCalledWith(result.getAttribute("data-value") ?? expect.any(String));
    // Copy returns only { clearAfterSecs } — nothing secret is fetched.
    expect(revealSpy).not.toHaveBeenCalled();

    const toast = await screen.findByText(/Copied/);
    expect(toast.closest("li")?.textContent ?? document.body.textContent).toContain("Clipboard clears in 30s");
    expect(result.textContent).not.toMatch(/sk_live|SECRET_|API_KEY_/);
  });

  it("filters by project and reports the empty state", async () => {
    const { ipc } = await openVault();
    const searchSpy = vi.spyOn(ipc, "search");
    const user = userEvent.setup();

    await user.keyboard("{Meta>}k{/Meta}");
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByPlaceholderText("Search vault…");
    await user.type(within(dialog).getByPlaceholderText("Search vault…"), "zzzz-nothing");
    expect(await within(dialog).findByText(/No matches/)).toBeInTheDocument();
    expect(searchSpy).toHaveBeenCalledWith("zzzz-nothing", expect.objectContaining({ unassignedOnly: false }));
  });

  it("closes on Escape", async () => {
    await openVault();
    const user = userEvent.setup();
    await user.keyboard("{Meta>}k{/Meta}");
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });
});
