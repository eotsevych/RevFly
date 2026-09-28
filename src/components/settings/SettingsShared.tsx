import type { ReactNode, InputHTMLAttributes, TextareaHTMLAttributes } from "react";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";

export function Section({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section className="font-mono">
      <header className="flex items-center gap-2 px-1 pb-3 pt-2">
        <span className="h-1.5 w-1.5 rounded-full bg-primary/60" aria-hidden="true" />
        <h3 className="text-xs font-medium uppercase text-foreground">{title}</h3>
      </header>
      {description ? (
        <p className="px-1 pb-4 text-[11px] text-pill-muted">
          <span className="text-primary/70"># </span>
          {description}
        </p>
      ) : null}
      <div className="space-y-5 px-1 pb-6">{children}</div>
    </section>
  );
}

export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="space-y-1.5 font-mono">
      <p className="text-[11px] uppercase text-muted-foreground">
        <span className="text-primary">&gt;</span> {label}
      </p>
      {children}
      {hint ? (
        <p className="text-[11px] text-muted-foreground">
          <span className="text-primary/70"># </span>
          {hint}
        </p>
      ) : null}
    </div>
  );
}

export function TerminalInput({ className = "", ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <div className="flex items-center gap-3 rounded-full border border-pill-border bg-pill-inset px-4 transition-colors focus-within:border-primary">
      <span className="select-none text-xs text-primary/70">&#9656;</span>
      <input
        {...props}
        className={`h-11 w-full bg-transparent font-mono text-xs text-foreground caret-primary outline-none placeholder:text-pill-muted ${className}`}
      />
    </div>
  );
}

export function TerminalTextarea({
  className = "",
  ...props
}: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return (
    <div className="rounded-xl border border-pill-border bg-pill-inset p-4 transition-colors focus-within:border-primary">
      <textarea
        {...props}
        className={`w-full resize-none bg-transparent font-mono text-xs leading-relaxed text-foreground caret-primary outline-none placeholder:text-muted-foreground/70 ${className}`}
      />
    </div>
  );
}

export function ToggleRow({
  title,
  description,
  checked,
  onCheckedChange,
}: {
  title: string;
  description: string;
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between rounded-xl border border-pill-border bg-pill-inset px-4 py-3.5 font-mono">
      <div className="pr-4">
        <p className="text-xs font-medium text-foreground">
          <span className={checked ? "text-primary" : "text-muted-foreground"}>
            [{checked ? "x" : " "}]
          </span>{" "}
          {title}
        </p>
        <p className="mt-0.5 text-[11px] text-muted-foreground">{description}</p>
      </div>
      <Switch checked={checked} onCheckedChange={onCheckedChange} />
    </div>
  );
}

export function Picker({
  value,
  onValueChange,
  options,
}: {
  value: string;
  onValueChange: (value: string) => void;
  options: string[];
}) {
  return (
    <div className="flex flex-wrap gap-1.5 font-mono">
      {options.map((option) => {
        const active = option === value;
        return (
          <Button
            key={option}
            type="button"
            variant="outline"
            onClick={() => onValueChange(option)}
            className={`h-8 rounded-full px-3 font-mono text-[10px] font-normal uppercase transition-colors ${
              active
                ? "border-primary bg-primary/15 text-primary hover:bg-primary/15"
                : "border-pill-border bg-pill-inset text-muted-foreground hover:border-primary/50 hover:bg-pill-inset hover:text-foreground"
            }`}
          >
            <span className="opacity-60">[</span>
            {active ? "•" : " "}
            <span className="opacity-60">]</span> {option}
          </Button>
        );
      })}
    </div>
  );
}
