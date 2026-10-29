const LOWER = "abcdefghijkmnopqrstuvwxyz";
const UPPER = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS = "23456789";
const SYMBOLS = "!@#$%^&*-_=+?";

function randomIndex(max: number): number {
  const buf = new Uint32Array(1);
  crypto.getRandomValues(buf);
  return buf[0] % max;
}

function pick(chars: string): string {
  return chars[randomIndex(chars.length)];
}

function shuffle<T>(input: T[]): T[] {
  const arr = [...input];
  for (let i = arr.length - 1; i > 0; i--) {
    const j = randomIndex(i + 1);
    [arr[i], arr[j]] = [arr[j], arr[i]];
  }
  return arr;
}

/** Cryptographically random secret; at least one char from each class. */
export function generateSecret(length = 32): string {
  const all = LOWER + UPPER + DIGITS + SYMBOLS;
  const chars = [pick(LOWER), pick(UPPER), pick(DIGITS), pick(SYMBOLS)];
  while (chars.length < length) chars.push(pick(all));
  return shuffle(chars).join("");
}

export type PasswordStrength = "empty" | "weak" | "fair" | "good" | "strong";

export function passwordStrength(password: string): PasswordStrength {
  if (!password) return "empty";
  let score = 0;
  if (password.length >= 8) score++;
  if (password.length >= 12) score++;
  if (/[a-z]/.test(password) && /[A-Z]/.test(password)) score++;
  if (/\d/.test(password)) score++;
  if (/[^A-Za-z0-9]/.test(password)) score++;
  if (score <= 1) return "weak";
  if (score === 2) return "fair";
  if (score === 3) return "good";
  return "strong";
}

export const STRENGTH_META: Record<PasswordStrength, { label: string; color: string; bars: number }> = {
  empty: { label: "", color: "transparent", bars: 0 },
  weak: { label: "Weak", color: "#E06C6C", bars: 1 },
  fair: { label: "Fair", color: "#D8B08A", bars: 2 },
  good: { label: "Good", color: "#E2A75A", bars: 3 },
  strong: { label: "Strong", color: "#7FD3A4", bars: 4 },
};
