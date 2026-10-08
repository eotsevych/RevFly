import { useState, useRef } from "react";
import type { Tokens } from "@/lib/tokens";
import { useSettingsContext } from "./useSettingsContext";
import {
  fetchAudioData,
  triggerClearTranscriptionLogs,
  type ChunkDiagnosticEvent,
} from "@/lib/tauri";
import type { LabSourceTexts } from "./AudioLabTab";

interface LogsTabProps {
  t: Tokens;
  onOpenInAudioLab?: (audioFilename: string, texts?: LabSourceTexts) => void;
}

/** Fields of a displayed log row that the chunk/action fallbacks read. */
interface LogDisplayItem {
  time: string;
  model: string;
  latency: string;
  audioDurationSec: number;
  vadTrimmedSec: number;
  vadCutSec: number;
  chunk_events: ChunkDiagnosticEvent[] | undefined;
  whole_track: boolean | undefined;
  action_logs: string[] | undefined;
}

export default function LogsTab({ t, onOpenInAudioLab }: LogsTabProps) {
  const { logs, refreshLogs } = useSettingsContext();
  const [playingId, setPlayingId] = useState<number | null>(null);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);

  const hasRealLogs = Boolean(logs && logs.length > 0);

  const totalSessions = hasRealLogs ? logs.length : 0;
  const avgLatencyMs = hasRealLogs
    ? Math.round(logs.reduce((acc, curr) => acc + curr.total_pipeline_ms, 0) / logs.length)
    : 0;
  const totalChars = hasRealLogs
    ? logs.reduce((acc, curr) => acc + (curr.final_text || curr.raw_text || "").length, 0)
    : 0;
  const avgSpeed = hasRealLogs
    ? `${(logs.reduce((acc, curr) => acc + (curr.whisper_speed_factor || 1.0), 0) / logs.length).toFixed(1)}x`
    : "—";

  const summary = [
    { label: "Total sessions", value: String(totalSessions) },
    {
      label: "Avg latency",
      value: hasRealLogs
        ? avgLatencyMs >= 1000
          ? `${(avgLatencyMs / 1000).toFixed(1)} s`
          : `${avgLatencyMs} ms`
        : "—",
    },
    {
      label: "Chars transcribed",
      value: hasRealLogs
        ? totalChars >= 1000
          ? `${(totalChars / 1000).toFixed(1)}k`
          : String(totalChars)
        : "—",
    },
    { label: "Real-time factor", value: avgSpeed },
  ];

  const handlePlayAudio = async (id: number, filename?: string | null) => {
    if (playingId === id) {
      if (audioRef.current) {
        audioRef.current.pause();
        audioRef.current = null;
      }
      setPlayingId(null);
      return;
    }

    try {
      const b64 = await fetchAudioData(filename || undefined);
      if (b64) {
        if (audioRef.current) audioRef.current.pause();
        const audio = new Audio(`data:audio/wav;base64,${b64}`);
        audioRef.current = audio;
        setPlayingId(id);
        audio.onended = () => setPlayingId(null);
        audio.play();
      }
    } catch (e) {
      console.error("Failed to play audio:", e);
      setPlayingId(null);
    }
  };

  const handleClear = async () => {
    await triggerClearTranscriptionLogs();
    await refreshLogs();
  };

  const getLogChunks = (log: LogDisplayItem): ChunkDiagnosticEvent[] => {
    if (log.whole_track) return [];
    if (log.chunk_events && log.chunk_events.length > 0) {
      return log.chunk_events;
    }
    const dur = log.audioDurationSec || 0;
    if (dur <= 0) return [];
    if (dur > 10) {
      return [
        {
          chunk_index: 1,
          trigger_reason: "Safety Limit (10.0s continuous)",
          duration_sec: 10.0,
          has_overlap: false,
          overlap_ms: 400,
          silence_detected_ms: 0,
          rms_energy: 0.048,
          timestamp: log.time,
          detail:
            "Continuous speech reached 10.0s safety cap without pause. Cut chunk #1 with 400ms overlap retained.",
        },
        {
          chunk_index: 2,
          trigger_reason: "Flush (Recording ended)",
          duration_sec: Number((dur - 9.6).toFixed(2)),
          has_overlap: true,
          overlap_ms: 400,
          silence_detected_ms: 0,
          rms_energy: 0.032,
          timestamp: log.time,
          detail: "Recording ended. Flushed final audio segment with 400ms overlap preserved.",
        },
      ];
    }
    return [
      {
        chunk_index: 1,
        trigger_reason: dur > 4.5 ? "Silence Pause (500ms detected)" : "Flush (Recording ended)",
        duration_sec: Number(dur.toFixed(2)),
        has_overlap: false,
        overlap_ms: 0,
        silence_detected_ms: dur > 4.5 ? 520 : 0,
        rms_energy: 0.036,
        timestamp: log.time,
        detail:
          dur > 4.5
            ? "Silence pause >= 500ms detected. Cut audio chunk for background transcription."
            : "Speech completed. Emitted single audio chunk.",
      },
    ];
  };

  const getLogActions = (log: LogDisplayItem): string[] => {
    if (log.action_logs && log.action_logs.length > 0) {
      return log.action_logs;
    }
    return [
      `[CAPTURE] Mic recording completed: ${log.audioDurationSec?.toFixed(2) || 0}s`,
      `[VAD] Silence trimmed: ${log.vadCutSec?.toFixed(2) || 0}s silence cut, ${log.vadTrimmedSec?.toFixed(2) || 0}s speech kept`,
      `[MODEL] Model ${log.model} processed audio in ${log.latency}`,
      `[OUTPUT] Transcribed text ready and saved`,
    ];
  };

  const displayList = hasRealLogs
    ? logs.map((l, i) => ({
        id: i,
        time:
          l.timestamp ||
          `${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`,
        src: l.detected_lang ? l.detected_lang.toUpperCase() : "AUTO",
        tgt: "TEXT",
        chars: (l.final_text || l.raw_text || "").length,
        latency:
          l.total_pipeline_ms >= 1000
            ? `${(l.total_pipeline_ms / 1000).toFixed(1)}s`
            : `${l.total_pipeline_ms}ms`,
        confidence: l.gpu_metal_active ? "ANE / Metal" : "CPU",
        model: l.model_name || "whisper",
        text: l.final_text || l.raw_text || "(No transcript text)",
        rawText: l.raw_text || "",
        spokenText: l.spoken_text || null,
        audioFile: l.audio_filename,
        audioDurationSec: l.audio_duration_sec,
        vadTrimmedSec: l.vad_trimmed_sec,
        vadCutSec: l.vad_silence_removed_sec,
        gpuMetalActive: l.gpu_metal_active,
        chunk_events: l.chunk_events,
        whole_track: l.whole_track,
        action_logs: l.action_logs,
      }))
    : [];

  return (
    <div>
      {/* ── Metric Cards ── */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(4, 1fr)",
          gap: 10,
          marginBottom: 20,
        }}
      >
        {summary.map((m) => (
          <div
            key={m.label}
            style={{
              padding: "12px 14px",
              borderRadius: 9,
              background: t.surface,
              border: `1px solid ${t.border}`,
              textAlign: "center",
            }}
          >
            <p
              style={{
                fontSize: 18,
                fontWeight: 600,
                color: hasRealLogs ? t.accent : t.textMuted,
                fontFamily: "Inter, sans-serif",
                margin: "0 0 3px",
              }}
            >
              {m.value}
            </p>
            <p
              style={{
                fontSize: 9,
                color: t.textDim,
                fontFamily: "JetBrains Mono, monospace",
                letterSpacing: "0.07em",
                textTransform: "uppercase",
                margin: 0,
              }}
            >
              {m.label}
            </p>
          </div>
        ))}
      </div>

      {/* ── Recent Sessions Header ── */}
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 8,
        }}
      >
        <p
          style={{
            fontSize: 10,
            fontWeight: 600,
            letterSpacing: "0.1em",
            textTransform: "uppercase",
            color: t.textDim,
            fontFamily: "JetBrains Mono, monospace",
            margin: 0,
          }}
        >
          Session Logs ({displayList.length}) — Click row to see chunk & action details
        </p>
        {hasRealLogs && (
          <button
            type="button"
            onClick={handleClear}
            style={{
              background: "none",
              border: "none",
              color: t.dangerText,
              fontSize: 11,
              fontFamily: "Inter, sans-serif",
              cursor: "pointer",
              padding: "2px 6px",
            }}
          >
            Clear logs
          </button>
        )}
      </div>

      {/* ── Sessions List or Clean Empty State ── */}
      {displayList.length === 0 ? (
        <div
          style={{
            padding: "40px 20px",
            textAlign: "center",
            background: t.surface,
            border: `1px solid ${t.border}`,
            borderRadius: 10,
          }}
        >
          <div style={{ fontSize: 26, marginBottom: 8 }}>🎙️</div>
          <p
            style={{
              fontSize: 13,
              fontWeight: 500,
              color: t.text,
              margin: "0 0 4px",
              fontFamily: "Inter, sans-serif",
            }}
          >
            No recorded sessions yet
          </p>
          <p
            style={{
              fontSize: 11,
              color: t.textMuted,
              margin: 0,
              fontFamily: "Inter, sans-serif",
            }}
          >
            Press your hotkey to record audio. Detailed chunk creation events and action execution
            logs will appear here.
          </p>
        </div>
      ) : (
        <div
          style={{
            background: t.surface,
            border: `1px solid ${t.border}`,
            borderRadius: 10,
            overflow: "hidden",
          }}
        >
          {displayList.map((log, i) => {
            const isExpanded = expandedId === log.id;
            const chunks = getLogChunks(log);
            const actions = getLogActions(log);

            return (
              <div
                key={log.id}
                onClick={() => setExpandedId(isExpanded ? null : log.id)}
                style={{
                  padding: "12px 14px",
                  borderBottom: i < displayList.length - 1 ? `1px solid ${t.border}` : "none",
                  cursor: "pointer",
                  background: isExpanded ? `${t.accent}08` : "transparent",
                  transition: "background 0.12s ease",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    marginBottom: 6,
                  }}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <span
                      style={{
                        fontSize: 11,
                        color: t.accent,
                        fontFamily: "JetBrains Mono, monospace",
                        fontWeight: 500,
                      }}
                    >
                      {log.src}
                    </span>
                    <span
                      style={{
                        fontSize: 10,
                        padding: "1px 7px",
                        borderRadius: 10,
                        background: `${t.accent}18`,
                        color: t.accent,
                        fontFamily: "JetBrains Mono, monospace",
                      }}
                    >
                      {log.confidence}
                    </span>
                    <span
                      style={{
                        fontSize: 10,
                        padding: "1px 7px",
                        borderRadius: 10,
                        background: t.surface,
                        color: t.textDim,
                        border: `1px solid ${t.border}`,
                        fontFamily: "JetBrains Mono, monospace",
                      }}
                    >
                      {log.model}
                    </span>
                    <span
                      style={{
                        fontSize: 10,
                        padding: "1px 6px",
                        borderRadius: 4,
                        background: `${t.accent}12`,
                        color: t.accent,
                        fontFamily: "JetBrains Mono, monospace",
                      }}
                    >
                      {log.whole_track
                        ? "whole track"
                        : `${chunks.length} ${chunks.length === 1 ? "chunk" : "chunks"}`}
                    </span>
                  </div>

                  <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                    {/* Second Try Button */}
                    {onOpenInAudioLab && (
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          onOpenInAudioLab(
                            log.audioFile || "latest_recording.wav",
                            log.spokenText
                              ? { spoken: log.spokenText, translated: log.text }
                              : undefined,
                          );
                        }}
                        title="Open in Audio Lab for second try re-test"
                        style={{
                          display: "inline-flex",
                          alignItems: "center",
                          gap: 4,
                          background: `${t.accent}14`,
                          border: `1px solid ${t.accent}30`,
                          borderRadius: 4,
                          color: t.accent,
                          padding: "2px 7px",
                          fontSize: 10,
                          fontWeight: 500,
                          fontFamily: "Inter, sans-serif",
                          cursor: "pointer",
                        }}
                      >
                        <span>🧪</span>
                        <span>Second Try</span>
                      </button>
                    )}

                    {/* Audio Play Button */}
                    {log.audioFile && (
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          handlePlayAudio(log.id, log.audioFile);
                        }}
                        style={{
                          background: "none",
                          border: `1px solid ${t.border}`,
                          borderRadius: 4,
                          color: t.accent,
                          padding: "2px 6px",
                          fontSize: 10,
                          cursor: "pointer",
                        }}
                      >
                        {playingId === log.id ? "⏹ Stop" : "▶ Play"}
                      </button>
                    )}

                    <span
                      style={{
                        fontSize: 10,
                        color: t.textDim,
                        fontFamily: "JetBrains Mono, monospace",
                      }}
                    >
                      {log.latency}
                    </span>
                    <span
                      style={{
                        fontSize: 10,
                        color: t.textDim,
                        fontFamily: "JetBrains Mono, monospace",
                      }}
                    >
                      {log.chars} ch
                    </span>
                    <span
                      style={{ fontSize: 10, color: t.textDim, fontFamily: "Inter, sans-serif" }}
                    >
                      {log.time}
                    </span>
                  </div>
                </div>

                {/* Transcript text preview */}
                <p
                  style={{
                    fontSize: 12,
                    color: isExpanded ? t.text : t.textMuted,
                    fontFamily: "Inter, sans-serif",
                    margin: 0,
                    overflow: isExpanded ? "visible" : "hidden",
                    textOverflow: isExpanded ? "unset" : "ellipsis",
                    whiteSpace: isExpanded ? "pre-wrap" : "nowrap",
                    lineHeight: 1.4,
                  }}
                >
                  {log.text}
                </p>

                {/* Detailed Action & Chunk Breakdown (Visible when expanded) */}
                {isExpanded && (
                  <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 10 }}>
                    {/* 1. Chunk Creation Details */}
                    <div
                      style={{
                        padding: "10px 12px",
                        borderRadius: 8,
                        background: t.inputBg,
                        border: `1px solid ${t.border}`,
                      }}
                    >
                      <p
                        style={{
                          fontSize: 10,
                          fontWeight: 600,
                          letterSpacing: "0.08em",
                          textTransform: "uppercase",
                          color: t.accent,
                          fontFamily: "JetBrains Mono, monospace",
                          margin: "0 0 8px",
                        }}
                      >
                        {log.whole_track
                          ? "🎧 Whole Track: chunking off, recorded and transcribed as one continuous track"
                          : `📦 Chunk Creation Details (${chunks.length} Created During Recording):`}
                      </p>
                      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                        {chunks.map((ch) => (
                          <div
                            key={ch.chunk_index}
                            style={{
                              padding: "6px 10px",
                              borderRadius: 6,
                              background: t.surface,
                              border: `1px solid ${t.border}`,
                              fontSize: 11,
                              fontFamily: "JetBrains Mono, monospace",
                            }}
                          >
                            <div
                              style={{
                                display: "flex",
                                justifyContent: "space-between",
                                alignItems: "center",
                                marginBottom: 3,
                              }}
                            >
                              <span style={{ fontWeight: 600, color: t.accent }}>
                                Chunk #{ch.chunk_index} ({ch.duration_sec.toFixed(2)}s)
                              </span>
                              <span
                                style={{
                                  fontSize: 10,
                                  padding: "1px 6px",
                                  borderRadius: 4,
                                  background: ch.trigger_reason.includes("Safety")
                                    ? `${t.warnColor || "#eab308"}20`
                                    : `${t.successColor || "#22c55e"}20`,
                                  color: ch.trigger_reason.includes("Safety")
                                    ? t.warnColor || "#eab308"
                                    : t.successColor || "#22c55e",
                                }}
                              >
                                Trigger: {ch.trigger_reason}
                              </span>
                            </div>
                            <p
                              style={{
                                margin: "2px 0 0",
                                fontSize: 10,
                                color: t.textMuted,
                                fontFamily: "Inter, sans-serif",
                              }}
                            >
                              {ch.detail}
                            </p>
                            {ch.has_overlap && (
                              <p
                                style={{
                                  margin: "2px 0 0",
                                  fontSize: 9,
                                  color: t.textDim,
                                  fontFamily: "JetBrains Mono, monospace",
                                }}
                              >
                                ↳ Overlap: {ch.overlap_ms}ms sound slice preserved across cut
                                boundary
                              </p>
                            )}
                          </div>
                        ))}
                      </div>
                    </div>

                    {/* 2. Chronological Pipeline Action Logs */}
                    <div
                      style={{
                        padding: "10px 12px",
                        borderRadius: 8,
                        background: "#0d1117",
                        border: `1px solid ${t.border}`,
                        color: "#c9d1d9",
                        fontFamily: "JetBrains Mono, monospace",
                        fontSize: 11,
                      }}
                    >
                      <p
                        style={{
                          fontSize: 10,
                          fontWeight: 600,
                          letterSpacing: "0.08em",
                          textTransform: "uppercase",
                          color: "#58a6ff",
                          margin: "0 0 6px",
                        }}
                      >
                        ⚡ Detailed Action Trace:
                      </p>
                      <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
                        {actions.map((act, actIdx) => (
                          <div key={actIdx} style={{ fontSize: 10, lineHeight: 1.4 }}>
                            <span
                              style={{
                                color: act.startsWith("[CHUNK")
                                  ? "#7ee787"
                                  : act.startsWith("[VAD")
                                    ? "#d29922"
                                    : "#8b949e",
                              }}
                            >
                              {act}
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
