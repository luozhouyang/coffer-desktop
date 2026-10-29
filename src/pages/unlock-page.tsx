import { useState } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { Link, useNavigate } from "@tanstack/react-router";
import { Eye, EyeOff, Fingerprint, KeyRound, Lock, ShieldCheck, WifiOff } from "lucide-react";
import { useUnlock } from "@/app/queries";
import { NoRecoveryNotice } from "@/components/common/no-recovery-notice";
import { BrandMark } from "@/components/common/brand";
import { Kbd } from "@/components/common/kbd";
import { InlineSpinner } from "@/components/common/status-bits";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Form, FormControl, FormField, FormItem, FormLabel, FormMessage } from "@/components/ui/form";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { errorMessage, isLockedError } from "@/lib/ipc-errors";

const unlockSchema = z.object({
  password: z.string().min(1, "Enter your master password."),
});

type UnlockValues = z.infer<typeof unlockSchema>;

export function UnlockPage() {
  const navigate = useNavigate();
  const unlock = useUnlock();
  const [showPassword, setShowPassword] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const form = useForm<UnlockValues>({
    resolver: zodResolver(unlockSchema),
    defaultValues: { password: "" },
  });

  const onSubmit = form.handleSubmit(async ({ password }) => {
    setFormError(null);
    try {
      await unlock.mutateAsync(password);
      void navigate({ to: "/", replace: true });
    } catch (err) {
      if (isLockedError(err)) return;
      setFormError(errorMessage(err));
      form.setValue("password", "");
      form.setFocus("password");
    }
  });

  return (
    <div className="grid h-svh grid-cols-1 bg-background lg:grid-cols-[minmax(0,1fr)_480px]">
      <aside className="relative hidden flex-col justify-between overflow-hidden border-r border-border bg-[#0B0E12] p-12 lg:flex">
        <div className="pointer-events-none absolute -top-32 -left-24 size-[420px] rounded-full bg-[#E2A75A]/6 blur-3xl" />
        <div className="relative flex items-center gap-3">
          <BrandMark className="size-9" />
          <span className="text-lg font-semibold tracking-[-0.01em]">Coffer</span>
        </div>
        <div className="relative max-w-md">
          <h2 className="text-[28px] font-semibold leading-tight tracking-[-0.02em]">
            A local vault for the keys that matter.
          </h2>
          <ul className="mt-8 space-y-4 text-[13px] text-muted-foreground">
            <li className="flex gap-3">
              <ShieldCheck className="mt-0.5 size-4 shrink-0 text-[#7FD3A4]" aria-hidden />
              Encrypted at rest with Argon2id + XChaCha20-Poly1305.
            </li>
            <li className="flex gap-3">
              <KeyRound className="mt-0.5 size-4 shrink-0 text-[#E2A75A]" aria-hidden />
              Secrets never cross to the interface — copying happens in the core.
            </li>
            <li className="flex gap-3">
              <WifiOff className="mt-0.5 size-4 shrink-0 text-[#8FB4FF]" aria-hidden />
              No account, no cloud, no sync. Offline by design.
            </li>
          </ul>
        </div>
        <p className="relative flex items-center gap-2 text-[11px] text-faint">
          <span className="size-1.5 rounded-full bg-[#7FD3A4]" aria-hidden /> Local vault · v0.1
        </p>
      </aside>

      <main className="flex items-center justify-center px-6 py-10">
        <div className="w-full max-w-[360px]">
          <div className="mb-8 flex items-center gap-3 lg:hidden">
            <BrandMark className="size-8" />
            <span className="text-base font-semibold">Coffer</span>
          </div>
          <div className="mb-6 flex size-11 items-center justify-center rounded-[12px] border border-border bg-card">
            <Lock className="size-5 text-[#E2A75A]" aria-hidden />
          </div>
          <h1 className="text-[22px] font-semibold tracking-[-0.015em]">Unlock vault</h1>
          <p className="mt-1 text-[13px] text-muted-foreground">Enter your master password to continue.</p>

          <Form {...form}>
            <form onSubmit={onSubmit} className="mt-6 space-y-4" noValidate>
              <FormField
                control={form.control}
                name="password"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel htmlFor="master-password">Master password</FormLabel>
                    <FormControl>
                      <div className="relative">
                        <Input
                          {...field}
                          id="master-password"
                          type={showPassword ? "text" : "password"}
                          autoFocus
                          autoComplete="current-password"
                          placeholder="••••••••••••"
                          className="h-10 bg-input-field pr-10 font-mono"
                          aria-invalid={Boolean(formError) || undefined}
                        />
                        <button
                          type="button"
                          onClick={() => setShowPassword((v) => !v)}
                          aria-label={showPassword ? "Hide password" : "Show password"}
                          className="absolute top-1/2 right-2 -translate-y-1/2 rounded p-1.5 text-faint hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                        >
                          {showPassword ? <EyeOff className="size-4" aria-hidden /> : <Eye className="size-4" aria-hidden />}
                        </button>
                      </div>
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              {formError ? (
                <p role="alert" className="rounded-md border border-[#3A1F23] bg-[#261214] px-3 py-2 text-[12px] text-[#F6DCDF]">
                  {formError}
                </p>
              ) : null}

              <Button type="submit" size="lg" className="h-11 w-full text-[13px] font-semibold" disabled={unlock.isPending}>
                {unlock.isPending ? <InlineSpinner /> : null}
                {unlock.isPending ? "Unlocking…" : "Unlock"}
              </Button>
            </form>
          </Form>

          <Tooltip>
            <TooltipTrigger asChild>
              <span className="mt-3 block">
                <Button variant="outline" className="h-10 w-full gap-2 text-[13px]" disabled>
                  <Fingerprint className="size-4" aria-hidden /> Unlock with Touch ID
                </Button>
              </span>
            </TooltipTrigger>
            <TooltipContent>Not enabled · turn it on after first unlock</TooltipContent>
          </Tooltip>

          <NoRecoveryNotice className="mt-5" />

          <div className="mt-6 flex items-center justify-between text-[11px] text-faint">
            <Link to="/setup" className="hover:text-muted-foreground">
              Create a new vault
            </Link>
            <span className="flex items-center gap-1">
              Press <Kbd>↵</Kbd> to unlock
            </span>
          </div>
        </div>
      </main>
    </div>
  );
}
