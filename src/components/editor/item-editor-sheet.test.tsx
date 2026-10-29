import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { createMockIpc } from "@/lib/mock-ipc";
import { renderApp, waitForPath } from "@/test/render-app";
import { uiStore } from "@/app/ui-store";

async function openVault() {
  const ipc = createMockIpc();
  const rendered = renderApp({ ipc, initialHref: "/" });
  await waitForPath(rendered.router, "/");
  await screen.findByTestId("vault-list");
  // Let the list/query notifications settle so the header CTA is interactive.
  await new Promise((resolve) => setTimeout(resolve, 50));
  return { ...rendered, ipc };
}

describe("item editor sheet", () => {
  it("validates title and secret before calling item_create", async () => {
    const { ipc } = await openVault();
    const createSpy = vi.spyOn(ipc, "itemCreate");
    const user = userEvent.setup();

    await user.click(screen.getAllByRole("button").find((b) => b.textContent?.trim() === "New")!);
    const sheet = await screen.findByRole("dialog");
    await user.click(within(sheet).getByRole("button", { name: /^Save/ }));

    expect(await within(sheet).findByText("Title is required.")).toBeInTheDocument();
    expect(createSpy).not.toHaveBeenCalled();

    await user.type(within(sheet).getByLabelText(/^Title$/), "New key");
    await user.click(within(sheet).getByRole("button", { name: /^Save/ }));
    expect(await within(sheet).findByText("Secret is required.")).toBeInTheDocument();
    expect(createSpy).not.toHaveBeenCalled();
  });

  it("creates an item with the entered values", async () => {
    void 0;
    const { ipc } = await openVault();
    const createSpy = vi.spyOn(ipc, "itemCreate");
    const user = userEvent.setup();

    await user.click(screen.getAllByRole("button").find((b) => b.textContent?.trim() === "New")!);
    const sheet = await screen.findByRole("dialog");
    await user.type(await within(sheet).findByLabelText(/^Title$/), "Fresh Stripe key");
    // The secret field masks input unless "Show" is toggled.
    await user.click(within(sheet).getByRole("button", { name: "Show secret" }));
    await user.type(within(sheet).getByLabelText("Secret"), "sk_live_fresh_value");
    await user.click(within(sheet).getByRole("button", { name: /^Save/ }));

    await waitFor(() => expect(createSpy).toHaveBeenCalledTimes(1));
    expect(createSpy.mock.calls[0][0]).toMatchObject({
      kind: "apiKey",
      title: "Fresh Stripe key",
      secret: "sk_live_fresh_value",
      projectId: null,
    });

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    const list = screen.getByTestId("vault-list");
    expect(await within(list).findByText("Fresh Stripe key")).toBeInTheDocument();
  });

  it("stores the exact secret typed while masked and preserves it across show/hide", async () => {
    const { ipc } = await openVault();
    const createSpy = vi.spyOn(ipc, "itemCreate");
    const user = userEvent.setup();

    await user.click(screen.getAllByRole("button").find((b) => b.textContent?.trim() === "New")!);
    const sheet = await screen.findByRole("dialog");
    await user.type(await within(sheet).findByLabelText(/^Title$/), "Masked key");

    // Masking is visual only: typing must keep the real value in the DOM, not
    // append to bullet glyphs (which previously corrupted the stored secret).
    const secret = within(sheet).getByLabelText("Secret");
    await user.type(secret, "sk_live_masked_value_42");
    expect(secret).toHaveValue("sk_live_masked_value_42");

    await user.click(within(sheet).getByRole("button", { name: "Show secret" }));
    expect(within(sheet).getByLabelText("Secret")).toHaveValue("sk_live_masked_value_42");
    await user.click(within(sheet).getByRole("button", { name: "Hide secret" }));
    expect(within(sheet).getByLabelText("Secret")).toHaveValue("sk_live_masked_value_42");

    await user.click(within(sheet).getByRole("button", { name: /^Save/ }));
    await waitFor(() => expect(createSpy).toHaveBeenCalledTimes(1));
    expect(createSpy.mock.calls[0][0]).toMatchObject({
      kind: "apiKey",
      title: "Masked key",
      secret: "sk_live_masked_value_42",
    });
  });

  it("creates a password item with a manually chosen category", async () => {
    const { ipc } = await openVault();
    const createSpy = vi.spyOn(ipc, "itemCreate");
    const setCategorySpy = vi.spyOn(ipc, "itemSetCategory");
    const user = userEvent.setup();

    await user.click(screen.getAllByRole("button").find((b) => b.textContent?.trim() === "New")!);
    const sheet = await screen.findByRole("dialog");
    await user.click(within(sheet).getByRole("radio", { name: "Password" }));
    await user.type(await within(sheet).findByLabelText(/^Title$/), "Masked password");
    await user.type(within(sheet).getByLabelText("Secret"), "s3cr3t-pass-42");

    await user.click(within(sheet).getByRole("combobox", { name: "Category" }));
    await user.click(await screen.findByRole("option", { name: "Payments" }));
    await user.click(within(sheet).getByRole("button", { name: /^Save/ }));

    await waitFor(() => expect(createSpy).toHaveBeenCalledTimes(1));
    expect(createSpy.mock.calls[0][0]).toMatchObject({
      kind: "password",
      title: "Masked password",
      secret: "s3cr3t-pass-42",
    });
    await waitFor(() => expect(setCategorySpy).toHaveBeenCalledWith(expect.any(String), "Payments"));
  });

  it("clears an existing category back to the default on edit", async () => {
    const { ipc, router } = await openVault();
    const setCategorySpy = vi.spyOn(ipc, "itemSetCategory");
    const user = userEvent.setup();

    const list = screen.getByTestId("vault-list");
    await within(list).findByText("Stripe live");
    await user.click(within(list).getByText("Stripe live"));
    await waitFor(() => expect(router.state.location.pathname).toMatch(/^\/item\//));

    const editButton = await screen.findByRole("button", { name: "Edit secret" });
    await waitFor(() => expect(editButton).toBeEnabled());
    await user.click(editButton);
    const sheet = await screen.findByRole("dialog");

    const category = await within(sheet).findByRole("combobox", { name: "Category" });
    await waitFor(() => expect(category).toHaveTextContent("Payments"));
    await user.click(category);
    await user.click(await screen.findByRole("option", { name: "Uncategorized (default)" }));
    await user.click(within(sheet).getByRole("button", { name: "Save changes" }));

    await waitFor(() => expect(setCategorySpy).toHaveBeenCalledWith(expect.any(String), null));
  });

  it("applies a classify_preview suggestion (manual category)", async () => {
    const { ipc } = await openVault();
    const classifySpy = vi.spyOn(ipc, "classifyPreview");
    // Different variable name: the spy replaces the method on the same object,
    // so re-reading it later would return the spy itself.
    const setCategorySpy = vi.spyOn(ipc, "itemSetCategory");
    const user = userEvent.setup();

    await user.click(screen.getAllByRole("button").find((b) => b.textContent?.trim() === "New")!);
    const sheet = await screen.findByRole("dialog");
    await user.type(await within(sheet).findByLabelText(/^Title$/), "OpenAI key");
    await user.click(within(sheet).getByRole("button", { name: "Show secret" }));
    await user.type(within(sheet).getByLabelText("Secret"), "sk-proj-abc123");

    await waitFor(() => expect(classifySpy).toHaveBeenCalled(), { timeout: 3000 });
    const suggestion = await within(sheet).findByTestId("classification-suggestion", {}, { timeout: 3000 });
    expect(suggestion).toHaveTextContent("AI / LLM");

    const apply = within(suggestion).getByRole("button", { name: /Apply/ });
    await user.click(apply);

    // Applying tags the draft; the manual category is stored on save.
    expect(within(sheet).getByText("llm")).toBeInTheDocument();

    await user.click(within(sheet).getByRole("button", { name: /^Save/ }));
    // For a brand-new item the suggested category is applied after creation.
    // The IPC spy records `(id, category)` (the client maps to argument keys).
    await waitFor(() => expect(setCategorySpy).toHaveBeenCalledWith(expect.any(String), "AI / LLM"));
  });

  it("edits an existing item and keeps the secret when left blank", async () => {
    const { ipc, router } = await openVault();
    const updateSpy = vi.spyOn(ipc, "itemUpdate");
    const setCategorySpy = vi.spyOn(ipc, "itemSetCategory");
    const user = userEvent.setup();

    const list = screen.getByTestId("vault-list");
    await within(list).findByText("Stripe live");
    await user.click(within(list).getByText("Stripe live"));
    await waitFor(() => expect(router.state.location.pathname).toMatch(/^\/item\//));

    const editButton = await screen.findByRole("button", { name: "Edit secret" });
    await waitFor(() => expect(editButton).toBeEnabled());
    await user.click(editButton);
    const sheet = await screen.findByRole("dialog");
    const title = await within(sheet).findByLabelText(/^Title$/);
    await waitFor(() => expect(title).toHaveValue("Stripe live"));
    await user.clear(title);
    await user.type(title, "Stripe production");
    await user.click(within(sheet).getByRole("button", { name: "Save changes" }));

    await waitFor(() => expect(updateSpy).toHaveBeenCalledTimes(1));
    const input = updateSpy.mock.calls[0][0];
    expect(input).toMatchObject({ title: "Stripe production" });
    expect(input).not.toHaveProperty("secret");
    // Saving without touching the category must not overwrite it.
    expect(setCategorySpy).not.toHaveBeenCalled();
  });

  it("blocks closing with unsaved changes until confirmed", async () => {
    await openVault();
    const user = userEvent.setup();
    uiStore.openCreate(null);
    const sheet = await screen.findByRole("dialog");
    await user.type(await within(sheet).findByLabelText(/^Title$/), "Draft");
    await user.click(within(sheet).getByRole("button", { name: "Cancel" }));

    // Closing the sheet is guarded while the draft is dirty.
    const confirm = await screen.findByText("Discard unsaved changes?", {}, { timeout: 3000 });
    expect(confirm).toBeInTheDocument();
    expect(uiStore.getState().editor).not.toBeNull();
    await user.click(screen.getByRole("button", { name: "Discard changes" }));
    await waitFor(() => expect(uiStore.getState().editor).toBeNull());
  });
});
