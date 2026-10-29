import { createFileRoute } from "@tanstack/react-router";
import { SecuritySettingsPage } from "@/pages/security-settings-page";

export const Route = createFileRoute("/settings/security")({
  component: SecuritySettingsPage,
});
