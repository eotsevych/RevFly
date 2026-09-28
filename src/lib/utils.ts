import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Readable message from any thrown value (Error, string from a Tauri command, etc.). */
export function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
