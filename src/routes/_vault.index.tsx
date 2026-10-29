import { createFileRoute } from "@tanstack/react-router";
import { VaultDetailEmpty } from "@/components/vault/vault-detail";

export const Route = createFileRoute("/_vault/")({
  component: VaultDetailEmpty,
});
