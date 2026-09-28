interface ToggleProps {
  value: boolean;
  onChange: (v: boolean) => void;
  accent: string;
}

export default function Toggle({ value, onChange, accent }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={value}
      onClick={() => onChange(!value)}
      style={{
        width: 42,
        height: 24,
        borderRadius: 12,
        border: "none",
        cursor: "pointer",
        background: value ? accent : "rgba(128,128,128,0.2)",
        position: "relative",
        transition: "background 0.2s",
        flexShrink: 0,
      }}
    >
      <div
        style={{
          position: "absolute",
          top: 3,
          left: value ? 21 : 3,
          width: 18,
          height: 18,
          borderRadius: "50%",
          background: "#fff",
          transition: "left 0.2s",
          boxShadow: "0 1px 4px rgba(0,0,0,0.3)",
        }}
      />
    </button>
  );
}
