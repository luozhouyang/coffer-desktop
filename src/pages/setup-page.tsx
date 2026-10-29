import { useState } from "react";
import { useForm, useWatch } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { useNavigate } from "@tanstack/react-router";
import { Eye, EyeOff, KeyRound, ShieldAlert } from "lucide-react";
import { useCreateVault } from "@/app/queries";
import { passwordStrength, STRENGTH_META } from "@/lib/password";
import { errorMessage } from "@/lib/ipc-errors";
import { BrandMark } from "@/components/common/brand";
import { NoRecoveryNotice } from "@/components/common/no-recovery-notice";
import { InlineSpinner } from "@/components/common/status-bits";
import { Checkbox } from "@/components/ui/checkbox";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Form, FormControl, FormField, FormItem, FormLabel, FormMessage } from "@/components/ui/form";
import { cn } from "@/lib/utils";

const setupSchema = z
  .object({
    password: z.string().min(8, "Use at least 8 characters."),
    confirm: z.string().min(1, "Confirm your master password."),
    acknowledged: z.boolean(),
  })
  .refine((v) => v.password === v.confirm, { path: ["confirm"], message: "Passwords do not match." })
  .refine((v) => v.acknowledged, {
    path: ["acknowledged"],
    message: "Confirm that you understand there is no recovery.",
  });

type SetupValues = z.infer<typeof setupSchema>;

export function SetupPage() {
  const navigate = useNavigate();
  const createVault = useCreateVault();
  const [showPassword, setShowPassword] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const form = useForm<SetupValues>({
    resolver: zodResolver(setupSchema),
    defaultValues: { password: "", confirm: "", acknowledged: false },
  });
  const password = useWatch({ control: form.control, name: "password" });
  const strength = STRENGTH_META[passwordStrength(password)];

  const onSubmit = form.handleSubmit(async ({ password }) => {
    setFormError(null);
    try {
      await createVault.mutateAsync(password);
      void navigate({ to: "/", replace: true });
    } catch (err) {
      setFormError(errorMessage(err));
    }
  });

  return (
    <div className="flex min-h-svh items-center justify-center bg-background px-6 py-10">
      <div className="w-full max-w-[420px]">
        <div className="mb-8 flex items-center gap-3">
          <BrandMark className="size-9" />
          <div>
            <h1 className="text-[20px] font-semibold tracking-[-0.015em]">Create your vault</h1>
            <p className="text-[12px] text-muted-foreground">Choose a strong master password. This is the only key.</p>
          </div>
        </div>

        <div className="rounded-[14px] border border-border bg-card p-5">
          <Form {...form}>
            <form onSubmit={onSubmit} className="space-y-4" noValidate>
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
                          autoComplete="new-password"
                          placeholder="At least 8 characters"
                          className="h-10 bg-input-field pr-10 font-mono"
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
                    <StrengthMeter label={strength.label} color={strength.color} bars={strength.bars} />
                    <FormMessage />
                  </FormItem>
                )}
              />
              <FormField
                control={form.control}
                name="confirm"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel htmlFor="confirm-password">Confirm password</FormLabel>
                    <FormControl>
                      <Input
                        {...field}
                        id="confirm-password"
                        type="password"
                        autoComplete="new-password"
                        placeholder="Repeat your master password"
                        className="h-10 bg-input-field font-mono"
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="acknowledged"
                render={({ field }) => (
                  <FormItem>
                    <label
                      htmlFor="acknowledge-no-recovery"
                      className="flex cursor-pointer items-start gap-2.5 rounded-[10px] border border-[#4A3720] bg-[#2A2114]/40 p-3 text-[12px] leading-relaxed"
                    >
                      <FormControl>
                        <Checkbox
                          id="acknowledge-no-recovery"
                          checked={field.value}
                          onCheckedChange={(v) => field.onChange(v === true)}
                          className="mt-0.5 size-4 shrink-0 rounded-[4px] border-[#4A3720] bg-[#0F131A] data-[state=checked]:border-[#E2A75A] data-[state=checked]:bg-[#E2A75A] data-[state=checked]:text-[#1A1307]"
                        />
                      </FormControl>
                      <span className="text-warning">
                        <span className="font-semibold text-[#E9ECF2]">I understand there is no recovery.</span> If I
                        lose my master password, my secrets cannot be opened by anyone — including me.
                      </span>
                    </label>
                    <FormMessage />
                  </FormItem>
                )}
              />

              {formError ? (
                <p role="alert" className="rounded-md border border-[#3A1F23] bg-[#261214] px-3 py-2 text-[12px] text-[#F6DCDF]">
                  {formError}
                </p>
              ) : null}

              <Button type="submit" size="lg" className="h-11 w-full gap-2 text-[13px] font-semibold" disabled={createVault.isPending}>
                {createVault.isPending ? <InlineSpinner /> : <KeyRound className="size-4" aria-hidden />}
                {createVault.isPending ? "Creating vault…" : "Create vault"}
              </Button>
            </form>
          </Form>

          <NoRecoveryNotice className="mt-4" />
          <p className="mt-3 flex items-start gap-2 text-[11px] text-faint">
            <ShieldAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden />
            Keys are derived on this device and never leave it. Back up your vault file yourself.
          </p>
        </div>
      </div>
    </div>
  );
}

function StrengthMeter({ label, color, bars }: { label: string; color: string; bars: number }) {
  if (!label) return null;
  return (
    <div className="flex items-center gap-2" aria-live="polite">
      <div className="flex gap-1">
        {[1, 2, 3, 4].map((i) => (
          <span
            key={i}
            className={cn("h-1 w-8 rounded-full bg-[#2A303B]")}
            style={i <= bars ? { backgroundColor: color } : undefined}
          />
        ))}
      </div>
      <span className="text-[11px]" style={{ color }}>
        {label}
      </span>
    </div>
  );
}
