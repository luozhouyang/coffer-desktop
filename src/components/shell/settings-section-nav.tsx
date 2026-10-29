import { ShieldCheck } from "lucide-react";
import { cn } from "@/lib/utils";

/** Breadcrumb-ish header for the settings content area. Other sections are not built yet. */
export function SettingsSectionNav({ className }: { className?: string }) {
  return (
    <nav aria-label="Settings section" className={cn("flex items-center gap-2 text-[13px]", className)}>
      <span className="text-faint">Settings</span>
      <span className="text-faint" aria-hidden>/</span>
      <span className="inline-flex items-center gap-1.5 font-medium text-foreground">
        <ShieldCheck className="size-3.5 text-[#E2A75A]" aria-hidden />
        Security
      </span>
    </nav>
  );
}
