import { useEffect, useState } from "react";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { FolderPlus, Loader2, Trash2 } from "lucide-react";
import { useCreateProject, useDeleteProject, useProjects, useUpdateProject } from "@/app/queries";
import { uiStore, useUiStore } from "@/app/ui-store";
import { errorMessage } from "@/lib/ipc-errors";
import { parseVaultSearch, toVaultLinkSearch } from "@/lib/vault-filter";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Form, FormControl, FormField, FormItem, FormLabel, FormMessage } from "@/components/ui/form";
import { Input } from "@/components/ui/input";
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

const projectSchema = z.object({
  name: z.string().trim().min(1, "Name is required.").max(100, "At most 100 characters."),
  description: z.string().max(500).optional(),
});

type ProjectValues = z.infer<typeof projectSchema>;

const COLORS = ["#E2A75A", "#8FB4FF", "#7FD3A4", "#C3A6FF", "#E88BA0", "#74CFD6"];

/** Hosted in `__root`; opened via `uiStore.openProjectDialog`. */
export function ProjectDialogs() {
  const request = useUiStore((s) => s.projectDialog);
  const { data: projects } = useProjects();
  const project = request?.projectId ? projects?.find((p) => p.id === request.projectId) : undefined;

  if (!request) return null;
  if (request.mode === "delete" && project) {
    return <DeleteProjectDialog key={project.id} project={project} />;
  }
  if ((request.mode === "create" || request.mode === "rename") && (request.mode === "create" || project)) {
    return <ProjectFormDialog key={project?.id ?? "new"} project={project} mode={request.mode} />;
  }
  return null;
}

function ProjectFormDialog({
  project,
  mode,
}: {
  project: { id: string; name: string; description: string | null; color: string | null } | undefined;
  mode: "create" | "rename";
}) {
  const create = useCreateProject();
  const update = useUpdateProject();
  const [color, setColor] = useState(project?.color ?? COLORS[0]);
  const [formError, setFormError] = useState<string | null>(null);
  const form = useForm<ProjectValues>({
    resolver: zodResolver(projectSchema),
    defaultValues: { name: project?.name ?? "", description: project?.description ?? "" },
  });
  const pending = create.isPending || update.isPending;

  const onSubmit = form.handleSubmit(async ({ name, description }) => {
    setFormError(null);
    try {
      if (mode === "create") {
        await create.mutateAsync({ name, description: description || null, color });
        toast.success(`Project “${name}” created`);
      } else if (project) {
        await update.mutateAsync({ id: project.id, name, description: description || null, color });
        toast.success("Project updated");
      }
      uiStore.closeProjectDialog();
    } catch (err) {
      setFormError(errorMessage(err));
    }
  });

  return (
    <Dialog open onOpenChange={(open) => !open && uiStore.closeProjectDialog()}>
      <DialogContent className="sm:max-w-[430px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <FolderPlus className="size-4 text-[#E2A75A]" aria-hidden />
            {mode === "create" ? "New project" : "Rename project"}
          </DialogTitle>
          <DialogDescription>
            Projects group related secrets. Items can belong to at most one project.
          </DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form onSubmit={onSubmit} className="space-y-4" noValidate>
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Name</FormLabel>
                  <FormControl>
                    <Input {...field} autoFocus maxLength={100} placeholder="e.g. Billing service" className="h-10 bg-input-field" />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="description"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Description <span className="text-faint">(optional)</span></FormLabel>
                  <FormControl>
                    <Input {...field} maxLength={500} placeholder="What lives in this project?" className="h-10 bg-input-field" />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <div>
              <p className="mb-2 text-[13px] font-medium">Color</p>
              <div className="flex gap-2" role="radiogroup" aria-label="Project color">
                {COLORS.map((c) => (
                  <button
                    key={c}
                    type="button"
                    role="radio"
                    aria-checked={color === c}
                    aria-label={`Color ${c}`}
                    onClick={() => setColor(c)}
                    className={cn(
                      "size-6 rounded-full ring-offset-2 ring-offset-popover focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
                      color === c && "ring-2 ring-white/70",
                    )}
                    style={{ backgroundColor: c }}
                  />
                ))}
              </div>
            </div>
            {formError ? (
              <p role="alert" className="text-[12px] text-destructive">
                {formError}
              </p>
            ) : null}
            <DialogFooter>
              <Button type="button" variant="ghost" onClick={() => uiStore.closeProjectDialog()}>
                Cancel
              </Button>
              <Button type="submit" disabled={pending}>
                {pending ? <Loader2 className="size-4 animate-spin" aria-hidden /> : null}
                {mode === "create" ? "Create project" : "Save"}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}

function DeleteProjectDialog({
  project,
}: {
  project: { id: string; name: string; itemCount: number };
}) {
  const remove = useDeleteProject();
  const navigate = useNavigate();
  const search = useSearch({ strict: false }) as Record<string, unknown>;

  useEffect(() => {
    // Fallback so a dialog left open after navigation cannot reappear.
    return () => uiStore.closeProjectDialog();
  }, []);

  return (
    <AlertDialog open onOpenChange={(open) => !open && uiStore.closeProjectDialog()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle className="flex items-center gap-2">
            <Trash2 className="size-4 text-destructive" aria-hidden /> Delete “{project.name}”?
          </AlertDialogTitle>
          <AlertDialogDescription>
            {project.itemCount > 0
              ? `Its ${project.itemCount} secret${project.itemCount === 1 ? "" : "s"} won’t be deleted — they move to Unassigned.`
              : "This project has no secrets yet."}{" "}
            This cannot be undone.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            className="bg-destructive text-white hover:bg-destructive/90"
            disabled={remove.isPending}
            onClick={(e) => {
              e.preventDefault();
              remove.mutate(project.id, {
                onSuccess: () => {
                  toast.success(`Project “${project.name}” deleted`);
                  uiStore.closeProjectDialog();
                  const filter = parseVaultSearch(search);
                  if (filter.project === project.id) {
                    void navigate({ to: "/", search: toVaultLinkSearch({ kind: filter.kind }) as never });
                  }
                },
                onError: (err) => toast.error("Could not delete project", { description: errorMessage(err) }),
              });
            }}
          >
            {remove.isPending ? <Loader2 className="size-4 animate-spin" aria-hidden /> : null}
            Delete project
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
