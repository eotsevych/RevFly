import type { Tokens } from "@/lib/tokens";
import Toggle from "@/components/ui/Toggle";
import Segmented from "@/components/ui/Segmented";
import { Section, Row, FieldTextarea } from "./SettingsPrimitives";
import { useSettingsContext } from "./useSettingsContext";

/** What happens to the text after transcription: cleanup and masking of confidential words. */
export default function TextTab({ t }: { t: Tokens }) {
  const { settings, updateSettings } = useSettingsContext();

  return (
    <div>
      {/* ── Post-Processing & Formatting (LLM-Friendly) ── */}
      <Section title="Post-Processing & Formatting (LLM-Friendly)" t={t}>
        <Row
          label="LLM-Friendly Normalization"
          hint="Master toggle to prepare spoken audio cleanly for downstream LLMs"
          t={t}
          last={!settings.text_normalization}
        >
          <Toggle
            value={settings.text_normalization ?? true}
            onChange={(v) => updateSettings({ text_normalization: v })}
            accent={t.accent}
          />
        </Row>

        {settings.text_normalization && (
          <>
            <Row
              label="Remove filler words"
              hint="Strip spoken 'um', 'uh', 'you know', 'like'"
              t={t}
            >
              <Toggle
                value={settings.remove_filler_words ?? true}
                onChange={(v) => updateSettings({ remove_filler_words: v })}
                accent={t.accent}
              />
            </Row>
            <Row
              label="Convert numbers to digits"
              hint="Convert 'twenty twenty four' to 2024"
              t={t}
            >
              <Toggle
                value={settings.convert_numbers ?? true}
                onChange={(v) => updateSettings({ convert_numbers: v })}
                accent={t.accent}
              />
            </Row>
            <Row
              label="Remove stutters"
              hint="Collapse immediate repeated words (e.g. 'I I want' → 'I want')"
              t={t}
            >
              <Toggle
                value={settings.remove_stutters ?? true}
                onChange={(v) => updateSettings({ remove_stutters: v })}
                accent={t.accent}
              />
            </Row>
            <Row
              label="Apply self-corrections"
              hint="Process self-correction edits (e.g. 'on Monday, no Tuesday')"
              t={t}
            >
              <Toggle
                value={settings.apply_self_corrections ?? true}
                onChange={(v) => updateSettings({ apply_self_corrections: v })}
                accent={t.accent}
              />
            </Row>
            <Row
              label="Strip noise markers"
              hint="Strip audio tags like [applause], [laughter], (cough)"
              t={t}
            >
              <Toggle
                value={settings.remove_noise_markers ?? true}
                onChange={(v) => updateSettings({ remove_noise_markers: v })}
                accent={t.accent}
              />
            </Row>
            <Row
              label="Collapse redundancy"
              hint="Merge immediate duplicate phrases into one"
              t={t}
            >
              <Toggle
                value={settings.collapse_redundancy ?? true}
                onChange={(v) => updateSettings({ collapse_redundancy: v })}
                accent={t.accent}
              />
            </Row>
            <Row label="Annotate ambiguities" hint="Mark uncertain phonetic spans with [?]" t={t}>
              <Toggle
                value={settings.annotate_ambiguity ?? true}
                onChange={(v) => updateSettings({ annotate_ambiguity: v })}
                accent={t.accent}
              />
            </Row>
            <Row
              label="Structured values"
              hint="Format currency, dates, percentages into consistent tokens"
              t={t}
              last
            >
              <Toggle
                value={settings.normalize_structured_values ?? true}
                onChange={(v) => updateSettings({ normalize_structured_values: v })}
                accent={t.accent}
              />
            </Row>
          </>
        )}
      </Section>

      {/* ── Confidential Text & Masking ── */}
      <Section title="Confidential Text & Masking" t={t}>
        <Row
          label="Mask Confidential Text"
          hint="Redact or replace secret words, names, and patterns before translation or pasting"
          t={t}
          last={!settings.mask_confidential}
        >
          <Toggle
            value={settings.mask_confidential ?? false}
            onChange={(v) => updateSettings({ mask_confidential: v })}
            accent={t.accent}
          />
        </Row>

        {settings.mask_confidential && (
          <>
            <div style={{ padding: "12px 16px", borderBottom: `1px solid ${t.border}` }}>
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "baseline",
                  marginBottom: 6,
                }}
              >
                <p
                  style={{
                    fontSize: 13,
                    color: t.text,
                    fontFamily: "Inter, sans-serif",
                    margin: 0,
                    fontWeight: 450,
                  }}
                >
                  Words & Replacement Rules
                </p>
                <span
                  style={{
                    fontSize: 10,
                    color: t.textDim,
                    fontFamily: "JetBrains Mono, monospace",
                  }}
                >
                  e.g. SecretKey -&gt; [KEY] or plain word
                </span>
              </div>
              <p
                style={{
                  fontSize: 11,
                  color: t.textMuted,
                  fontFamily: "Inter, sans-serif",
                  margin: "0 0 8px",
                }}
              >
                Add one word or phrase per line (or comma-separated). Use <code>-&gt;</code> or{" "}
                <code>=</code> for custom replacements.
              </p>
              <FieldTextarea
                value={settings.mask_words ?? ""}
                onChange={(v) => updateSettings({ mask_words: v })}
                placeholder="John -&gt; [NAME]&#10;SecretProject -&gt; [PROJECT]&#10;password&#10;api_key"
                rows={4}
                mono
                t={t}
              />
            </div>

            <Row
              label="Match Sensitivity"
              hint="Fuzzy threshold to catch speech recognition typos (e.g. 90% catches speech slips)"
              t={t}
            >
              <Segmented
                options={[
                  { value: "100", label: "100% (Exact)" },
                  { value: "90", label: "90% (Rec.)" },
                  { value: "85", label: "85%" },
                  { value: "80", label: "80%" },
                ]}
                value={String(settings.mask_threshold ?? 90)}
                onChange={(v) => updateSettings({ mask_threshold: Number(v) })}
                t={t}
              />
            </Row>

            <Row
              label="Default Mask Token"
              hint="Used when no specific -&gt; replacement token is configured"
              t={t}
            >
              <Segmented
                options={[
                  { value: "***", label: "***" },
                  { value: "[REDACTED]", label: "[REDACTED]" },
                  { value: "[CONFIDENTIAL]", label: "[CONFIDENTIAL]" },
                  { value: "••••", label: "••••" },
                ]}
                value={settings.mask_format ?? "***"}
                onChange={(v) => updateSettings({ mask_format: v })}
                t={t}
              />
            </Row>

            <Row
              label="Auto-mask email addresses"
              hint="Replace emails like user@example.com with [EMAIL]"
              t={t}
            >
              <Toggle
                value={settings.mask_emails ?? true}
                onChange={(v) => updateSettings({ mask_emails: v })}
                accent={t.accent}
              />
            </Row>

            <Row label="Auto-mask phone numbers" hint="Replace phone numbers with [PHONE]" t={t}>
              <Toggle
                value={settings.mask_phones ?? true}
                onChange={(v) => updateSettings({ mask_phones: v })}
                accent={t.accent}
              />
            </Row>

            <Row
              label="Auto-mask card numbers"
              hint="Replace 16-digit credit card numbers with ****-****-****-****"
              t={t}
              last
            >
              <Toggle
                value={settings.mask_cards ?? true}
                onChange={(v) => updateSettings({ mask_cards: v })}
                accent={t.accent}
              />
            </Row>
          </>
        )}
      </Section>
    </div>
  );
}
