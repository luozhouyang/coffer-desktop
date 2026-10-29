import type { AppError, ErrorCode } from "@/lib/ipc-types";

/** Normalised IPC error. `message` is safe to render and log (never secrets). */
export class IpcError extends Error {
  readonly code: ErrorCode;

  constructor(code: ErrorCode, message: string) {
    super(message);
    this.name = "IpcError";
    this.code = code;
  }
}

const KNOWN_CODES: ReadonlySet<string> = new Set<ErrorCode>([
  "locked",
  "invalidPassword",
  "vaultNotInitialized",
  "alreadyInitialized",
  "notFound",
  "conflict",
  "validation",
  "io",
  "keychainUnavailable",
  "internal",
]);

export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    typeof (value as AppError).code === "string" &&
    "message" in value &&
    typeof (value as AppError).message === "string"
  );
}

/** Converts anything thrown by a Tauri invoke (or a mock) into an IpcError. */
export function toIpcError(err: unknown): IpcError {
  if (err instanceof IpcError) return err;
  if (isAppError(err)) {
    const code = KNOWN_CODES.has(err.code) ? err.code : "internal";
    return new IpcError(code, err.message);
  }
  if (err instanceof Error) return new IpcError("internal", err.message);
  if (typeof err === "string") return new IpcError("internal", err);
  return new IpcError("internal", "Unexpected error");
}

export function errorCode(err: unknown): ErrorCode | null {
  return err instanceof IpcError ? err.code : isAppError(err) ? err.code : null;
}

const MESSAGES: Record<ErrorCode, string> = {
  locked: "Vault is locked.",
  invalidPassword: "Incorrect master password.",
  vaultNotInitialized: "No vault exists yet.",
  alreadyInitialized: "A vault already exists.",
  notFound: "Not found.",
  conflict: "A conflicting item already exists.",
  validation: "The input is not valid.",
  io: "Could not read or write vault data.",
  keychainUnavailable: "Biometric unlock is not available.",
  internal: "Something went wrong.",
};

export function errorMessage(err: unknown): string {
  if (err instanceof IpcError) return err.message || MESSAGES[err.code];
  if (isAppError(err)) return err.message || MESSAGES[err.code] || MESSAGES.internal;
  if (err instanceof Error) return err.message;
  return MESSAGES.internal;
}

export function isLockedError(err: unknown): boolean {
  return errorCode(err) === "locked";
}
