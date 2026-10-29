import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createMockIpc } from "@/lib/mock-ipc";
import { queryKeys } from "@/lib/query-keys";
import { renderApp, waitForPath } from "@/test/render-app";

async function openVault() {
  const ipc = createMockIpc();
  const rendered = renderApp({ ipc, initialHref: "/" });
  await waitForPath(rendered.router, "/");
  await screen.findByTestId("vault-list");
  return { ...rendered, ipc };
}

describe("vault lock events", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("on vault://locked clears the query cache and redirects to /unlock", async () => {
    const { ipc, queryClient, router } = await openVault();

    // Cache is populated with metadata before the lock.
    await waitFor(() =>
      expect(
        queryClient.getQueryData(queryKeys.items({ unassignedOnly: false, projectId: null, kind: null })),
      ).toBeTruthy(),
    );

    act(() => {
      ipc.__emit("vault://locked");
    });

    await waitForPath(router, "/unlock");
    expect(await screen.findByRole("button", { name: "Unlock" })).toBeInTheDocument();

    // The unlocked metadata (items and item details) is gone immediately…
    const remaining = queryClient.getQueryCache().findAll().map((q) => q.queryKey);
    expect(remaining.filter((key) => ["items", "item"].includes(key[0] as string))).toEqual([]);
    // …and anything refetched afterwards hits a locked backend (errors).
    for (const q of queryClient.getQueryCache().findAll()) {
      if (q.queryKey[0] === "vault") continue;
      await waitFor(() => expect(q.state.status).toBe("error"));
    }
    const status = queryClient.getQueryData<{ unlocked: boolean }>(queryKeys.vaultStatus);
    expect(status?.unlocked).toBe(false);
    expect(ipc.__state.unlocked).toBe(false);
  });

  it("drops revealed secrets held in component state when the vault locks", async () => {
    const { ipc, router } = await openVault();
    const user = userEvent.setup();

    const list = screen.getByTestId("vault-list");
    await user.click(await within(list).findByText("Stripe live"));
    await user.click(await screen.findByRole("button", { name: "Reveal secret" }));
    await waitFor(() =>
      expect(screen.getByTestId("secret-value")).toHaveTextContent("sk_live_coffer_checkout"),
    );

    act(() => {
      ipc.__emit("vault://locked");
    });

    await waitForPath(router, "/unlock");
    expect(document.body.textContent).not.toContain("sk_live_coffer_checkout");
    expect(screen.queryByTestId("secret-value")).not.toBeInTheDocument();
  });

  it("redirects guarded routes to /unlock when the vault is locked", async () => {
    const ipc = createMockIpc();
    ipc.__emit("vault://locked");
    const { router } = renderApp({ ipc, initialHref: "/settings/security" });
    await waitForPath(router, "/unlock");
    expect(await screen.findByRole("button", { name: "Unlock" })).toBeInTheDocument();
  });

  it("shows the lock warning banner on vault://lock-warning", async () => {
    const { ipc } = await openVault();
    act(() => {
      ipc.__emit("vault://lock-warning", { seconds: 30 });
    });
    const banner = await screen.findByRole("status", undefined, { timeout: 2000 });
    expect(banner.textContent).toContain("Locking in");
    expect(banner.textContent).toContain("30s");

    // Activity dismisses the warning (the backend idle timer resets).
    await userEvent.setup().click(screen.getByRole("button", { name: "Stay unlocked" }));
    await waitFor(() =>
      expect(screen.queryByText(/Locking in/)).not.toBeInTheDocument(),
    );
  });

  it("throttles vault_touch_activity to at most once per 10 seconds", async () => {
    const { ipc } = await openVault();
    const touchSpy = vi.spyOn(ipc, "vaultTouchActivity");
    const user = userEvent.setup();

    await user.keyboard("a");
    await user.keyboard("b");
    await user.keyboard("c");
    await user.click(document.body);

    expect(touchSpy.mock.calls.length).toBeLessThanOrEqual(1);
  });
});
