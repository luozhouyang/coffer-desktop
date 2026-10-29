import { useState } from "react";
import { AlertTriangle, Fingerprint, KeyRound, ShieldCheck } from "lucide-react";
import {
  useDisableKeychain,
  useEnableKeychain,
  useKeychainStatus,
  useSettings,
  useSaveSettings,
  useVaultStatus,
} from "@/app/queries";
import { useVault } from "@/app/vault-context";
import { errorMessage } from "@/lib/ipc-errors";
import type { Settings } from "@/lib/ipc-types";
import { formatDuration } from "@/lib/format";
import { ErrorState, InlineSpinner } from "@/components/common/status-bits";
import { NoRecoveryNotice } from "@/components/common/no-recovery-notice";
import { Button } from "@/components/ui/button";
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "sonner";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

const AUTO_LOCK_OPTIONS = [
  { value: "60", label: "1 minute" },
  { value: "300", label: "5 minutes" },
  { value: "900", label: "15 minutes" },
  { value: "1800", label: "30 minutes" },
  { value: "3600", label: "60 minutes" },
];

const CLIPBOARD_OPTIONS = [
  { value: "15", label: "15 seconds" },
  { value: "30", label: "30 seconds" },
  { value: "60", label: "60 seconds" },
];

export function SecuritySettingsPage() {
  const settingsQuery = useSettings();
  const statusQuery = useVaultStatus();
  const save = useSaveSettings();
  const { lock } = useVault();
  const keychainQuery = useKeychainStatus();
  const enableKeychain = useEnableKeychain();
  const disableKeychain = useDisableKeychain();
  const keychain = keychainQuery.data;

  const [touchIdOpen, setTouchIdOpen] = useState(false);
  const [touchIdPassword, setTouchIdPassword] = useState("");
  const [touchIdError, setTouchIdError] = useState<string | null>(null);

  const onTouchIdToggle = (checked: boolean) => {
    if (checked) {
      setTouchIdPassword("");
      setTouchIdError(null);
      setTouchIdOpen(true);
      return;
    }
    disableKeychain.mutate(undefined, {
      onSuccess: () => toast.success("Touch ID unlock turned off"),
      onError: (err) => toast.error("Could not turn off Touch ID", { description: errorMessage(err) }),
    });
  };

  const submitTouchId = async (event: React.FormEvent) => {
    event.preventDefault();
    setTouchIdError(null);
    try {
      await enableKeychain.mutateAsync(touchIdPassword);
      setTouchIdOpen(false);
      setTouchIdPassword("");
      toast.success("Touch ID unlock enabled");
    } catch (err) {
      setTouchIdError(errorMessage(err));
    }
  };

  const update = (patch: Partial<Settings>) => {
    const current = settingsQuery.data;
    if (!current) return;
    const next: Settings = { ...current, ...patch };
    save.mutate(next, {
      onSuccess: () => toast.success("Settings saved", { id: "settings-saved" }),
      onError: (err) => toast.error("Could not save settings", { description: errorMessage(err) }),
    });
  };

  if (settingsQuery.isPending) {
    return <SettingsSkeleton />;
  }
  if (settingsQuery.isError || !settingsQuery.data) {
    return (
      <ErrorState
        title="Could not load settings"
        message={errorMessage(settingsQuery.error)}
        onRetry={() => void settingsQuery.refetch()}
      />
    );
  }

  const settings = settingsQuery.data;
  const autoLock = statusQuery.data?.autoLockSecs ?? settings.autoLockSecs;

  return (
    <div>
      <header>
        <div className="flex items-center gap-2 text-[11px] font-medium uppercase tracking-[0.12em] text-faint">
          <ShieldCheck className="size-3.5 text-[#E2A75A]" aria-hidden /> Security
        </div>
        <h1 className="mt-2 text-[24px] font-semibold tracking-[-0.015em]">Security</h1>
        <p className="mt-1 text-[13px] text-muted-foreground">
          Tune how long the vault stays open and how quickly copied secrets disappear.
        </p>
      </header>

      <section className="mt-8 space-y-4" aria-labelledby="locking-heading">
        <h2 id="locking-heading" className="text-[13px] font-semibold">
          Locking
        </h2>
        <div className="divide-y divide-border rounded-[14px] border border-border bg-card">
          <SettingRow
            title="Lock after inactivity"
            description="The vault wipes keys from memory after this idle period."
            control={
              <Select value={String(settings.autoLockSecs)} onValueChange={(v) => update({ autoLockSecs: Number(v) })}>
                <SelectTrigger size="sm" aria-label="Lock after inactivity" className="w-[150px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent align="end">
                  {(AUTO_LOCK_OPTIONS.some((o) => Number(o.value) === settings.autoLockSecs)
                    ? AUTO_LOCK_OPTIONS
                    : [...AUTO_LOCK_OPTIONS, { value: String(settings.autoLockSecs), label: formatDuration(settings.autoLockSecs) }]
                  ).map((o) => (
                    <SelectItem key={o.value} value={o.value}>
                      {o.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            }
          />
          <SettingRow
            title="Lock when the screen locks"
            description="Not configurable yet — the vault always locks with the OS session."
            control={<Switch checked disabled aria-label="Lock when the screen locks" />}
          />
        </div>
      </section>

      {keychain?.supported ? (
        <section className="mt-8 space-y-4" aria-labelledby="touch-id-heading">
          <h2 id="touch-id-heading" className="text-[13px] font-semibold">
            Biometric unlock
          </h2>
          <div className="rounded-[14px] border border-border bg-card">
            <SettingRow
              title="Unlock with Touch ID"
              description={
                !keychain.available
                  ? "Touch ID is not set up on this Mac. Add a fingerprint in System Settings first."
                  : keychain.enabled
                    ? "The vault key stays in the macOS keychain and is released only after Touch ID. Your master password still works."
                    : "Skip typing your master password at launch. Enabling stores the vault key in the macOS keychain, protected by Touch ID."
              }
              control={
                <Switch
                  checked={keychain.enabled}
                  disabled={!keychain.available || enableKeychain.isPending || disableKeychain.isPending}
                  onCheckedChange={onTouchIdToggle}
                  aria-label="Unlock with Touch ID"
                />
              }
            />
          </div>
        </section>
      ) : null}

      <section className="mt-8 space-y-4" aria-labelledby="clipboard-heading">
        <h2 id="clipboard-heading" className="text-[13px] font-semibold">
          Clipboard
        </h2>
        <div className="divide-y divide-border rounded-[14px] border border-border bg-card">
          <SettingRow
            title="Clear clipboard after copying"
            description="Coffer only clears the clipboard if it still contains the copied secret."
            control={
              <Select
                value={String(settings.clipboardClearSecs)}
                onValueChange={(v) => update({ clipboardClearSecs: Number(v) })}
              >
                <SelectTrigger size="sm" aria-label="Clear clipboard after copying" className="w-[150px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent align="end">
                  {(CLIPBOARD_OPTIONS.some((o) => Number(o.value) === settings.clipboardClearSecs)
                    ? CLIPBOARD_OPTIONS
                    : [...CLIPBOARD_OPTIONS, { value: String(settings.clipboardClearSecs), label: `${settings.clipboardClearSecs} seconds` }]
                  ).map((o) => (
                    <SelectItem key={o.value} value={o.value}>
                      {o.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            }
          />
        </div>
      </section>

      <section className="mt-8 space-y-4" aria-labelledby="search-heading">
        <h2 id="search-heading" className="text-[13px] font-semibold">
          Search
        </h2>
        <div className="rounded-[14px] border border-border bg-card">
          <SettingRow
            title="Deep search secret values"
            description="Off by default. When on, ⌘K also scans decrypted secret values, temporarily in memory."
            control={
              <Switch
                checked={settings.deepSearch}
                onCheckedChange={(checked) => update({ deepSearch: checked })}
                aria-label="Deep search secret values"
              />
            }
          />
        </div>
      </section>

      <section className="mt-8 grid gap-4 lg:grid-cols-2" aria-labelledby="more-heading">
        <h2 id="more-heading" className="sr-only">
          Master password and vault
        </h2>
        <div className="rounded-[14px] border border-border bg-card p-4">
          <div className="flex items-center gap-2 text-[13px] font-semibold">
            <KeyRound className="size-4 text-[#E2A75A]" aria-hidden /> Master password
          </div>
          <p className="mt-1.5 text-[12px] leading-relaxed text-muted-foreground">
            Changing your master password re-wraps the vault key. Your secrets stay the same.
          </p>
          <Button variant="outline" size="sm" className="mt-3" disabled>
            Change master password
          </Button>
        </div>
        <div className="rounded-[14px] border border-border bg-card p-4">
          <div className="flex items-center gap-2 text-[13px] font-semibold">
            <AlertTriangle className="size-4 text-warning" aria-hidden /> Current session
          </div>
          <p className="mt-1.5 text-[12px] leading-relaxed text-muted-foreground">
            Auto-lock is set to {formatDuration(autoLock)}. You can lock the vault immediately at any time.
          </p>
          <Button variant="secondary" size="sm" className="mt-3" onClick={() => void lock()}>
            Lock vault now
          </Button>
        </div>
      </section>

      <Separator className="my-8" />
      <NoRecoveryNotice />
      {save.isPending ? (
        <p className="mt-4 flex items-center gap-2 text-[11px] text-faint" role="status">
          <InlineSpinner className="size-3" /> Saving…
        </p>
      ) : null}

      <Dialog
        open={touchIdOpen}
        onOpenChange={(open) => {
          if (enableKeychain.isPending) return;
          setTouchIdOpen(open);
          if (!open) {
            setTouchIdPassword("");
            setTouchIdError(null);
          }
        }}
      >
        <DialogContent className="sm:max-w-[400px]">
          <DialogHeader>
            <div className="flex items-center gap-2 text-[13px] font-semibold">
              <Fingerprint className="size-4 text-[#E2A75A]" aria-hidden /> Enable Touch ID
            </div>
            <DialogTitle className="sr-only">Enable Touch ID</DialogTitle>
            <DialogDescription>
              Enter your master password once. Coffer stores the vault key in the macOS keychain with a Touch ID
              requirement; the key never leaves this Mac.
            </DialogDescription>
          </DialogHeader>
          <form onSubmit={(event) => void submitTouchId(event)} className="space-y-3" noValidate>
            <div>
              <Label htmlFor="touch-id-password">Master password</Label>
              <Input
                id="touch-id-password"
                type="password"
                autoFocus
                autoComplete="current-password"
                value={touchIdPassword}
                onChange={(event) => setTouchIdPassword(event.target.value)}
                className="mt-1.5 h-10 font-mono"
              />
            </div>
            {touchIdError ? (
              <p role="alert" className="rounded-md border border-[#3A1F23] bg-[#261214] px-3 py-2 text-[12px] text-[#F6DCDF]">
                {touchIdError}
              </p>
            ) : null}
            <div className="flex justify-end gap-2 pt-1">
              <DialogClose asChild>
                <Button type="button" variant="ghost" disabled={enableKeychain.isPending}>
                  Cancel
                </Button>
              </DialogClose>
              <Button type="submit" disabled={enableKeychain.isPending || touchIdPassword.length === 0}>
                {enableKeychain.isPending ? <InlineSpinner /> : null}
                {enableKeychain.isPending ? "Enabling…" : "Enable"}
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function SettingRow({
  title,
  description,
  control,
}: {
  title: string;
  description: string;
  control: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 px-4 py-3.5">
      <div className="min-w-0">
        <Label className="text-[13px] font-medium">{title}</Label>
        <p className="mt-0.5 text-[11.5px] leading-relaxed text-muted-foreground">{description}</p>
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  );
}

function SettingsSkeleton() {
  return (
    <div className="space-y-4" aria-label="Loading settings" role="status">
      <Skeleton className="h-7 w-40" />
      <Skeleton className="h-4 w-72" />
      <Skeleton className="mt-8 h-[120px] w-full rounded-[14px]" />
      <Skeleton className="h-[70px] w-full rounded-[14px]" />
      <Skeleton className="h-[70px] w-full rounded-[14px]" />
    </div>
  );
}
