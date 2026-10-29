import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { createMockIpc } from "@/lib/mock-ipc";
import { renderApp, waitForPath } from "@/test/render-app";

async function openVault(initialHref = "/") {
  const ipc = createMockIpc();
  const rendered = renderApp({ ipc, initialHref });
  await waitForPath(rendered.router, initialHref.split("?")[0]);
  await screen.findByTestId("vault-list");
  return { ...rendered, ipc };
}

function rows() {
  return screen.getByTestId("vault-list").querySelectorAll("[data-item-id]");
}

describe("vault list and detail", () => {
  it("selects an item and shows metadata only — never the secret", async () => {
    const { router, ipc } = await openVault();
    const user = userEvent.setup();

    const list = screen.getByTestId("vault-list");
    await within(list).findByText("OpenAI production");
    await user.click(within(list).getByText("OpenAI production"));

    await waitFor(() => expect(router.state.location.pathname).toMatch(/^\/item\//));
    // Opening an item keeps the current filters in the URL (none here).
    expect(router.state.location.search).toEqual({});

    expect(await screen.findByTestId("detail-title")).toHaveTextContent("OpenAI production");
    // The detail page lives in the second <main> (the first is the sidebar inset).
    const detail = screen.getAllByRole("main")[1];
    expect(detail).toBeTruthy();
    expect(within(detail).getByText("AI / LLM")).toBeInTheDocument();
    expect(within(detail).getAllByText("Coffer dashboard").length).toBeGreaterThan(0);
    expect(within(detail).getByText("Rule match")).toBeInTheDocument();
    expect(within(detail).getAllByText(/clipboard clears in 30s/).length).toBeGreaterThan(0);

    // The secret is neither in the DOM nor revealed without an explicit click.
    await expect(ipc.itemRevealSecret("nonexistent")).rejects.toMatchObject({ code: "notFound" });
    expect(detail.textContent).not.toContain("{{API_KEY_");
    const secretEl = within(detail).getByTestId("secret-value");
    expect(secretEl).toHaveTextContent("••••");
    expect(secretEl).not.toHaveAttribute("data-revealed");
    expect(within(detail).getByRole("button", { name: "Reveal secret" })).toBeInTheDocument();
    expect(within(detail).getByRole("button", { name: "Copy secret" })).toBeInTheDocument();
  });

  it("preserves project filters when opening an item", async () => {
    const { router } = await openVault("/?project=p-orbit");
    const user = userEvent.setup();
    const list = screen.getByTestId("vault-list");
    await within(list).findByText("Stripe live");
    await user.click(within(list).getByText("Stripe live"));

    await waitFor(() => expect(router.state.location.pathname).toMatch(/^\/item\//));
    expect(router.state.location.search).toMatchObject({ project: "p-orbit" });
  });

  it("reveals a secret on demand and hides it again", async () => {
    const ipc = createMockIpc();
    const rendered = renderApp({ ipc, initialHref: "/" });
    await waitForPath(rendered.router, "/");
    const list = await screen.findByTestId("vault-list");
    await within(list).findByText("Stripe live");
    await userEvent.setup().click(within(list).getByText("Stripe live"));

    const revealButton = await screen.findByRole("button", { name: "Reveal secret" });
    await userEvent.setup().click(revealButton);

    const secret = await screen.findByTestId("secret-value");
    await waitFor(() => expect(secret).toHaveAttribute("data-revealed", "true"));
    expect(secret).toHaveTextContent("sk_live_coffer_checkout");

    await userEvent.setup().click(screen.getByRole("button", { name: "Hide secret" }));
    await waitFor(() => expect(screen.getByTestId("secret-value")).not.toHaveAttribute("data-revealed"));
    expect(screen.getByTestId("secret-value")).toHaveTextContent("••••");
  });

  it("shows the kind chips and empty-set copy for an empty filter", async () => {
    await openVault("/?project=unassigned&kind=env");
    expect(await screen.findByText("No secrets here yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: ".env" })).toHaveAttribute("aria-pressed", "true");
  });

  it("renders rows with icon tile, title, category badge and relative time", async () => {
    await openVault();
    const list = screen.getByTestId("vault-list");
    await within(list).findByText("OpenAI production");
    const row = Array.from(rows()).find((r) => r.textContent?.includes("OpenAI production")) as HTMLElement;
    expect(within(row).getByTestId("category-badge")).toHaveTextContent("AI / LLM");
    expect(row.querySelector(".font-mono")).toBeTruthy();
  });
});
