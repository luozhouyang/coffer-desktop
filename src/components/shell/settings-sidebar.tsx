import { Link, useMatchRoute, useSearch } from "@tanstack/react-router";
import { Archive, ArrowUpCircle, Lock, ShieldCheck } from "lucide-react";
import { useVault } from "@/app/vault-context";
import { Brand } from "@/components/common/brand";
import { SIDEBAR_STYLE } from "@/components/shell/app-sidebar";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarSeparator,
  useSidebar,
} from "@/components/ui/sidebar";

const SECTIONS: {
  label: string;
  icon: typeof ShieldCheck;
  to: "/settings/security" | "/settings/backup" | "/settings/updates" | null;
}[] = [
  { label: "Security", icon: ShieldCheck, to: "/settings/security" },
  { label: "Backup", icon: Archive, to: "/settings/backup" },
  { label: "Updates", icon: ArrowUpCircle, to: "/settings/updates" },
];

export function SettingsSidebar() {
  const search = useSearch({ strict: false }) as Record<string, unknown>;
  const { setOpenMobile, isMobile } = useSidebar();
  const matchRoute = useMatchRoute();
  const { lock } = useVault();
  const backSearch: Record<string, string | undefined> = {};
  if (typeof search.project === "string") backSearch.project = search.project;
  if (typeof search.kind === "string") backSearch.kind = search.kind;

  return (
    <Sidebar collapsible="icon" variant="sidebar" style={SIDEBAR_STYLE} aria-label="Settings navigation">
      <SidebarHeader className="px-2 pt-3 pb-2">
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton asChild size="lg" tooltip="Back to vault">
              <Link to="/" search={backSearch} onClick={() => isMobile && setOpenMobile(false)}>
                <Brand />
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel className="text-[10px] uppercase tracking-[0.12em] text-faint">Settings</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu className="gap-0.5">
              {SECTIONS.map(({ label, icon: Icon, to }) => (
                <SidebarMenuItem key={label}>
                  {to ? (
                    <SidebarMenuButton asChild isActive={matchRoute({ to, fuzzy: false }) !== false} tooltip={label}>
                      <Link to={to} search={backSearch} onClick={() => isMobile && setOpenMobile(false)}>
                        <Icon className="size-4" strokeWidth={1.8} aria-hidden />
                        <span>{label}</span>
                      </Link>
                    </SidebarMenuButton>
                  ) : (
                    <SidebarMenuButton tooltip={`${label} · coming soon`} disabled>
                      <Icon className="size-4" strokeWidth={1.8} aria-hidden />
                      <span>{label}</span>
                    </SidebarMenuButton>
                  )}
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter className="p-2">
        <SidebarSeparator className="mb-1" />
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton tooltip="Lock vault" onClick={() => void lock()}>
              <Lock className="size-4" strokeWidth={1.8} aria-hidden />
              <span>Lock vault</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
    </Sidebar>
  );
}
