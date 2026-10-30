import { createFileRoute } from "@tanstack/react-router";
import { BackupSettingsPage } from "@/pages/backup-settings-page";

export const Route = createFileRoute("/settings/backup")({
  component: BackupSettingsPage,
});
