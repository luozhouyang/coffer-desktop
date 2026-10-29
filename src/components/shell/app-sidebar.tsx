import { Link, useSearch } from "@tanstack/react-router";
import {
  ChevronRight,
  Inbox,
  Lock,
  Moon,
  Pencil,
  Plus,
  Search,
  Settings,
  Trash2,
  KeyRound,
} from "lucide-react";
import { useSidebarCounts, useLockVault, useProjects, useSettings } from "@/app/queries";
import { useVault } from "@/app/vault-context";
import { uiStore } from "@/app/ui-store";
import type { Project } from "@/lib/ipc-types";
import { projectColor } from "@/lib/categories";
import { formatDuration } from "@/lib/format";
import { cn } from "@/lib/utils";
import { parseVaultSearch, toVaultLinkSearch } from "@/lib/vault-filter";
import { isMacPlatform } from "@/lib/platform";
import { Brand, BrandMark } from "@/components/common/brand";
import { Kbd } from "@/components/common/kbd";
import { ProjectDot } from "@/components/common/project-chip";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarSeparator,
  useSidebar,
} from "@/components/ui/sidebar";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

const SIDEBAR_STYLE = {
  "--sidebar-width": "15.5rem",
  "--sidebar-width-icon": "3rem",
} as React.CSSProperties;

export { SIDEBAR_STYLE };

export function AppSidebar() {
  // `useSearch` is typed per route; read it loosely so the sidebar works on any
  // `_vault` route (index or item detail).
  const search = useSearch({ strict: false }) as Record<string, unknown>;
  const filter = parseVaultSearch(search);
  const { data: counts, isPending: countsPending } = useSidebarCounts();
  const { data: projects, isPending: projectsPending } = useProjects();
  const { openPalette, openProjectDialog } = uiStore;
  const { setOpenMobile, isMobile } = useSidebar();
  const { warning } = useVault();
  const settingsQuery = useSettings();
  const lockMutation = useLockVault();

  const closeMobile = () => {
    if (isMobile) setOpenMobile(false);
  };

  const isAll = !filter.project;
  const isUnassigned = filter.project === "unassigned";

  const onLock = () => {
    closeMobile();
    lockMutation.mutate();
  };

  return (
    <Sidebar collapsible="icon" variant="sidebar" aria-label="Vault navigation" data-testid="app-sidebar">
      <SidebarHeader className="gap-2 px-2 pt-3 pb-2">
        <div className="flex h-8 items-center gap-1 pl-1 group-data-[collapsible=icon]:pl-0">
          <Brand className="min-w-0 group-data-[collapsible=icon]:hidden" />
          <div className="hidden w-full justify-center group-data-[collapsible=icon]:flex">
            <BrandMark />
          </div>
        </div>
        <button
          type="button"
          onClick={() => {
            closeMobile();
            openPalette();
          }}
          aria-label="Search secrets"
          className="group/search flex h-8 w-full items-center gap-2 rounded-[8px] border border-[#1d222b] bg-[#0F131A] px-2 text-[12px] text-faint transition-colors hover:border-[#2B3240] hover:text-muted-foreground group-data-[collapsible=icon]:size-8 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:border-transparent group-data-[collapsible=icon]:bg-transparent group-data-[collapsible=icon]:px-0 [&>span]:group-data-[collapsible=icon]:hidden"
        >
          <Search className="size-3.5 shrink-0" aria-hidden />
          <span>Search secrets</span>
          <span className="ml-auto flex items-center gap-0.5">
            <Kbd className="h-[18px] min-w-0 bg-[#1a1f28] px-1 text-[9px]">{isMacPlatform() ? "⌘" : "Ctrl"}</Kbd>
            <Kbd className="h-[18px] min-w-0 bg-[#1a1f28] px-1 text-[9px]">K</Kbd>
          </span>
        </button>
      </SidebarHeader>

      <SidebarContent className="gap-0 px-0">
        <SidebarGroup className="py-2">
          <SidebarGroupContent>
            <SidebarMenu className="gap-0.5">
              <NavRow
                to="/"
                search={toVaultLinkSearch({ kind: filter.kind })}
                active={isAll}
                icon={<KeyRound className="size-4" strokeWidth={1.8} aria-hidden />}
                label="All secrets"
                badge={counts?.all}
                pending={countsPending}
                onNavigate={closeMobile}
              />
              <NavRow
                to="/"
                search={toVaultLinkSearch({ project: "unassigned", kind: filter.kind })}
                active={isUnassigned}
                icon={<Inbox className="size-4" strokeWidth={1.8} aria-hidden />}
                label="Unassigned"
                badge={counts?.unassigned}
                pending={countsPending}
                onNavigate={closeMobile}
              />
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>

        <SidebarGroup className="py-1">
          <SidebarGroupLabel className="h-7 text-[10px] font-semibold uppercase tracking-[0.12em] text-faint">
            Projects
          </SidebarGroupLabel>
          <SidebarGroupAction
            title="New project"
            aria-label="New project"
            onClick={() => openProjectDialog({ mode: "create" })}
            className="top-1 right-2 size-6 text-faint hover:text-foreground"
          >
            <Plus className="size-3.5" aria-hidden />
          </SidebarGroupAction>
          <SidebarGroupContent>
            <SidebarMenu className="gap-0.5">
              {projectsPending ? <ProjectRowSkeletons /> : null}
              {!projectsPending && projects?.length === 0 ? (
                <SidebarMenuItem>
                  <p className="px-2 py-2 text-[11px] leading-relaxed text-faint group-data-[collapsible=icon]:hidden">
                    Projects group related secrets. Create one to get started.
                  </p>
                </SidebarMenuItem>
              ) : null}
              {projects?.map((project) => (
                <ProjectRow
                  key={project.id}
                  project={project}
                  count={counts?.projects.find((c) => c.projectId === project.id)?.count ?? project.itemCount}
                  active={filter.project === project.id}
                  onNavigate={closeMobile}
                />
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>

      <SidebarFooter className="gap-1 p-2">
        <AutoLockStatus autoLockSecs={settingsQuery.data?.autoLockSecs} warning={warning?.seconds ?? null} />
        <SidebarSeparator className="my-1 group-data-[collapsible=icon]:hidden" />
        <SidebarMenu className="gap-0.5">
          <SidebarMenuItem>
            <SidebarMenuButton asChild tooltip="Settings" onClick={closeMobile}>
              <Link
                to="/settings/security"
                search={{ project: filter.project, kind: filter.kind }}
              >
                <Settings className="size-4" strokeWidth={1.8} aria-hidden />
                <span>Settings</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
          <SidebarMenuItem>
            <SidebarMenuButton
              tooltip="Lock vault"
              onClick={onLock}
              disabled={lockMutation.isPending}
              className="text-muted-foreground hover:text-foreground"
            >
              <Lock className="size-4" strokeWidth={1.8} aria-hidden />
              <span>Lock vault</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
        <p className="px-2 pt-1 text-[10px] text-faint group-data-[collapsible=icon]:hidden">
          <span className="text-[#7FD3A4]">●</span> Local vault · v0.1
        </p>
      </SidebarFooter>
    </Sidebar>
  );
}

function NavRow({
  to,
  search,
  active,
  icon,
  label,
  badge,
  pending,
  onNavigate,
}: {
  to: "/";
  search: Record<string, string | undefined>;
  active: boolean;
  icon: React.ReactNode;
  label: string;
  badge: number | undefined;
  pending: boolean;
  onNavigate: () => void;
}) {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={active} tooltip={label} onClick={onNavigate}>
        <Link to={to} search={search as never} aria-current={active ? "page" : undefined}>
          {icon}
          <span>{label}</span>
        </Link>
      </SidebarMenuButton>
      {pending && badge === undefined ? (
        <Skeleton className="absolute top-1/2 right-2 h-3 w-5 -translate-y-1/2 group-data-[collapsible=icon]:hidden" />
      ) : (
        <SidebarMenuBadge>{badge ?? 0}</SidebarMenuBadge>
      )}
    </SidebarMenuItem>
  );
}

function ProjectRow({
  project,
  count,
  active,
  onNavigate,
}: {
  project: Project;
  count: number;
  active: boolean;
  onNavigate: () => void;
}) {
  const { openProjectDialog } = uiStore;
  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={active} tooltip={project.name} onClick={onNavigate}>
        <Link
          to="/"
          search={{ project: project.id } as never}
          aria-current={active ? "page" : undefined}
          className="group/project"
        >
          <ProjectDot color={projectColor(project.color, project.id)} className="size-[7px]" />
          <span className="truncate">{project.name}</span>
        </Link>
      </SidebarMenuButton>
      <SidebarMenuBadge className="transition-opacity group-hover/project:hidden group-focus-within/project:hidden">
        {count}
      </SidebarMenuBadge>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-label={`Manage project ${project.name}`}
            className="absolute top-1/2 right-1 hidden size-6 -translate-y-1/2 items-center justify-center rounded-[6px] text-muted-foreground hover:bg-black/20 hover:text-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-none group-hover/project:flex group-focus-within/project:flex group-data-[collapsible=icon]:hidden"
          >
            <ChevronRight className="size-3.5 rotate-90" aria-hidden />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" side="right" className="w-44">
          <DropdownMenuItem onSelect={() => openProjectDialog({ mode: "rename", projectId: project.id })}>
            <Pencil className="size-4" aria-hidden /> Rename
          </DropdownMenuItem>
          <DropdownMenuItem
            variant="destructive"
            onSelect={() => openProjectDialog({ mode: "delete", projectId: project.id })}
          >
            <Trash2 className="size-4" aria-hidden /> Delete project
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </SidebarMenuItem>
  );
}

function ProjectRowSkeletons() {
  return (
    <>
      {[0, 1, 2].map((i) => (
        <SidebarMenuItem key={i}>
          <div className="flex h-8 items-center gap-2 px-2">
            <Skeleton className="size-2 rounded-full" />
            <Skeleton className="h-3 flex-1" />
          </div>
        </SidebarMenuItem>
      ))}
    </>
  );
}

function AutoLockStatus({ autoLockSecs, warning }: { autoLockSecs: number | undefined; warning: number | null }) {
  // Countdown is approximate: the backend owns the idle timer; we show the
  // configured window and surface the exact warning seconds when received.
  const label = warning !== null ? `Locking in ${warning}s` : `Auto-lock ${formatDuration(autoLockSecs ?? 300)}`;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div
          className="flex h-7 items-center gap-2 rounded-[6px] px-2 text-[11px] text-faint group-data-[collapsible=icon]:hidden"
          data-testid="auto-lock-status"
        >
          <Moon className="size-3.5" aria-hidden />
          <span className={cn(warning !== null && "text-[#E2A75A]")}>{label}</span>
        </div>
      </TooltipTrigger>
      <TooltipContent side="top" className="max-w-56 text-xs">
        The vault locks after {Math.round((autoLockSecs ?? 300) / 60)} minutes of inactivity. Keys are wiped from
        memory.
      </TooltipContent>
    </Tooltip>
  );
}

