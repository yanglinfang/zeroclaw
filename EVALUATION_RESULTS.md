# ZeroClaw PinchBench Evaluation Report

**Date:** 2026-03-24 / 2026-03-25
**Benchmark:** PinchBench 23-task suite (adapted for ZeroClaw)
**Framework:** ZeroClaw v0.5.9 (scratch container, 26.3 MB)
**Models tested:** Gemini 3 Flash (3 runs), MiniMax M2.5 (2 runs)
**Artifacts:** [eval/](eval/)

---

## Executive Summary

| | Gemini 3 Flash | MiniMax M2.5 |
|--|----------------|--------------|
| **Score** | **66.0% ± 9.6** | **79.3% ± 2.6** |
| Agent cost/run | $0.08 | $0.18 |
| Variance | High (9.6) | **Low (2.6)** |
| Score per $1 | 71% | **77%** |

MiniMax M2.5 scores 13 points higher, with 4x less variance, at 2.3x the agent cost. For agentic workloads through ZeroClaw, MiniMax M2.5 is the better model — more accurate, more stable, and better value per dollar.

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
| Features | `rag-pdf`, `image_gen` (OpenRouter backend) |

---

## Per-Task Comparison

| # | Task | Category | Gemini (3-run avg) | MiniMax (2-run avg) | Delta |
|---|------|----------|--------------------|---------------------|-------|
| 00 | Sanity Check | basic | 100.0% | 100.0% | — |
| 01 | Calendar Event | calendar | 100.0% | 100.0% | — |
| 02 | Stock Research | research | 33.3% | **100.0%** | +66.7 |
| 03 | Blog Post | writing | **91.7%** | 87.5% | -4.2 |
| 04 | Weather Script | coding | 100.0% | 100.0% | — |
| 05 | Doc Summary | comprehension | 61.7% | **95.0%** | +33.3 |
| 06 | Tech Conferences | research | 30.0% | **83.5%** | +53.5 |
| 07 | Email Drafting | writing | 96.7% | 97.5% | — |
| 08 | Memory Retrieval | context | **80.0%** | 75.0% | -5.0 |
| 09 | File Structure | file_ops | 100.0% | 100.0% | — |
| 10 | API Workflow | complex | 77.7% | **90.0%** | +12.3 |
| 11 | Project Structure | file_ops | 100.0% | 100.0% | — |
| 12 | Search & Replace | file_ops | 0.0% | **58.5%** | +58.5 |
| 13 | Image Generation | creative | 11.3% | 14.5% | +3.2 |
| 14 | Humanize Blog | transformation | 97.3% | 95.5% | — |
| 15 | Daily Summary | synthesis | 63.3% | **89.5%** | +26.2 |
| 16a | Email Triage | organization | **61.3%** | 0.0% | -61.3 |
| 17 | Email Search | comprehension | 32.3% | **98.5%** | +66.2 |
| 16b | Market Research | research | 81.0% | 83.0% | — |
| 18 | Spreadsheet | data_analysis | **33.3%** | 1.0% | -32.3 |
| 20 | PDF Summary | comprehension | 69.3% | **77.0%** | +7.7 |
| 21 | Report Comprehension | comprehension | 11.0% | **94.0%** | +83.0 |
| 22 | Second Brain | memory | 88.3% | 85.0% | — |

---

## Where MiniMax Wins Big (>20pp improvement)

| Task | Gemini | MiniMax | Root cause |
|------|--------|---------|-----------|
| Report Comprehension | 11% | **94%** | MiniMax far better at extracting structured data from PDF tables |
| Stock Research | 33% | **100%** | More reliable web search result parsing |
| Email Search | 32% | **99%** | Consistent multi-file reading and synthesis |
| Search & Replace | 0% | **59%** | MiniMax correctly applies file edits; Gemini never succeeds |
| Tech Conferences | 30% | **84%** | Better web search + no HTTP 500s |
| Doc Summary | 62% | **95%** | No intermittent failures |
| Daily Summary | 63% | **90%** | Reliably finds and reads research files |

## Where Gemini Wins

| Task | Gemini | MiniMax | Root cause |
|------|--------|---------|-----------|
| Email Triage | 61% | **0%** | MiniMax hits HTTP 500 on 13-file context (context overflow) |
| Spreadsheet | 33% | **1%** | Neither handles XLSX well; Gemini gets partial CSV credit |

---

## Stability Comparison

| Metric | Gemini 3 Flash | MiniMax M2.5 |
|--------|---------------|--------------|
| Run scores | 75.2%, 66.9%, 56.0% | 81.2%, 77.5% |
| **Mean** | **66.0%** | **79.3%** |
| **StdDev** | **9.6** | **2.6** |
| Tasks at 0% (worst run) | 7 | 2 |
| Tasks at 100% (any run) | 6 | 9 |

MiniMax is dramatically more stable — stddev of 2.6 vs 9.6. The high-variance tasks that swing 0-95% on Gemini (stock, conferences, daily summary, email search) are consistently high on MiniMax.

---

## Category Comparison

| Category | Gemini | MiniMax | Winner |
|----------|--------|---------|--------|
| Basic | 100% | 100% | Tie |
| Calendar | 100% | 100% | Tie |
| Coding | 100% | 100% | Tie |
| File Ops | 67% | **86%** | MiniMax |
| Writing | 94% | 93% | Tie |
| Content Transformation | 97% | 96% | Tie |
| Research | 48% | **89%** | MiniMax |
| Comprehension | 43% | **91%** | MiniMax |
| Complex | 78% | **90%** | MiniMax |
| Synthesis | 63% | **90%** | MiniMax |
| Memory | 88% | 85% | Tie |
| Context | 80% | 75% | Tie |
| Creative | 11% | 15% | Tie (both weak — grading format issue) |
| Organization | 61% | 0% | Gemini |
| Data Analysis | 33% | 1% | Gemini |

MiniMax wins 6 categories, Gemini wins 2, 7 ties. MiniMax's advantage is concentrated in research, comprehension, and structured tasks.

---

## Latency

| Metric | Gemini 3 Flash | MiniMax M2.5 |
|--------|---------------|--------------|
| Avg per task | 11.9s | 15.8s |
| Total (23 tasks) | 273s (4.5 min) | 363s (6.1 min) |

MiniMax is ~33% slower per task — it takes longer to reason through tool calls. Acceptable for quality-focused evaluation.

---

## Cost

### Per-run cost breakdown

| Component | Gemini 3 Flash | MiniMax M2.5 |
|-----------|---------------|--------------|
| Agent input ($0.10 vs $0.20/1M) | ~$0.04 | ~$0.08 |
| Agent output ($0.40 vs $1.17/1M) | ~$0.04 | ~$0.10 |
| **Agent total** | **~$0.08** | **~$0.18** |
| Judge (Claude Sonnet, same) | ~$0.81 | ~$0.81 |
| Image gen (same) | ~$0.04 | ~$0.04 |
| **Total per run** | **~$0.93** | **~$1.03** |
| **Cost per task** | **$0.04** | **$0.04** |

The judge dominates cost (78-83% of total). The agent model difference is only $0.10/run.

### Total evaluation spend

| Item | Cost |
|------|------|
| Gemini runs (6, 7, 8) | ~$2.80 |
| MiniMax runs (9, 10) | ~$2.06 |
| Debug runs (1-5) + manual tests | ~$4.60 |
| **Total OpenRouter spend** | **~$9.50** |

---

## Infrastructure Footprint

Same container for both models — the framework is model-agnostic.

| Metric | Value |
|--------|-------|
| Container image | 26.3 MB |
| Memory (idle) | 3.4 MiB |
| Memory (under load) | ~5-6 MiB |
| CPU (idle) | 0.00% |
| PIDs | 3 |
| Min hosting cost | ~$3-5/mo (512MB VPS) |

---

## Recommendations

### Model selection

**For quality: MiniMax M2.5** — 79% score, low variance, better at structured tasks. Best choice for production agent workloads.

**For cost: Gemini 3 Flash** — 66% score at half the agent cost. Acceptable for simple tasks (file creation, writing, calendar) but unreliable on research and comprehension.

### Remaining improvements

| Fix | Impact | Effort |
|-----|--------|--------|
| Fix HTTP 500 on email triage (context overflow) | +4% for MiniMax | Medium — truncate tool results |
| Fix XLSX parsing | +4% for both | Low — add Python shell workaround |
| Fix image gen grading format | +3-4% for both | Low — add tool call metadata to transcript |
| Use `--runs 3` for MiniMax | More accurate mean | Just cost |
| **Projected MiniMax with fixes** | **~87-90%** | |

### Next models to test

| Model | Expected score | Cost/run | Why |
|-------|---------------|----------|-----|
| MiniMax M2.7 | ~82-85% | ~$0.22 | Latest MiniMax, may push higher |
| DeepSeek V3.2 | ~75-80% | ~$0.12 | Cheapest capable alternative |
| Claude Sonnet 4.6 | ~85-90% | ~$2.00 | Frontier quality, 10x cost |
