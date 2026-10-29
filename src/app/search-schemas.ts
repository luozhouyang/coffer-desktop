import { z } from "zod";
import { ITEM_KINDS } from "@/lib/ipc-types";

const kindSchema = z.enum(ITEM_KINDS as unknown as [string, ...string[]]);

/**
 * Vault list filters live in typed search params so they survive navigation
 * into an item and back. Each key is optional; route components drop keys
 * whose value is "all"/undefined.
 *
 *  - `project`: project id, or "unassigned" for items without a project
 *  - `kind`: apiKey | env | password | note
 *  - `q`: free-text filter over titles (client-side; the palette uses `search`)
 */
export const vaultSearchSchema = z.object({
  project: z.string().min(1).optional().catch(undefined),
  kind: kindSchema.optional().catch(undefined),
  q: z.string().max(200).optional().catch(undefined),
});

export type VaultSearch = z.infer<typeof vaultSearchSchema>;

/** Only `project` and `kind` are carried into settings for a back link. */
export const settingsSearchSchema = z.object({
  project: z.string().optional().catch(undefined),
  kind: z.string().optional().catch(undefined),
});

export const unlockSearchSchema = z.object({
  redirect: z.string().optional().catch(undefined),
});
