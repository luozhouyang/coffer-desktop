import { createFileRoute } from "@tanstack/react-router";
import { UpdatesSettingsPage } from "@/pages/updates-settings-page";

export const Route = createFileRoute("/settings/updates")({
  component: UpdatesSettingsPage,
});
