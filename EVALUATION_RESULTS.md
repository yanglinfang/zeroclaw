# ZeroClaw PinchBench Evaluation Results

**Last updated:** 2026-03-27
**Benchmark:** PinchBench 23-task suite
**Framework:** ZeroClaw v0.5.9 (scratch container, 26.3 MB)
**Artifacts:** [eval/](eval/)

---

## Configuration

| Setting | Value |
|---------|-------|
| Agent model | `minimax/minimax-m2.7` (Runs 32-36), `minimax/minimax-m2.5` (Run 15) |
| Judge model | `anthropic/claude-sonnet-4.6` (via OpenRouter) |
| Image gen model | `google/gemini-3.1-flash-image-preview` (via OpenRouter) |
| Container | `FROM scratch`, static MUSL binary, 26.3 MB |
| Gateway timeout | 300s |
| Max tool iterations | 25 |
| Max tool result chars | 8,000 default, 32,000 for pdf_read |
| Search snippet cap | 200 chars, 3,000 total |
| Compaction threshold | 30 messages, keep 12 recent |
| Features | `rag-pdf`, `rag-xlsx`, `image_gen` |

---

## Run Summary

| Metric | Run 15 (M2.5) | Run 32 (M2.7) | Run 33 (M2.7) | Run 34 (M2.7) | Run 36 (M2.7) |
|--------|--------------|--------------|--------------|--------------|--------------|
| Date | 2026-03-26 | 2026-03-27 | 2026-03-27 | 2026-03-27 | 2026-03-27 |
| Config | PRs 1-3 + per-tool truncation | Post-refactoring | Pre-cost-fix rebuild | With cost tracking fix | Cost fix + eval folder |
| Overall Score | 81.8% | 81.3% | 74.9% | 75.5% | 80.0% |
| Agent cost (tracked) | — | — | — | $0.4496 | $0.4901 |
| OpenRouter daily spend | ~$1.03 | $0.655 | — | $0.8104 | $1.2046 |
| Input tokens | — | — | — | 1,333,043 | 1,459,126 |
| Output tokens | — | — | — | 41,377 | 43,644 |
| API requests | — | — | — | 83 | 96 |
| Mean latency/task | 15.8s | 33.2s | 41.0s | 29.9s | 32.0s |
| Total wall-clock time | 6m 3s | 18m 21s | 15m 44s | 11m 28s | 12m 16s |
| Tasks >= 80% | 15 | 16 | 14 | 15 | 16 |
| Tasks at 0% | 1 | 1 | 2 | 2 | 1 |

Note: Runs 32-33 did not have working cost tracking. Agent cost and token counts were not recorded. OpenRouter daily spend includes judge + image gen costs and is cumulative within a UTC day.

---

## Per-Task Comparison

| # | Task | Category | Grading | Run 15 (M2.5) | Run 32 (M2.7) | Run 33 (M2.7) | Run 34 (M2.7) | Run 36 (M2.7) |
|---|------|----------|---------|--------------|--------------|--------------|--------------|--------------|
| 00 | Sanity Check | basic | automated | 100% | 100% | 100% | 100% | 100% |
| 01 | Calendar Event | calendar | automated | 100% | 100% | 100% | 0% | 0% |
| 02 | Stock Research | research | automated | 100% | 100% | 100% | 100% | 100% |
| 03 | Blog Post | writing | llm_judge | 80% | 83% | 75% | 88% | 80% |
| 04 | Weather Script | coding | automated | 100% | 100% | 100% | 100% | 100% |
| 05 | Doc Summary | comprehension | llm_judge | 85% | 94% | 92% | 88% | 100% |
| 06 | Tech Conferences | research | llm_judge | 75% | 95% | 80% | 0% | 90% |
| 07 | Email Drafting | writing | llm_judge | 95% | 100% | 95% | 100% | 100% |
| 08 | Memory Retrieval | context | automated | 80% | 70% | 70% | 70% | 70% |
| 09 | File Structure | file_ops | automated | 100% | 100% | 100% | 100% | 100% |
| 10 | API Workflow | complex | hybrid | 87% | 42% | 50% | 50% | 48% |
| 11 | Project Structure | file_ops | automated | 100% | 100% | 100% | 100% | 100% |
| 12 | Search & Replace | file_ops | automated | 100% | 33% | 50% | 100% | 67% |
| 13 | Image Generation | creative | hybrid | 10% | 42% | 30% | 21% | 26% |
| 14 | Humanize Blog | transformation | llm_judge | 98% | 99% | 0% | 45% | 50% |
| 15 | Daily Summary | synthesis | llm_judge | 88% | 87% | 85% | 99% | 86% |
| 16a | Email Triage | organization | hybrid | 35% | 84% | 74% | 84% | 88% |
| 17 | Email Search | comprehension | hybrid | 98% | 84% | 39% | 97% | 97% |
| 16b | Market Research | research | hybrid | 88% | 82% | 89% | 81% | 85% |
| 18 | Spreadsheet | data_analysis | hybrid | 0% | 0% | 100% | 40% | 92% |
| 20 | PDF Summary | comprehension | llm_judge | 84% | 84% | 0% | 78% | 68% |
| 21 | Report Comprehension | comprehension | automated | 94% | 94% | 94% | 94% | 94% |
| 22 | Second Brain | memory | hybrid | 85% | 95% | 100% | 100% | 99% |

---

## Per-Task Latency (seconds)

| # | Task | Run 32 | Run 33 | Run 34 | Run 36 |
|---|------|--------|--------|--------|--------|
| 00 | Sanity Check | 2.16 | 15.95 | 2.56 | 3.58 |
| 01 | Calendar Event | 18.20 | 13.68 | 10.62 | 13.26 |
| 02 | Stock Research | 19.51 | 283.24 | 22.14 | 31.89 |
| 03 | Blog Post | 18.93 | 21.79 | 13.52 | 24.57 |
| 04 | Weather Script | 15.22 | 20.62 | 31.23 | 15.89 |
| 05 | Doc Summary | 14.66 | 20.06 | 23.65 | 17.80 |
| 06 | Tech Conferences | 16.57 | 47.74 | 15.31 | 23.60 |
| 07 | Email Drafting | 8.29 | 9.01 | 20.61 | 15.21 |
| 08 | Memory Retrieval | 9.32 | 8.38 | 8.80 | 8.90 |
| 09 | File Structure | 12.18 | 10.82 | 13.50 | 27.19 |
| 10 | API Workflow | 18.21 | 15.41 | 13.70 | 16.87 |
| 11 | Project Structure | 20.90 | 12.44 | 10.99 | 13.04 |
| 12 | Search & Replace | 54.49 | 36.89 | 35.53 | 26.95 |
| 13 | Image Generation | 42.88 | 19.17 | 36.84 | 22.30 |
| 14 | Humanize Blog | 31.65 | 27.98 | 22.86 | 35.01 |
| 15 | Daily Summary | 55.46 | 34.26 | 29.39 | 36.32 |
| 16a | Email Triage | 52.00 | 58.47 | 50.93 | 49.25 |
| 17 | Email Search | 88.62 | 64.72 | 82.30 | 77.42 |
| 16b | Market Research | 82.83 | 92.74 | 134.17 | 121.83 |
| 18 | Spreadsheet | 53.38 | 63.78 | 17.58 | 71.95 |
| 20 | PDF Summary | 33.71 | 25.46 | 28.77 | 31.70 |
| 21 | Report Comprehension | 41.78 | 19.89 | 41.53 | 22.88 |
| 22 | Second Brain | 29.48 | 21.39 | 21.13 | 28.61 |

---

## Cost

### Model Pricing (OpenRouter, per 1M tokens)

| Model | Input | Output |
|-------|-------|--------|
| minimax/minimax-m2.5 | $0.20 | $1.17 |
| minimax/minimax-m2.7 | $0.30 | $1.20 |
| anthropic/claude-sonnet-4.6 (judge) | $3.00 | $15.00 |

### Tracked Agent Cost (runs with working cost tracking)

| Metric | Run 34 | Run 36 |
|--------|--------|--------|
| Agent input tokens | 1,333,043 | 1,459,126 |
| Agent output tokens | 41,377 | 43,644 |
| Agent cost (total) | $0.4496 | $0.4901 |
| OpenRouter daily spend | $0.8104 | $1.2046 |

---

## Infrastructure

| Metric | Value |
|--------|-------|
| Container image | 26.3 MB |
| Memory (idle) | 3.4 MiB |
| Memory (under load) | ~5-6 MiB |
| CPU (idle) | 0.00% |
| PIDs | 3 |

---

## Run History

| Run | Date | Model | Config | Score | Agent Cost | OpenRouter Daily |
|-----|------|-------|--------|-------|------------|-----------------|
| 15 | 2026-03-26 | MiniMax M2.5 | PRs 1-3 + per-tool truncation | 81.8% | — | ~$1.03 |
| 32 | 2026-03-27 | MiniMax M2.7 | Post-refactoring validation | 81.3% | — | $0.655 |
| 33 | 2026-03-27 | MiniMax M2.7 | Pre-cost-fix rebuild | 74.9% | — | — |
| 34 | 2026-03-27 | MiniMax M2.7 | With cost tracking fix | 75.5% | $0.4496 | $0.8104 |
| 36 | 2026-03-27 | MiniMax M2.7 | Cost fix + eval folder | 80.0% | $0.4901 | $1.2046 |
