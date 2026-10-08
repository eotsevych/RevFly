---
name: perf-analyze
description: Analyze RevFly's performance log (performance.jsonl) for slowdowns, memory growth and their causes. Use when the user runs /perf-analyze or asks to analyze RevFly performance, dictation speed, CPU/RAM usage or performance degradation.
---

# Analyze RevFly performance

RevFly writes one JSON line per dictation (`"type": "dictation"`) and one every 5 minutes
(`"type": "heartbeat"`) to `~/Library/Application Support/revfly/logs/performance.jsonl`
(rotated to `performance.1.jsonl` at 10 MB). The writer is `src-tauri/src/perf.rs`; the stage laps
are set in `stop_listening_and_process` in `src-tauri/src/app_controller.rs`.

Arguments (optional): a time window such as `today`, `last 3 days`, `since 2026-10-07`, or a path
to a different log. Default: all data, with extra attention to the last 24 hours.

## Steps

1. Run the summary report:
   ```bash
   python3 scripts/perf_report.py
   ```
   (Pass a path as the first argument for a different log.) If it says there are no dictations,
   tell the user and stop.

2. Dig into the raw lines for anything the report flags. Use `python3 -c` or `jq` over the JSONL
   rather than reading the whole file. Useful cuts:
   - speed_factor over time (by hour or by dictation number) to see a trend, not just day medians
   - the slowest 10 dictations with every stage, `usage` and `context`
   - non-`ok` outcomes and the stage they ended in (the latest stage present, in the pipeline
     order below; the keys in `stages_ms` are written alphabetically, not in order). Since 0.1.10
     an early-ending run files its unfinished time under that stage and stops its clock there, so
     the time an error stays on the pill isn't counted. Older runs lack that last stage.
   - heartbeats: `app_rss_mb` against `app_uptime_sec` (memory growth while idle), and app restarts
     (`app_uptime_sec` dropping back near 0)
   - `inference_ms` against `speech_sec` (inference should scale roughly linearly with speech)

3. For specific slow runs, cross-check `transcription_pipeline.log` in the same folder around the
   same timestamp (`[PERF]`, `[MODEL_LOAD]`, `[IDLE_CLEANUP]`, `[VAD]`, `[PARAKEET_DONE]` lines).

## How to read the fields

- `stages_ms`: laps, in pipeline order: `stop_capture` (recorder flush + denoise), `audio_prep` (leveling +
  WAV), `vad`, `save_audio`, `model_ready`, `inference`, `post_process` (cleanup + language
  detection), `translation`, `paste`, `history_db`, `finalize`. Their sum is `total_ms`.
- `speed_factor`: seconds of speech per second of inference. Higher is better. A falling trend at
  similar `speech_sec` is the clearest sign of real degradation.
- `model_wait_ms`: part of the `inference` lap spent waiting for the model to load. Large values
  mean the model wasn't ready (expected with `model_load_mode: on_stop`, or right after
  `IDLE_CLEANUP` freed it).
- `usage`: sampled every 250 ms while processing. CPU is % of one core (400 = four cores busy).
  `available_mem_min_mb` low or `swap_used_peak_mb` rising means memory pressure. On macOS
  `available_mem_min_mb` comes from the kernel's memory-pressure level; before 0.1.10 it was
  always 0 there, so ignore it in older lines.
- `context`: `thermal_state` (nominal/fair/serious/critical), `low_power_mode`, `load_avg_1m`
  (other apps competing for CPU), `app_uptime_sec`, `dictations_since_launch`.

## What to look for

- **Degradation with uptime or dictation count** (speed falls or RAM grows the longer the app
  runs, recovering after a restart) → a leak or growing state in the app. Name the stage.
- **Slow runs explained by the machine**: `low_power_mode: true`, thermal `serious`/`critical`,
  high `load_avg_1m` or system CPU, low free RAM / swap growth. Say so instead of blaming RevFly.
- **One stage dominating**: e.g. `vad` in seconds (it should be ~100 ms per 10 s of audio),
  `history_db` > 500 ms (DB size or disk), `paste` > 200 ms, `stop_capture` > 300 ms (denoise).
- **Model load cost**: `model_wait_ms` and `MODEL_LOAD` times; compare `on_start` vs `on_stop`
  runs for both latency and `app_rss_peak_mb`.
- **Failures/cancellations clustering** at a stage or time.

## Report back

Lead with the answer: is there degradation, how big, and the most likely cause, backed by numbers
(medians and counts, with the time range and n). Then list notable findings in order of impact,
each with the evidence and a concrete next step (a code location to look at, a setting to change,
or more data to collect). Say clearly when there's too little data to conclude something. Don't
paste the whole report table; quote only the rows that matter.
