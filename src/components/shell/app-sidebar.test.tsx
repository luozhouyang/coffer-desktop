import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { createMockIpc } from "@/lib/mock-ipc";
import { renderApp, waitForPath } from "@/test/render-app";

async function openVault() {
  const ipc = createMockIpc();
  const rendered = renderApp({ ipc, initialHref: "/" });
  await waitForPath(rendered.router, "/");
  await screen.findByTestId("vault-list");
  // Sidebar counts arrive with a fetch; wait for the seeded totals.
  await screen.findAllByText("All secrets");
  // The sidebar renders twice (desktop + mobile Sheet); use the desktop copy.
  const sidebar = document.querySelector<HTMLElement>('[data-slot="sidebar-container"]') ?? document.body;
  await within(sidebar)
    .findAllByText("10", {}, { timeout: 3000 })
    .catch(() => undefined);
  return { ...rendered, sidebar };
}

function rowTitles(container: HTMLElement) {
  return Array.from(container.querySelectorAll("[data-item-id]")).map((el) => el.textContent ?? "");
}

describe("vault sidebar", () => {
  it("renders all / unassigned and per-project counts from sidebar_counts", async () => {
    const { ipc, sidebar } = await openVault();
    expect(within(sidebar).getByRole("link", { name: /All secrets/ })).toBeInTheDocument();
    expect(within(sidebar).getByRole("link", { name: /Unassigned/ })).toBeInTheDocument();

    const counts = await ipc.sidebarCounts();
    expect(counts.all).toBe(10);
    expect(counts.unassigned).toBe(2);
    // Every project row is rendered with its badge equal to the server count.
    for (const p of counts.projects) {
      expect(within(sidebar).getAllByText(String(p.count)).length).toBeGreaterThan(0);
    }
    expect(within(sidebar).getByText("Coffer dashboard")).toBeInTheDocument();
    expect(within(sidebar).getByText("Orbit API")).toBeInTheDocument();
    expect(within(sidebar).getByText("Lab experiments")).toBeInTheDocument();
  });

  it("filters the list to Unassigned and preserves it in the URL", async () => {
    const { router, sidebar } = await openVault();
    const user = userEvent.setup();

    await user.click(within(sidebar).getByRole("link", { name: /Unassigned/ }));
    await waitForPath(router, "/");

    expect(router.state.location.search).toMatchObject({ project: "unassigned" });
    expect(await screen.findByRole("heading", { level: 1, name: "Unassigned" })).toBeInTheDocument();

    const list = screen.getByTestId("vault-list");
    await within(list).findByText("GitHub personal");
    const titles = rowTitles(list);
    expect(titles.some((t) => t.includes("GitHub personal"))).toBe(true);
    expect(titles.some((t) => t.includes("Slack webhook"))).toBe(true);
    expect(titles.some((t) => t.includes("OpenAI production"))).toBe(false);
  });

  it("filters by project when a project row is clicked", async () => {
    const { router, sidebar } = await openVault();
    const user = userEvent.setup();

    await user.click(within(sidebar).getByRole("link", { name: /Orbit API/ }));
    await waitForPath(router, "/");

    expect(router.state.location.search).toMatchObject({ project: "p-orbit" });
    expect(await screen.findByRole("heading", { level: 1, name: "Orbit API" })).toBeInTheDocument();
    const list = screen.getByTestId("vault-list");
    await within(list).findByText("Stripe live");
    const titles = rowTitles(list);
    expect(titles.some((t) => t.includes("Stripe live"))).toBe(true);
    expect(titles.some((t) => t.includes("Vercel deploy token"))).toBe(true);
    expect(titles.some((t) => t.includes("GitHub personal"))).toBe(false);
  });

  it("kind chips filter within the current project filter", async () => {
    const { router, sidebar } = await openVault();
    const user = userEvent.setup();

    await user.click(within(sidebar).getByRole("link", { name: /Orbit API/ }));
    await waitForPath(router, "/");
    await user.click(screen.getByRole("button", { name: "Passwords" }));

    expect(router.state.location.search).toMatchObject({ project: "p-orbit", kind: "password" });
    const list = screen.getByTestId("vault-list");
    await within(list).findByText("AWS console");
    const titles = rowTitles(list);
    expect(titles.some((t) => t.includes("AWS console"))).toBe(true);
    expect(titles.some((t) => t.includes("Stripe live"))).toBe(false);
  });

  it("locks the vault from the sidebar and returns to unlock", async () => {
    const { router, sidebar, ipc } = await openVault();
    const user = userEvent.setup();

    await user.click(within(sidebar).getByRole("button", { name: /Lock vault/ }));
    await waitForPath(router, "/unlock");
    expect(ipc.__state.unlocked).toBe(false);
    expect(await screen.findByRole("button", { name: "Unlock" })).toBeInTheDocument();
  });

  it("creates a project from the sidebar dialog and refreshes the list", async () => {
    const { sidebar } = await openVault();
    const user = userEvent.setup();

    await user.click(within(sidebar).getByRole("button", { name: "New project" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/Name/), "Payments team");
    await user.click(within(dialog).getByRole("button", { name: "Create project" }));

    expect(await within(sidebar).findByText("Payments team")).toBeInTheDocument();
  });
});
