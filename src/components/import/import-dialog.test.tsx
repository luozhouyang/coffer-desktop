import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { uiStore } from "@/app/ui-store";
import { createMockIpc } from "@/lib/mock-ipc";
import { renderApp, waitForPath } from "@/test/render-app";

async function openImport() {
  const ipc = createMockIpc();
  const rendered = renderApp({ ipc, initialHref: "/" });
  await waitForPath(rendered.router, "/");
  await screen.findByTestId("vault-list");
  const user = userEvent.setup();
  const trigger = screen.getAllByRole("button").find((button) => button.textContent?.trim() === "Import");
  expect(trigger).toBeTruthy();
  await user.click(trigger!);
  const dialog = await screen.findByRole("dialog");
  return { ...rendered, ipc, user, dialog };
}

function upload(dialog: HTMLElement, contents: string, name = "myapp.env") {
  const input = within(dialog).getByTestId("import-file-input");
  const file = new File([contents], name, { type: "text/plain" });
  fireEvent.change(input, { target: { files: [file] } });
}

describe("import .env dialog", () => {
  it("parses a file, masks values, and imports the selected variables", async () => {
    const { ipc, user, dialog } = await openImport();
    const commit = vi.spyOn(ipc, "importCommit");

    upload(dialog, "# local\nOPENAI_API_KEY=sk-test-secret\nNEW_KEY=hello\n");
    expect(await within(dialog).findByText("OPENAI_API_KEY")).toBeInTheDocument();
    expect(within(dialog).getByText("NEW_KEY")).toBeInTheDocument();
    // Plaintext never reaches the preview.
    expect(within(dialog).queryByText(/sk-test-secret/)).not.toBeInTheDocument();
    expect(within(dialog).getAllByText("New").length).toBe(2);

    await user.click(within(dialog).getByRole("button", { name: "Import 2 secrets" }));
    await waitFor(() => expect(commit).toHaveBeenCalledTimes(1));
    expect(commit.mock.calls[0][0]).toMatchObject({ projectId: null, selected: [0, 1] });

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    const list = screen.getByTestId("vault-list");
    expect(await within(list).findByText("OPENAI_API_KEY")).toBeInTheDocument();
    expect(await within(list).findByText("NEW_KEY")).toBeInTheDocument();
  });

  it("updates an existing item in the chosen project and preserves unselected rows", async () => {
    const { ipc, user, dialog, queryClient } = await openImport();
    const existing = await ipc.itemCreate({
      kind: "apiKey",
      title: "OPENAI_API_KEY",
      secret: "old-value",
      projectId: null,
      tags: [],
    });
    await queryClient.invalidateQueries();

    upload(dialog, "OPENAI_API_KEY=new-value\nEXTRA=skip-me\n");
    expect(await within(dialog).findByText("Update")).toBeInTheDocument();
    expect(within(dialog).getAllByText("New").length).toBe(1);

    // Drop the second row from the import.
    await user.click(within(dialog).getByRole("checkbox", { name: "Import EXTRA" }));
    await user.click(within(dialog).getByRole("button", { name: "Import 1 secret" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    const revealed = await ipc.itemRevealSecret(existing.id);
    expect(revealed.value).toBe("new-value");
    const list = screen.getByTestId("vault-list");
    await waitFor(() => expect(within(list).queryByText("EXTRA")).not.toBeInTheDocument());
  });

  it("cancels the backend session when closed before importing", async () => {
    const { ipc, user, dialog } = await openImport();
    const cancel = vi.spyOn(ipc, "importCancel");

    upload(dialog, "ONLY_KEY=value\n");
    await within(dialog).findByText("ONLY_KEY");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(cancel).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("suggests a project from the file name", async () => {
    const { user, dialog } = await openImport();
    // Seed project `Coffer dashboard`; its hint is the lowercased stem.
    upload(dialog, "A=1\n", "coffer dashboard.env");
    await within(dialog).findByText("A");
    const trigger = within(dialog).getByRole("combobox", { name: "Import into project" });
    expect(trigger).toHaveTextContent("Coffer dashboard");
    expect(user).toBeTruthy();
  });

  it("keeps an archived default project selectable instead of silently importing unassigned", async () => {
    const ipc = createMockIpc();
    await ipc.projectUpdate({ id: "p-lab", archived: true });
    const rendered = renderApp({ ipc, initialHref: "/" });
    await waitForPath(rendered.router, "/");
    await screen.findByTestId("vault-list");
    act(() => uiStore.openImport("p-lab"));

    const dialog = await screen.findByRole("dialog");
    const commit = vi.spyOn(ipc, "importCommit");
    upload(dialog, "ARCHIVED_KEY=value\n");
    await within(dialog).findByText("ARCHIVED_KEY");

    const trigger = within(dialog).getByRole("combobox", { name: "Import into project" });
    expect(trigger).toHaveTextContent("Lab experiments (archived)");

    const user = userEvent.setup();
    await user.click(within(dialog).getByRole("button", { name: "Import 1 secret" }));
    await waitFor(() => expect(commit).toHaveBeenCalledTimes(1));
    expect(commit.mock.calls[0][0]).toMatchObject({ projectId: "p-lab" });
  });
});
