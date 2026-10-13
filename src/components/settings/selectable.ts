import type { CSSProperties } from "react";

/** Lets text be selected and copied inside the Settings window, which turns selection off. */
export const SELECTABLE: CSSProperties = {
  userSelect: "text",
  WebkitUserSelect: "text",
  cursor: "text",
};
