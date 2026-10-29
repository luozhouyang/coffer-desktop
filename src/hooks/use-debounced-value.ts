import { useEffect, useState } from "react";

/** Debounces rapid value changes (used for the 120ms palette search). */
export function useDebouncedValue<T>(value: T, delayMs = 120): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs]);
  return debounced;
}
