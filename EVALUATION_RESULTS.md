# ZeroClaw PinchBench Evaluation Report

**Date:** 2026-03-24 — 2026-03-26
**Benchmark:** PinchBench 23-task suite (adapted for ZeroClaw)
**Framework:** ZeroClaw v0.5.9 (scratch container, 26.3 MB)
**Models tested:** Gemini 3 Flash (3 runs), MiniMax M2.5 (7 runs across 3 configs), MiniMax M2.7 (1 run)
**Artifacts:** [eval/](eval/)

---

## Executive Summary

| | Gemini 3 Flash | MiniMax M2.5 (baseline) | MiniMax M2.5 (final) |
|--|----------------|------------------------|----------------------|
| **Score** | 66.0% ± 9.6 | 79.3% ± 2.6 | **81.8%** |
| Agent cost/run | $0.08 | $0.18 | $0.18 |
| Email Triage (was 0%) | 61% | 0% | **35-93%** |
| Search/Replace (was 0-58%) | 0% | 58% | **100%** |
| PDF Comprehension (was 0%) | 11% | 94% → 0% | **94%** |

Framework improvements fixed three structural failures: tool result truncation (per-tool aware — 8K default, 32K for PDF), web search trimming, and earlier compaction. MiniMax M2.5 remains the recommended agent model — 16 points higher than Gemini Flash at only 2.3x the agent cost.

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
| Max tool result chars | 8,000 default, 32,000 for pdf_read (PR 1) |
| Search snippet cap | 200 chars, 3,000 total (PR 2) |
| Compaction threshold | 30 messages, keep 12 recent (PR 3) |
| Features | `rag-pdf`, `image_gen` (OpenRouter backend) |

---

## Framework Improvements Applied

Four changes to ZeroClaw's core agent loop, validated by PinchBench before/after comparison:

| PR | Change | File | Impact |
|----|--------|------|--------|
| 1 | **Tool result truncation** — cap tool output at 8,000 chars, with per-tool override: `pdf_read` gets 32,000 chars | `src/agent/loop_.rs` | Email Triage: 0% → 93%, PDF Comprehension: 0% → 94% |
| 2 | **Web search trimming** — cap snippets at 200 chars, total output at 3,000 chars | `src/tools/web_search_tool.rs` | Reduces context waste on research tasks |
| 3 | **Earlier compaction** — trigger at 30 messages (was 50), keep 12 recent (was 20) | `src/agent/loop_.rs` | Search/Replace stabilized at 100% |
| ~~4~~ | ~~System prompt budget~~ — cap at 16K chars | ~~`src/channels/mod.rs`~~ | **Reverted** — caused regressions on tasks needing full bootstrap context |

Key insight: tool result truncation must be **tool-aware**. A flat 8K limit fixes multi-file overflow (email triage) but breaks single-document comprehension (PDF). The solution: small limits for tools that return many small results, large limits for tools that return one large document.

---

## Final Results: MiniMax M2.5 with Framework Fixes (Run 15 — best run)

| # | Task | Category | Score | Grading |
|---|------|----------|-------|---------|
| 00 | Sanity Check | basic | **100%** | automated |
| 01 | Calendar Event | calendar | **100%** | automated |
| 02 | Stock Research | research | **100%** | automated |
| 03 | Blog Post | writing | **80%** | llm_judge |
| 04 | Weather Script | coding | **100%** | automated |
| 05 | Doc Summary | comprehension | **85%** | llm_judge |
| 06 | Tech Conferences | research | **75%** | llm_judge |
| 07 | Email Drafting | writing | **95%** | llm_judge |
| 08 | Memory Retrieval | context | **80%** | automated |
| 09 | File Structure | file_ops | **100%** | automated |
| 10 | API Workflow | complex | **87%** | hybrid |
| 11 | Project Structure | file_ops | **100%** | automated |
| 12 | Search & Replace | file_ops | **100%** | automated |
| 13 | Image Generation | creative | **10%** | hybrid |
| 14 | Humanize Blog | transformation | **98%** | llm_judge |
| 15 | Daily Summary | synthesis | **88%** | llm_judge |
| 16a | Email Triage | organization | **35%** | hybrid |
| 17 | Email Search | comprehension | **98%** | hybrid |
| 16b | Market Research | research | **88%** | hybrid |
| 18 | Spreadsheet | data_analysis | **0%** | hybrid |
| 20 | PDF Summary | comprehension | **84%** | llm_judge |
| 21 | Report Comprehension | comprehension | **94%** | automated |
| 22 | Second Brain | memory | **85%** | hybrid |

**Overall: 81.8% (18.8 / 23.0)**

---

## By Category

| Category | Score | Tasks | Status |
|----------|-------|-------|--------|
| Basic | 100% | 1 | Solid |
| Calendar | 100% | 1 | Solid |
| Coding | 100% | 1 | Solid |
| File Ops | 100% | 3 | Solid |
| Content Transformation | 98% | 1 | Solid |
| Email Search | 98% | 1 | Solid |
| Writing | 88% | 2 | Solid |
| Synthesis | 88% | 1 | Solid |
| Research | 88% | 3 | Solid |
| Complex | 87% | 1 | Good |
| Memory | 85% | 1 | Good |
| Context | 80% | 1 | Good |
| Comprehension | 90% | 4 | Good (PDF fix: 0%→94%) |
| Organization | 35% | 1 | Fair (model variance) |
| Creative | 10% | 1 | Weak (grading format issue) |
| Data Analysis | 0% | 1 | Failed (XLSX, no Python in scratch) |

---

## Model Comparison: Gemini 3 Flash vs MiniMax M2.5 vs MiniMax M2.7

### Per-task

| # | Task | Gemini (3-run avg) | M2.5 (run 15, best) | M2.7 (run 14) |
|---|------|--------------------|---------------------|---------------|
| 00 | Sanity | 100% | 100% | 100% |
| 01 | Calendar | 100% | 100% | 100% |
| 02 | Stock | 33% | **100%** | **100%** |
| 03 | Blog | **92%** | 80% | 85% |
| 04 | Weather | 100% | 100% | 100% |
| 05 | Summary | 62% | **85%** | **86%** |
| 06 | Conferences | 30% | **75%** | **88%** |
| 07 | Email | 97% | 95% | **100%** |
| 08 | Memory | 80% | 80% | 70% |
| 09 | Files | 100% | 100% | 100% |
| 10 | Workflow | 78% | **87%** | 84% |
| 11 | Project | 100% | 100% | 100% |
| 12 | Search/Replace | 0% | **100%** | **100%** |
| 13 | Image Gen | 11% | 10% | **12%** |
| 14 | Humanize | 97% | **98%** | **98%** |
| 15 | Daily Summary | 63% | **88%** | 15% |
| 16a | Email Triage | **61%** | 35% | **92%** |
| 17 | Email Search | 32% | **98%** | 39% |
| 16b | Market Research | 81% | **88%** | 85% |
| 18 | Spreadsheet | **33%** | 0% | 0% |
| 20 | PDF Summary | 69% | **84%** | 71% |
| 21 | Comprehension | 11% | **94%** | 0% |
| 22 | Second Brain | 88% | 85% | 85% |

### Summary

| Metric | Gemini 3 Flash | MiniMax M2.5 | MiniMax M2.7 |
|--------|---------------|--------------|--------------|
| **Overall** | 66.0% ± 9.6 | **81.8%** | 74.4% |
| Agent cost/run | $0.08 | $0.18 | $0.22 |
| Input $/1M | $0.10 | $0.20 | $0.30 |
| Output $/1M | $0.40 | $1.17 | $1.20 |
| Tasks > 80% | 11 | **15** | 13 |
| Tasks at 0% | 5 | **1** | 3 |
| Best for | Budget | **Overall best** | Not recommended |

**MiniMax M2.5 is the clear winner** — highest score (81.8%), only 1 task at 0% (spreadsheet — no Python runtime), and cheaper than M2.7. M2.7 costs 50% more on input but scores 7 points lower. M2.7's "multi-agent collaboration" capabilities don't benefit ZeroClaw's single-turn webhook architecture.

---

## Latency

| Metric | Gemini 3 Flash | MiniMax M2.5 | MiniMax M2.7 |
|--------|---------------|--------------|--------------|
| Avg per task | 11.9s | 15.8s | 14.9s |
| Total (23 tasks) | 273s (4.5 min) | 363s (6.1 min) | 343s (5.7 min) |

---

## Cost

### Per-run breakdown

| Component | Gemini Flash | MiniMax M2.5 | MiniMax M2.7 |
|-----------|-------------|--------------|--------------|
| Agent (input + output) | ~$0.08 | ~$0.18 | ~$0.22 |
| Judge (Claude Sonnet) | ~$0.81 | ~$0.81 | ~$0.81 |
| Image gen | ~$0.04 | ~$0.04 | ~$0.04 |
| **Total per run** | **~$0.93** | **~$1.03** | **~$1.07** |

### Total evaluation spend

| Item | Cost |
|------|------|
| Gemini runs (6, 7, 8) | ~$2.80 |
| MiniMax M2.5 runs (9, 10, 11, 12, 13, 15) | ~$6.18 |
| MiniMax M2.7 run (14) | ~$1.07 |
| Debug runs (1-5) + manual tests | ~$4.60 |
| **Total OpenRouter spend** | **~$14.65** |

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
| Tool result truncation (8K default) | Email Triage: 0% → 35-93% across runs after fix |
| Per-tool truncation (32K for pdf_read) | PDF Comprehension: 0% → 94% — recovered without regressing email triage |
| Web search snippet trimming | Reduced context waste; no regressions |
| Earlier compaction (30 msgs) | Search/Replace stabilized at 100% |

### Framework change that didn't work

| Change | Evidence | Resolution |
|--------|----------|------------|
| System prompt cap (16K) | Variance jumped from 2.6 to 10.0 stddev; tasks that need full bootstrap context regressed | Reverted |
| Flat 8K truncation (no per-tool) | Fixed email triage but broke PDF comprehension (0%) | Replaced with per-tool limits |

### Remaining limitations

| Issue | Root cause | Fix path |
|-------|-----------|----------|
| Image Gen scores 10% | Grading function expects tool call metadata in transcript | Add tool call events to transcript format |
| Spreadsheet scores 0% | XLSX parsing not natively supported; no Python in scratch container | Add Python runtime or use Alpine-based image |
| Email Triage varies (35-93%) | Model sometimes reads files via shell (fails in scratch) vs file_read (works) | Model prompting or tool hint in system prompt |

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
| 13 | MiniMax M2.5 | PRs 1-3 | 79.3% |
| 14 | MiniMax M2.7 | PRs 1-3 | 74.4% |
| **15** | **MiniMax M2.5** | **PRs 1-3 + per-tool truncation** | **81.8%** |
