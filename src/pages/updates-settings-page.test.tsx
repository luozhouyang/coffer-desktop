import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockIpc } from "@/lib/mock-ipc";
import { renderApp, waitForPath } from "@/test/render-app";

const getVersion = vi.fn<() => Promise<string>>();
const check = vi.fn<() => Promise<unknown>>();
const relaunch = vi.fn<() => Promise<void>>();

vi.mock("@tauri-apps/api/app", () => ({ getVersion: () => getVersion() }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: () => check() }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: () => relaunch() }));

/** Marks the jsdom window as a Tauri webview so `isTauriRuntime()` returns true. */
function enableTauriRuntime() {
  Reflect.set(window, "__TAURI_INTERNALS__", {});
}

let hold: { promise: Promise<void>; release: () => void } = { promise: Promise.resolve(), release: () => {} };
function holdDownload() {
  let release = () => {};
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  hold = { promise, release };
}

function createFakeUpdate(version: string, body = "Bug fixes and performance improvements.") {
  return {
    version,
    currentVersion: "0.1.0",
    body,
    downloadAndInstall: vi.fn(async (onEvent?: (event: unknown) => void) => {
      onEvent?.({ event: "Started", data: { contentLength: 100 } });
      onEvent?.({ event: "Progress", data: { chunkLength: 40 } });
      // Hold the download open so the progress UI can be observed mid-flight.
      await hold.promise;
      onEvent?.({ event: "Progress", data: { chunkLength: 60 } });
      onEvent?.({ event: "Finished" });
    }),
  };
}

async function openUpdates() {
  const ipc = createMockIpc();
  const rendered = renderApp({ ipc, initialHref: "/settings/updates" });
  await waitForPath(rendered.router, "/settings/updates");
  await screen.findByRole("heading", { level: 1, name: "Updates" });
  return { ...rendered, ipc };
}

describe("updates settings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    enableTauriRuntime();
    getVersion.mockResolvedValue("0.1.0");
    check.mockResolvedValue(null);
  });

  afterEach(() => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  });

  it("renders the current app version", async () => {
    await openUpdates();
    await waitFor(() => expect(screen.getByTestId("current-version")).toHaveTextContent("0.1.0"));
    expect(getVersion).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: /Check for updates/ })).toBeEnabled();
  });

  it("reports that the app is up to date", async () => {
    await openUpdates();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Check for updates/ }));
    expect(await screen.findByText("You are running the latest version.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Download and install/ })).not.toBeInTheDocument();
  });

  it("enables install when an update is found and reports download progress", async () => {
    holdDownload();
    const update = createFakeUpdate("0.2.0");
    check.mockResolvedValue(update);
    await openUpdates();
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Check for updates/ }));
    expect(await screen.findByTestId("available-version")).toHaveTextContent("0.2.0");
    expect(screen.getByText("Bug fixes and performance improvements.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Download and install/ }));
    await waitFor(() => expect(update.downloadAndInstall).toHaveBeenCalledTimes(1));
    const progress = await screen.findByTestId("download-progress");
    expect(progress).toHaveTextContent("Downloading… 40%");
    expect(progress).toHaveTextContent("(40 B of 100 B)");
    expect(screen.queryByRole("button", { name: /Restart to finish update/ })).not.toBeInTheDocument();

    hold.release();
    expect(await screen.findByRole("button", { name: /Restart to finish update/ })).toBeInTheDocument();
    expect(screen.queryByTestId("download-progress")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Restart to finish update/ }));
    await waitFor(() => expect(relaunch).toHaveBeenCalledTimes(1));
  });

  it("shows an error hint when the check fails", async () => {
    check.mockRejectedValue(new Error("Network unavailable"));
    await openUpdates();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Check for updates/ }));
    expect(await screen.findByText(/Update check failed: Network unavailable/)).toBeInTheDocument();
  });

  it("disables update actions outside the Tauri runtime", async () => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    await openUpdates();
    expect(screen.getByText("Not available in browser preview.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Check for updates/ })).toBeDisabled();
    expect(getVersion).not.toHaveBeenCalled();
  });
});
