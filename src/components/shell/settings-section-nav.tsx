import { useLocation } from "@tanstack/react-router";
import { ArrowUpCircle, ShieldCheck } from "lucide-react";
import { cn } from "@/lib/utils";

const SECTIONS = [
  { path: "/settings/security", label: "Security", icon: ShieldCheck },
  { path: "/settings/updates", label: "Updates", icon: ArrowUpCircle },
] as const;

/** Breadcrumb-ish header for the settings content area, driven by the current section. */
export function SettingsSectionNav({ className }: { className?: string }) {
  const pathname = useLocation({ select: (location) => location.pathname });
  const section =
    SECTIONS.find((s) => pathname === s.path || pathname.startsWith(`${s.path}/`)) ?? SECTIONS[0];
  const Icon = section.icon;
  return (
    <nav aria-label="Settings section" className={cn("flex items-center gap-2 text-[13px]", className)}>
      <span className="text-faint">Settings</span>
      <span className="text-faint" aria-hidden>/</span>
      <span className="inline-flex items-center gap-1.5 font-medium text-foreground">
        <Icon className="size-3.5 text-[#E2A75A]" aria-hidden />
        {section.label}
      </span>
    </nav>
  );
}
