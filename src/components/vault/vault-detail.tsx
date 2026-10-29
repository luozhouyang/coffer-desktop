import { useState } from "react";
import { useNavigate, useSearch } from "@tanstack/react-router";
import {
  Copy,
  Eye,
  EyeOff,
  KeyRound,
  Pencil,
  Plus,
  Trash2,
  ExternalLink,
  RefreshCw,
} from "lucide-react";
import { useAudit, useDeleteItem, useItem, useProjects, useSettings } from "@/app/queries";
import { uiStore } from "@/app/ui-store";
import { useCopySecret, useRevealSecret } from "@/hooks/use-reveal-secret";
import { errorCode, errorMessage } from "@/lib/ipc-errors";
import type { ItemDetail } from "@/lib/ipc-types";
import { absoluteDate, longRelativeTime, hostFromUrl } from "@/lib/format";
import { parseVaultSearch, toVaultLinkSearch } from "@/lib/vault-filter";
import { CategoryBadge } from "@/components/common/category-badge";
import { ProjectChip } from "@/components/common/project-chip";
import { KindIcon } from "@/components/common/kind-icon";
import { EncryptedBadge, ErrorState, EmptyState } from "@/components/common/status-bits";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { cn } from "@/lib/utils";

export function VaultDetailEmpty() {
  const { openCreate } = uiStore;
  return (
    <div className="flex h-full items-center justify-center p-8">
      <EmptyState
        icon={<KeyRound className="size-5" aria-hidden />}
        title="Select a secret"
        description="Pick a secret from the list to view its details. Press ⌘K to search across the vault."
        action={
          <Button size="sm" variant="outline" onClick={() => openCreate(null)}>
            <Plus className="size-4" aria-hidden /> New secret
          </Button>
        }
      />
    </div>
  );
}

export function VaultDetail({ itemId }: { itemId: string }) {
  const navigate = useNavigate();
  const search = useSearch({ strict: false }) as Record<string, unknown>;
  const { data: item, isPending, isError, error, refetch } = useItem(itemId);
  const { data: projects } = useProjects();
  const { data: settings } = useSettings();
  const audit = useAudit(itemId);
  const remove = useDeleteItem();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const { openEdit } = uiStore;

  if (isPending) return <DetailSkeleton />;
  if (isError) {
    if (errorCode(error) === "notFound") {
      return (
        <div className="flex h-full items-center justify-center p-8">
          <EmptyState
            icon={<KeyRound className="size-5" aria-hidden />}
            title="Secret not found"
            description="It may have been deleted. Choose another secret from the list."
            action={
              <Button
                size="sm"
                variant="outline"
                onClick={() => void navigate({ to: "/", search: toVaultLinkSearch(parseVaultSearch(search)) as never })}
              >
                Back to list
              </Button>
            }
          />
        </div>
      );
    }
    return (
      <ErrorState
        className="h-full justify-center"
        title="Could not load secret"
        message={errorMessage(error)}
        onRetry={() => void refetch()}
      />
    );
  }
  if (!item) return null;

  const project = projects?.find((p) => p.id === item.projectId) ?? null;
  const clearSecs = settings?.clipboardClearSecs ?? 30;

  return (
    <div className="mx-auto w-full max-w-[680px] px-8 py-7">
      <header className="flex items-start gap-4">
        <KindIcon kind={item.kind} />
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-[24px] font-semibold leading-tight tracking-[-0.015em]" data-testid="detail-title">
            {item.title}
          </h2>
          <div className="mt-2 flex flex-wrap items-center gap-2">
            <CategoryBadge category={item.category} />
            <SourceLabel item={item} />
            {project ? <ProjectChip name={project.name} color={project.color} id={project.id} /> : null}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <IconButton label="Edit secret" onClick={() => openEdit(item.id)}>
            <Pencil className="size-4" aria-hidden />
          </IconButton>
          <IconButton label="Delete secret" onClick={() => setConfirmDelete(true)}>
            <Trash2 className="size-4" aria-hidden />
          </IconButton>
        </div>
      </header>

      <SecretCard item={item} clearSecs={clearSecs} />

      <section className="mt-6" aria-labelledby="metadata-heading">
        <h3 id="metadata-heading" className="mb-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-faint">
          Details
        </h3>
        <div className="grid grid-cols-[120px_1fr] gap-x-4 gap-y-2.5 rounded-[12px] border border-border bg-card px-4 py-3.5 text-[13px]">
          <MetaRow label="Username" value={item.username} mono />
          <MetaRow label="Project" value={project?.name ?? "No project"} />
          <MetaRow label="Website" value={item.url} link />
          <MetaRow label="Created" value={absoluteDate(item.createdAt)} title={longRelativeTime(item.createdAt)} />
          <MetaRow label="Updated" value={absoluteDate(item.updatedAt)} title={longRelativeTime(item.updatedAt)} />
          <MetaRow label="Type" value={kindLabel(item.kind)} />
        </div>
      </section>

      {item.tags.length > 0 ? (
        <section className="mt-5" aria-labelledby="tags-heading">
          <h3 id="tags-heading" className="mb-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-faint">
            Tags
          </h3>
          <div className="flex flex-wrap gap-1.5">
            {item.tags.map((tag) => (
              <Badge key={tag} variant="secondary" className="rounded-[6px] font-normal text-muted-foreground">
                {tag}
              </Badge>
            ))}
          </div>
        </section>
      ) : null}

      {item.notes ? (
        <section className="mt-5" aria-labelledby="notes-heading">
          <h3 id="notes-heading" className="mb-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-faint">
            Notes
          </h3>
          <div className="rounded-[12px] border border-border bg-card px-4 py-3 text-[13px] leading-relaxed whitespace-pre-wrap text-muted-foreground">
            {item.notes}
          </div>
        </section>
      ) : null}

      {audit.data && audit.data.length > 0 ? (
        <section className="mt-5" aria-labelledby="activity-heading">
          <h3 id="activity-heading" className="mb-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-faint">
            Activity
          </h3>
          <ol className="space-y-1.5">
            {audit.data.slice(0, 6).map((entry) => (
              <li key={entry.id} className="flex items-center gap-2 text-[12px] text-muted-foreground">
                <span className="size-1.5 rounded-full bg-faint" aria-hidden />
                <span className="capitalize">{auditLabel(entry.action)}</span>
                <span className="text-faint">·</span>
                <span className="text-faint">{longRelativeTime(entry.createdAt)}</span>
              </li>
            ))}
          </ol>
        </section>
      ) : null}

      <Separator className="my-6" />
      <footer className="flex items-center gap-2 pb-4 text-[11px] text-faint">
        <span className="size-1.5 rounded-full bg-[#7FD3A4]" aria-hidden />
        Local vault · encrypted at rest · clipboard clears in {clearSecs}s
      </footer>

      <AlertDialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete “{item.title}”?</AlertDialogTitle>
            <AlertDialogDescription>
              This removes the secret from your vault. There is no undo and no recovery.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-white hover:bg-destructive/90"
              onClick={() => {
                remove.mutate(item.id, {
                  onSuccess: () => {
                    void navigate({ to: "/", search: toVaultLinkSearch(parseVaultSearch(search)) as never });
                  },
                });
              }}
              disabled={remove.isPending}
            >
              Delete secret
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function IconButton({ label, onClick, children }: { label: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label={label} onClick={onClick} className="text-muted-foreground">
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

function SourceLabel({ item }: { item: ItemDetail }) {
  if (!item.category || item.categorySource === "default") {
    return <span className="text-[11px] text-faint">Default category</span>;
  }
  const text =
    item.categorySource === "manual"
      ? "Set manually"
      : item.categorySource === "import"
        ? "From import"
        : "Rule match";
  return (
    <span className="text-[11px] text-faint" data-testid="category-source">
      {text}
    </span>
  );
}

function SecretCard({ item, clearSecs }: { item: ItemDetail; clearSecs: number }) {
  const reveal = useRevealSecret(item.id);
  const copier = useCopySecret();
  const isEnv = item.kind === "env";
  const rows = !isEnv || reveal.value === null ? null : parseEnvRows(reveal.value);

  return (
    <section className="mt-6" aria-labelledby="secret-heading">
      <h3 id="secret-heading" className="sr-only">
        Secret
      </h3>
      <div className="rounded-[14px] border border-border bg-card p-4">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-faint">Secret</span>
            <EncryptedBadge />
          </div>
          <div className="flex items-center gap-1.5">
            <Button
              size="sm"
              variant="ghost"
              className="h-7 gap-1.5 px-2 text-[12px] text-muted-foreground"
              onClick={() => (reveal.revealed ? reveal.hide() : void reveal.reveal())}
              disabled={reveal.pending || !item.hasSecret}
              aria-label={reveal.revealed ? "Hide secret" : "Reveal secret"}
            >
              {reveal.revealed ? <EyeOff className="size-3.5" aria-hidden /> : <Eye className="size-3.5" aria-hidden />}
              {reveal.pending ? "Revealing…" : reveal.revealed ? "Hide" : "Reveal"}
            </Button>
            <Button
              size="sm"
              variant="secondary"
              className="h-7 gap-1.5 px-2 text-[12px]"
              onClick={() => void copier.copy(item.id, item.title)}
              aria-label="Copy secret"
              disabled={!item.hasSecret}
            >
              <Copy className="size-3.5" aria-hidden /> Copy
            </Button>
          </div>
        </div>

        <div className="mt-3 rounded-[9px] border border-[#2B3240] bg-[#0F131A] px-3 py-2.5">
          {isEnv && reveal.revealed && rows ? (
            <div className="max-h-64 space-y-1 overflow-y-auto" data-testid="revealed-secret">
              {rows.map((row, i) => (
                <div key={`${row.key}-${i}`} className="flex items-center gap-2 font-mono text-[12px]">
                  <span className="shrink-0 text-[#A7AFBD]">{row.key}</span>
                  <span className="text-faint">=</span>
                  <span className="truncate text-[#E9ECF2]">{row.value}</span>
                </div>
              ))}
            </div>
          ) : item.hasSecret ? (
            <p
              className={cn(
                "truncate-secret font-mono text-[13px]",
                reveal.revealed ? "text-[#E9ECF2]" : "text-[#6B7482]",
              )}
              data-testid="secret-value"
              data-revealed={reveal.revealed || undefined}
            >
              {reveal.revealed ? reveal.value : "••••••••••••••••••••"}
            </p>
          ) : (
            <p className="text-[12px] text-faint">No secret stored for this entry.</p>
          )}
        </div>
        <div className="mt-2 flex items-center justify-between gap-2 text-[11px] text-faint">
          <span>
            {reveal.revealed
              ? `Hides automatically in ${reveal.secondsLeft}s.`
              : `Copies in Rust · clipboard clears in ${clearSecs}s`}
          </span>
          {reveal.revealed ? (
            <button type="button" className="inline-flex items-center gap-1 hover:text-muted-foreground" onClick={reveal.hide}>
              <RefreshCw className="size-3" aria-hidden /> Hide now
            </button>
          ) : null}
        </div>
        {reveal.error ? (
          <p role="alert" className="mt-2 text-[11px] text-destructive">
            {reveal.error}
          </p>
        ) : null}
      </div>
    </section>
  );
}

function parseEnvRows(raw: string): { key: string; value: string }[] {
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (Array.isArray(parsed)) {
      return parsed
        .filter((r): r is { key: string; value: string } => typeof r?.key === "string")
        .map((r) => ({ key: r.key, value: String(r.value ?? "") }));
    }
  } catch {
    // fall through: raw text
  }
  return raw.split(/\r?\n/).map((line) => {
    const idx = line.indexOf("=");
    return idx === -1 ? { key: line, value: "" } : { key: line.slice(0, idx), value: line.slice(idx + 1) };
  });
}

function MetaRow({
  label,
  value,
  mono,
  link,
  title,
}: {
  label: string;
  value: string | null | undefined;
  mono?: boolean;
  link?: boolean;
  title?: string;
}) {
  const host = link ? hostFromUrl(value) : null;
  return (
    <>
      <dt className="text-faint">{label}</dt>
      <dd className={cn("min-w-0 truncate", mono && "font-mono text-[12px]", !value && "text-faint")} title={title}>
        {value ? (
          link && /^https?:\/\//i.test(value) ? (
            <a
              href={value}
              target="_blank"
              rel="noreferrer noopener"
              className="inline-flex items-center gap-1 text-[#8FB4FF] hover:underline"
            >
              {host ?? value}
              <ExternalLink className="size-3" aria-hidden />
            </a>
          ) : (
            value
          )
        ) : (
          "—"
        )}
      </dd>
    </>
  );
}

function kindLabel(kind: ItemDetail["kind"]) {
  return { apiKey: "API key", env: "Environment file", password: "Password", note: "Note" }[kind];
}

function auditLabel(action: string) {
  return (
    { create: "Created", update: "Updated", delete: "Deleted", reveal: "Revealed", copy: "Copied", projectChange: "Moved project" }[
      action
    ] ?? action
  );
}

function DetailSkeleton() {
  return (
    <div className="mx-auto w-full max-w-[680px] space-y-6 px-8 py-7" aria-label="Loading secret" role="status">
      <div className="flex items-center gap-4">
        <Skeleton className="size-[34px] rounded-[9px]" />
        <div className="space-y-2">
          <Skeleton className="h-6 w-64" />
          <Skeleton className="h-3 w-40" />
        </div>
      </div>
      <Skeleton className="h-28 w-full rounded-[14px]" />
      <Skeleton className="h-40 w-full rounded-[12px]" />
    </div>
  );
}
