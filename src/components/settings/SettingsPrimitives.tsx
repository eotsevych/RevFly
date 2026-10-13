import { useState } from "react";
import type React from "react";
import type { Tokens } from "../../lib/tokens";

// ── Copy-to-clipboard icon button ──────────────────────────────────────────
export function CopyButton({
  text,
  t,
  title = "Copy",
}: {
  text: string;
  t: Tokens;
  title?: string;
}) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const color = state === "copied" ? t.successColor : state === "failed" ? t.errorColor : t.textDim;
  return (
    <button
      type="button"
      title={state === "copied" ? "Copied" : state === "failed" ? "Couldn't copy" : title}
      aria-label={title}
      disabled={!text}
      onClick={async (e) => {
        e.stopPropagation();
        try {
          await navigator.clipboard.writeText(text);
          setState("copied");
        } catch {
          setState("failed");
        }
        setTimeout(() => setState("idle"), 1500);
      }}
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        width: 22,
        height: 22,
        flexShrink: 0,
        padding: 0,
        borderRadius: 5,
        border: `1px solid ${t.border}`,
        background: "transparent",
        color,
        cursor: text ? "pointer" : "default",
        opacity: text ? 1 : 0.4,
      }}
    >
      {state === "copied" ? (
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <path
            d="M3 8.5l3.2 3L13 4.5"
            stroke="currentColor"
            strokeWidth="1.8"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      ) : (
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <rect
            x="5"
            y="5"
            width="8.5"
            height="8.5"
            rx="1.6"
            stroke="currentColor"
            strokeWidth="1.4"
          />
          <path
            d="M10.5 3.2V3a1.5 1.5 0 0 0-1.5-1.5H3.5A1.5 1.5 0 0 0 2 3v5.5A1.5 1.5 0 0 0 3.5 10h.2"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
          />
        </svg>
      )}
    </button>
  );
}

// ── Section block ──────────────────────────────────────────────────────────
export function Section({
  title,
  children,
  t,
  action,
}: {
  title: string;
  children: React.ReactNode;
  t: Tokens;
  /** Shown at the right of the title, e.g. a button that copies the whole section. */
  action?: React.ReactNode;
}) {
  return (
    <div style={{ marginBottom: 24 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
          marginBottom: 8,
        }}
      >
        <p
          style={{
            fontSize: 11,
            fontWeight: 600,
            letterSpacing: "0.1em",
            textTransform: "uppercase",
            color: t.textDim,
            fontFamily: "JetBrains Mono, monospace",
            margin: 0,
          }}
        >
          {title}
        </p>
        {action}
      </div>
      <div
        style={{
          background: t.surface,
          border: `1px solid ${t.border}`,
          borderRadius: 10,
          overflow: "hidden",
        }}
      >
        {children}
      </div>
    </div>
  );
}

// ── Row with label/hint on left, control on right ──────────────────────────
export function Row({
  label,
  hint,
  children,
  t,
  last,
}: {
  label: React.ReactNode;
  hint?: string;
  children: React.ReactNode;
  t: Tokens;
  last?: boolean;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "12px 16px",
        borderBottom: last ? "none" : `1px solid ${t.border}`,
        gap: 16,
      }}
    >
      <div style={{ flex: 1, minWidth: 0 }}>
        <p
          style={{
            fontSize: 14,
            color: t.text,
            fontFamily: "Inter, sans-serif",
            margin: 0,
            fontWeight: 500,
          }}
        >
          {label}
        </p>
        {hint && (
          <p
            style={{
              fontSize: 12,
              color: t.textMuted,
              fontFamily: "Inter, sans-serif",
              margin: "2px 0 0",
            }}
          >
            {hint}
          </p>
        )}
      </div>
      {children}
    </div>
  );
}

// ── Form field label ───────────────────────────────────────────────────────
export function FormLabel({ children, t }: { children: React.ReactNode; t: Tokens }) {
  return (
    <p
      style={{
        fontSize: 11,
        fontWeight: 600,
        letterSpacing: "0.05em",
        textTransform: "uppercase",
        color: t.label,
        fontFamily: "JetBrains Mono, monospace",
        margin: "0 0 5px",
      }}
    >
      {children}
    </p>
  );
}

export function FormGroup({ children }: { children: React.ReactNode }) {
  return <div style={{ marginBottom: 14 }}>{children}</div>;
}

// ── Text input ─────────────────────────────────────────────────────────────
export function FieldInput({
  value,
  onChange,
  placeholder,
  type = "text",
  mono,
  t,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
  mono?: boolean;
  t: Tokens;
}) {
  return (
    <input
      type={type}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      style={{
        background: t.inputBg,
        border: `1px solid ${t.inputBorder}`,
        borderRadius: 7,
        padding: "8px 11px",
        color: t.inputText,
        fontSize: 13,
        fontFamily: mono ? "JetBrains Mono, monospace" : "Inter, sans-serif",
        outline: "none",
        width: "100%",
        transition: "border-color 0.2s",
        boxSizing: "border-box",
      }}
      onFocus={(e) => {
        e.currentTarget.style.borderColor = t.borderFocus;
      }}
      onBlur={(e) => {
        e.currentTarget.style.borderColor = t.inputBorder;
      }}
    />
  );
}

// ── Select ─────────────────────────────────────────────────────────────────
export function FieldSelect({
  value,
  onChange,
  options,
  t,
  disabled = false,
}: {
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
  t: Tokens;
  disabled?: boolean;
}) {
  const [isFocused, setIsFocused] = useState(false);
  const [isHovered, setIsHovered] = useState(false);

  return (
    <div
      style={{
        position: "relative",
        display: "inline-flex",
        alignItems: "center",
        minWidth: 140,
        maxWidth: 360,
      }}
    >
      <select
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        onFocus={() => setIsFocused(true)}
        onBlur={() => setIsFocused(false)}
        onMouseEnter={() => setIsHovered(true)}
        onMouseLeave={() => setIsHovered(false)}
        style={{
          appearance: "none",
          WebkitAppearance: "none",
          width: "100%",
          background: isHovered && !disabled ? t.surfaceHover : t.inputBg,
          border: `1px solid ${isFocused ? t.accent : isHovered ? t.borderFocus : t.inputBorder}`,
          borderRadius: 8,
          padding: "7px 30px 7px 12px",
          color: disabled ? t.textDim : t.inputText,
          fontSize: 13,
          fontWeight: 450,
          fontFamily: "Inter, -apple-system, sans-serif",
          outline: "none",
          cursor: disabled ? "not-allowed" : "pointer",
          transition: "all 0.15s ease",
          boxShadow: isFocused ? `0 0 0 2px ${t.accent}22` : "none",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          overflow: "hidden",
        }}
      >
        {options.map((o) => (
          <option
            key={o.value}
            value={o.value}
            style={{
              background: t.modalBg,
              color: t.inputText,
              padding: "6px 10px",
            }}
          >
            {o.label}
          </option>
        ))}
      </select>

      {/* Modern styled Chevron Down indicator */}
      <div
        style={{
          position: "absolute",
          right: 10,
          pointerEvents: "none",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: isFocused ? t.accent : isHovered ? t.text : t.textDim,
          transition: "color 0.15s ease, transform 0.15s ease",
          transform: isFocused ? "rotate(180deg)" : "none",
        }}
      >
        <svg
          width="11"
          height="11"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </div>
    </div>
  );
}

// ── Textarea ───────────────────────────────────────────────────────────────
export function FieldTextarea({
  value,
  onChange,
  placeholder,
  rows = 3,
  mono,
  t,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  rows?: number;
  mono?: boolean;
  t: Tokens;
}) {
  return (
    <textarea
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      rows={rows}
      style={{
        background: t.inputBg,
        border: `1px solid ${t.inputBorder}`,
        borderRadius: 7,
        padding: "8px 11px",
        color: t.inputText,
        fontSize: 13,
        fontFamily: mono ? "JetBrains Mono, monospace" : "Inter, sans-serif",
        outline: "none",
        width: "100%",
        resize: "vertical",
        transition: "border-color 0.2s",
        boxSizing: "border-box",
      }}
      onFocus={(e) => {
        e.currentTarget.style.borderColor = t.borderFocus;
      }}
      onBlur={(e) => {
        e.currentTarget.style.borderColor = t.inputBorder;
      }}
    />
  );
}

// ── API Key Input with reveal toggle ──────────────────────────────────────
export function ApiKeyInput({
  value,
  onChange,
  placeholder = "Paste API key…",
  t,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  t: Tokens;
}) {
  const [show, setShow] = useState(false);
  return (
    <div style={{ position: "relative", width: "100%" }}>
      <input
        type={show ? "text" : "password"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        style={{
          background: t.inputBg,
          border: `1px solid ${t.inputBorder}`,
          borderRadius: 7,
          padding: "8px 38px 8px 11px",
          color: t.inputText,
          fontSize: 13,
          fontFamily: "JetBrains Mono, monospace",
          outline: "none",
          width: "100%",
          boxSizing: "border-box",
          transition: "border-color 0.2s",
        }}
        onFocus={(e) => {
          e.currentTarget.style.borderColor = t.borderFocus;
        }}
        onBlur={(e) => {
          e.currentTarget.style.borderColor = t.inputBorder;
        }}
      />
      <button
        type="button"
        onClick={() => setShow((s) => !s)}
        style={{
          position: "absolute",
          right: 8,
          top: "50%",
          transform: "translateY(-50%)",
          background: "none",
          border: "none",
          cursor: "pointer",
          color: t.textDim,
          fontSize: 13,
          padding: "2px 4px",
          lineHeight: 1,
        }}
        title={show ? "Hide" : "Show"}
      >
        {show ? "🙈" : "👁"}
      </button>
    </div>
  );
}
