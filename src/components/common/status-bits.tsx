import { AlertTriangle, Loader2, ShieldCheck } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

export function InlineSpinner({ className }: { className?: string }) {
  return <Loader2 className={cn("size-4 animate-spin", className)} aria-hidden />;
}

export function ListSkeleton({ rows = 6 }: { rows?: number }) {
  return (
    <div className="space-y-0.5 px-2" aria-label="Loading secrets" role="status">
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="flex h-[76px] items-center gap-3 px-2">
          <Skeleton className="size-[34px] rounded-[9px]" />
          <div className="flex-1 space-y-2">
            <Skeleton className="h-3 w-2/5" />
            <Skeleton className="h-2.5 w-3/5" />
          </div>
        </div>
      ))}
    </div>
  );
}

export function ErrorState({
  title = "Could not load",
  message,
  onRetry,
  className,
}: {
  title?: string;
  message: string;
  onRetry?: () => void;
  className?: string;
}) {
  return (
    <div role="alert" className={cn("flex flex-col items-center gap-2 px-6 py-10 text-center", className)}>
      <AlertTriangle className="size-5 text-destructive" aria-hidden />
      <p className="text-sm font-medium">{title}</p>
      <p className="max-w-xs text-xs text-muted-foreground">{message}</p>
      {onRetry ? (
        <Button variant="outline" size="sm" onClick={onRetry} className="mt-1">
          Retry
        </Button>
      ) : null}
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  description,
  action,
  className,
}: {
  icon?: React.ReactNode;
  title: string;
  description: string;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col items-center gap-2 px-6 py-12 text-center", className)}>
      {icon ? (
        <div className="mb-1 flex size-11 items-center justify-center rounded-xl border border-border bg-card text-muted-foreground">
          {icon}
        </div>
      ) : null}
      <p className="text-sm font-semibold">{title}</p>
      <p className="max-w-xs text-xs leading-relaxed text-muted-foreground">{description}</p>
      {action ? <div className="mt-2">{action}</div> : null}
    </div>
  );
}

export function EncryptedBadge({ label = "Encrypted at rest" }: { label?: string }) {
  return (
    <span className="inline-flex h-[21px] items-center gap-1 rounded-[5px] border border-[#1F3A2D] bg-[#122619] px-1.5 text-[10px] font-semibold text-[#7FD3A4]">
      <ShieldCheck className="size-3" aria-hidden />
      {label}
    </span>
  );
}
