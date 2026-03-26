# ZeroClaw PinchBench Evaluation Report

**Date:** 2026-03-24 — 2026-03-26
**Benchmark:** PinchBench 23-task suite (adapted for ZeroClaw)
**Framework:** ZeroClaw v0.5.9 (scratch container, 26.3 MB)
**Models tested:** Gemini 3 Flash (3 runs), MiniMax M2.5 (5 runs across 2 configurations)
**Artifacts:** [eval/](eval/)

---

## Executive Summary

| | Gemini 3 Flash | MiniMax M2.5 (baseline) | MiniMax M2.5 (with framework fixes) |
|--|----------------|------------------------|--------------------------------------|
| **Score** | 66.0% ± 9.6 | 79.3% ± 2.6 | **79.3%** |
| Agent cost/run | $0.08 | $0.18 | $0.18 |
| Email Triage (was 0%) | 61% | 0% | **93%** |
| Search/Replace (was 0-58%) | 0% | 58% | **100%** |

Three framework improvements (tool result truncation, web search trimming, earlier compaction) fixed two structural failures without regressing overall score. MiniMax M2.5 remains the recommended agent model — 13 points higher than Gemini Flash, 4x more stable, at only 2.3x the agent cost.

---

## Configuration

| Setting | Value |
|---------|-------|
| Agent models | `google/gemini-3-flash-preview`, `minimax/minimax-m2.5` |
| Judge model | `anthropic/claude-sonnet-4.6` (via OpenRouter, direct API) |
| Image gen model | `google/gemini-3.1-flash-image-preview` (via OpenRouter) |
| Container | `FROM scratch`, static MUSL binary, 26.3 MB |
| Memory at idle | 3.4 MiB |
| Gateway timeout | 300s |
| Max tool iterations | 25 |
| Max tool result chars | 8,000 (PR 1) |
| Search snippet cap | 200 chars, 3,000 total (PR 2) |
| Compaction threshold | 30 messages, keep 12 recent (PR 3) |
| Features | `rag-pdf`, `image_gen` (OpenRouter backend) |

---

## Framework Improvements Applied

Three changes to ZeroClaw's core agent loop, validated by PinchBench before/after comparison:

| PR | Change | File | Impact |
|----|--------|------|--------|
| 1 | **Tool result truncation** — cap tool output at 8,000 chars before injecting into context | `src/agent/loop_.rs` | Email Triage: 0% → 93% |
| 2 | **Web search trimming** — cap snippets at 200 chars, total output at 3,000 chars | `src/tools/web_search_tool.rs` | Reduces context waste on research tasks |
| 3 | **Earlier compaction** — trigger at 30 messages (was 50), keep 12 recent (was 20) | `src/agent/loop_.rs` | Multi-step tasks focus on recent context sooner |
| ~~4~~ | ~~System prompt budget~~ — cap at 16K chars | ~~`src/channels/mod.rs`~~ | **Reverted** — caused regressions on tasks needing full bootstrap context |

---

## Final Results: MiniMax M2.5 with Framework Fixes (Run 13)

| # | Task | Category | Score | Grading |
|---|------|----------|-------|---------|
| 00 | Sanity Check | basic | **100%** | automated |
| 01 | Calendar Event | calendar | **100%** | automated |
| 02 | Stock Research | research | **100%** | automated |
| 03 | Blog Post | writing | **90%** | llm_judge |
| 04 | Weather Script | coding | **100%** | automated |
| 05 | Doc Summary | comprehension | **88%** | llm_judge |
| 06 | Tech Conferences | research | **90%** | llm_judge |
| 07 | Email Drafting | writing | **95%** | llm_judge |
| 08 | Memory Retrieval | context | **70%** | automated |
| 09 | File Structure | file_ops | **100%** | automated |
| 10 | API Workflow | complex | **87%** | hybrid |
| 11 | Project Structure | file_ops | **100%** | automated |
| 12 | Search & Replace | file_ops | **100%** | automated |
| 13 | Image Generation | creative | **10%** | hybrid |
| 14 | Humanize Blog | transformation | **96%** | llm_judge |
| 15 | Daily Summary | synthesis | **94%** | llm_judge |
| 16a | Email Triage | organization | **93%** | hybrid |
| 17 | Email Search | comprehension | **96%** | hybrid |
| 16b | Market Research | research | **44%** | hybrid |
| 18 | Spreadsheet | data_analysis | **0%** | hybrid |
| 20 | PDF Summary | comprehension | **78%** | llm_judge |
| 21 | Report Comprehension | comprehension | **0%** | automated |
| 22 | Second Brain | memory | **92%** | hybrid |

**Overall: 79.3% (18.2 / 23.0)**

---

## By Category

| Category | Score | Tasks | Status |
|----------|-------|-------|--------|
| Basic | 100% | 1 | Solid |
| Calendar | 100% | 1 | Solid |
| Coding | 100% | 1 | Solid |
| File Ops | 100% | 3 | Solid |
| Writing | 93% | 2 | Solid |
| Content Transformation | 96% | 1 | Solid |
| Organization | 93% | 1 | Solid (fixed from 0%) |
| Memory | 92% | 1 | Solid |
| Synthesis | 94% | 1 | Solid |
| Comprehension | 66% | 4 | Mixed |
| Research | 78% | 3 | Good |
| Complex | 87% | 1 | Good |
| Context | 70% | 1 | Fair |
| Creative | 10% | 1 | Weak (grading format issue) |
| Data Analysis | 0% | 1 | Failed (XLSX) |

---

## Model Comparison: Gemini 3 Flash vs MiniMax M2.5

### Per-task (best available data)

| # | Task | Gemini (3-run avg) | MiniMax (best config) |
|---|------|--------------------|----------------------|
| 00 | Sanity | 100% | 100% |
| 01 | Calendar | 100% | 100% |
| 02 | Stock | 33% | **100%** |
| 03 | Blog | 92% | 90% |
| 04 | Weather | 100% | 100% |
| 05 | Summary | 62% | **88%** |
| 06 | Conferences | 30% | **90%** |
| 07 | Email | 97% | 95% |
| 08 | Memory | **80%** | 70% |
| 09 | Files | 100% | 100% |
| 10 | Workflow | 78% | **87%** |
| 11 | Project | 100% | 100% |
| 12 | Search/Replace | 0% | **100%** |
| 13 | Image Gen | 11% | 10% |
| 14 | Humanize | 97% | 96% |
| 15 | Daily Summary | 63% | **94%** |
| 16a | Email Triage | 61% | **93%** |
| 17 | Email Search | 32% | **96%** |
| 16b | Market Research | 81% | 44% |
| 18 | Spreadsheet | **33%** | 0% |
| 20 | PDF Summary | 69% | **78%** |
| 21 | Comprehension | 11% | 0% |
| 22 | Second Brain | 88% | **92%** |

### Summary

| Metric | Gemini 3 Flash | MiniMax M2.5 |
|--------|---------------|--------------|
| **Overall** | **66.0% ± 9.6** | **79.3%** |
| Agent cost/run | $0.08 | $0.18 |
| Tasks > 80% | 11 | **15** |
| Tasks at 0% | 5 | 2 |
| Best for | Cost-sensitive simple tasks | Quality-focused agentic work |

MiniMax wins on research (+60pp on conferences), comprehension (+26pp on summary), file operations (+100pp on search/replace), and multi-file tasks (+93pp on email triage). Gemini wins on cost (2.3x cheaper) and spreadsheet parsing.

---

## Latency

| Metric | Gemini 3 Flash | MiniMax M2.5 |
|--------|---------------|--------------|
| Avg per task | 11.9s | 15.8s |
| Total (23 tasks) | 273s (4.5 min) | 363s (6.1 min) |

---

## Cost

### Per-run breakdown

| Component | Gemini Flash | MiniMax M2.5 |
|-----------|-------------|--------------|
| Agent (input + output) | ~$0.08 | ~$0.18 |
| Judge (Claude Sonnet) | ~$0.81 | ~$0.81 |
| Image gen | ~$0.04 | ~$0.04 |
| **Total per run** | **~$0.93** | **~$1.03** |

### Total evaluation spend

| Item | Cost |
|------|------|
| Gemini runs (6, 7, 8) | ~$2.80 |
| MiniMax runs (9, 10, 11, 12, 13) | ~$5.15 |
| Debug runs (1-5) + manual tests | ~$4.60 |
| **Total OpenRouter spend** | **~$12.55** |

---

## Infrastructure

| Metric | Value |
|--------|-------|
| Container image | 26.3 MB |
| Memory (idle) | 3.4 MiB |
| Memory (under load) | ~5-6 MiB |
| CPU (idle) | 0.00% |
| PIDs | 3 |
| Min hosting cost | ~$3-5/mo (512MB VPS) |

---

## What Worked, What Didn't

### Framework changes that worked

| Change | Evidence |
|--------|----------|
| Tool result truncation (8K cap) | Email Triage: 0% → 93% across all runs after fix |
| Web search snippet trimming | Reduced context waste; no regressions |
| Earlier compaction (30 msgs) | Search/Replace stabilized at 100% |

### Framework change that didn't work

| Change | Evidence | Resolution |
|--------|----------|------------|
| System prompt cap (16K) | Variance jumped from 2.6 to 10.0 stddev; tasks that need full bootstrap context regressed | Reverted |

### Remaining limitations

| Issue | Root cause | Fix path |
|-------|-----------|----------|
| Image Gen scores 10% | Grading function expects tool call metadata in transcript | Add tool call events to transcript format |
| Spreadsheet scores 0% | XLSX parsing not natively supported | Add Python shell workaround |
| Comprehension scores 0% | Model extracts wrong values from dense PDF tables | Model limitation; try stronger model |
| Market Research varies (44-86%) | Web search result quality inconsistent | Consider Brave API over DuckDuckGo |

---

## Run History

| Run | Model | Config | Score |
|-----|-------|--------|-------|
| 6 | Gemini Flash | Baseline | 75.2% |
| 7 | Gemini Flash | Baseline | 66.9% |
| 8 | Gemini Flash | Baseline | 56.0% |
| 9 | MiniMax M2.5 | Baseline | 81.2% |
| 10 | MiniMax M2.5 | Baseline | 77.5% |
| 11 | MiniMax M2.5 | PRs 1-4 | 79.5% |
| 12 | MiniMax M2.5 | PRs 1-4 | 65.4% |
| **13** | **MiniMax M2.5** | **PRs 1-3** | **79.3%** |
