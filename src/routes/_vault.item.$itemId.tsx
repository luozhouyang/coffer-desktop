import { createFileRoute } from "@tanstack/react-router";
import { VaultDetail } from "@/components/vault/vault-detail";

export const Route = createFileRoute("/_vault/item/$itemId")({
  component: VaultItemRoute,
});

function VaultItemRoute() {
  const { itemId } = Route.useParams();
  return <VaultDetail itemId={itemId} />;
}
