# ZeroClaw PinchBench Evaluation Results

**Last updated:** 2026-03-27
**Benchmark:** PinchBench 23-task suite
**Framework:** ZeroClaw v0.5.9 (scratch container, 26.3 MB)
**Artifacts:** [eval/](eval/)

---

## Configuration

| Setting | Value |
|---------|-------|
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

## Latest Results

### Run 32: MiniMax M2.7 — Post-Refactoring Validation (2026-03-27)

| Metric | Value |
|---|---|
| Model | `minimax/minimax-m2.7` |
| Overall Score | 81.3% (18.7 / 23.0) |
| Total cost (OpenRouter) | $0.655 |
| Total wall-clock time | 18m 21s |
| Mean latency/task | 33.2s |
| Total API requests | 25 |
| Tasks >= 80% | 16 |
| Tasks at 0% | 1 |

#### Per-Task Scores

| # | Task | Category | Score | Grading | Latency |
|---|------|----------|-------|---------|---------|
| 00 | Sanity Check | basic | 100% | automated | 2.16s |
| 01 | Calendar Event | calendar | 100% | automated | 18.20s |
| 02 | Stock Research | research | 100% | automated | 19.51s |
| 03 | Blog Post | writing | 83% | llm_judge | 18.93s |
| 04 | Weather Script | coding | 100% | automated | 15.22s |
| 05 | Doc Summary | comprehension | 94% | llm_judge | 14.66s |
| 06 | Tech Conferences | research | 95% | llm_judge | 16.57s |
| 07 | Email Drafting | writing | 100% | llm_judge | 8.29s |
| 08 | Memory Retrieval | context | 70% | automated | 9.32s |
| 09 | File Structure | file_ops | 100% | automated | 12.18s |
| 10 | API Workflow | complex | 42% | hybrid | 18.21s |
| 11 | Project Structure | file_ops | 100% | automated | 20.90s |
| 12 | Search & Replace | file_ops | 33% | automated | 54.49s |
| 13 | Image Generation | creative | 42% | hybrid | 42.88s |
| 14 | Humanize Blog | transformation | 99% | llm_judge | 31.65s |
| 15 | Daily Summary | synthesis | 87% | llm_judge | 55.46s |
| 16a | Email Triage | organization | 84% | hybrid | 52.00s |
| 17 | Email Search | comprehension | 84% | hybrid | 88.62s |
| 16b | Market Research | research | 82% | hybrid | 82.83s |
| 18 | Spreadsheet | data_analysis | 0% | hybrid | 53.38s |
| 20 | PDF Summary | comprehension | 84% | llm_judge | 33.71s |
| 21 | Report Comprehension | comprehension | 94% | automated | 41.78s |
| 22 | Second Brain | memory | 95% | hybrid | 29.48s |

#### Category Breakdown

| Category | Score | Tasks |
|----------|-------|-------|
| Basic | 100.0% | 1 |
| Calendar | 100.0% | 1 |
| Coding | 100.0% | 1 |
| Content Transformation | 99.0% | 1 |
| Memory | 95.0% | 1 |
| Research | 92.4% | 3 |
| Comprehension | 89.2% | 4 |
| Synthesis | 87.0% | 1 |
| Organization | 83.8% | 1 |
| File Ops | 77.8% | 3 |
| Context | 70.0% | 1 |
| Creative | 42.3% | 1 |
| Complex | 41.7% | 1 |
| Data Analysis | 0.0% | 1 |

---

### Run 15: MiniMax M2.5 — Best Overall (2026-03-26)

| Metric | Value |
|---|---|
| Model | `minimax/minimax-m2.5` |
| Overall Score | 81.8% (18.8 / 23.0) |
| Total cost (OpenRouter) | ~$1.03 |
| Mean latency/task | 15.8s |
| Total wall-clock time | 6m 3s |
| Tasks >= 80% | 15 |
| Tasks at 0% | 1 |

#### Per-Task Scores

| # | Task | Category | Score | Grading |
|---|------|----------|-------|---------|
| 00 | Sanity Check | basic | 100% | automated |
| 01 | Calendar Event | calendar | 100% | automated |
| 02 | Stock Research | research | 100% | automated |
| 03 | Blog Post | writing | 80% | llm_judge |
| 04 | Weather Script | coding | 100% | automated |
| 05 | Doc Summary | comprehension | 85% | llm_judge |
| 06 | Tech Conferences | research | 75% | llm_judge |
| 07 | Email Drafting | writing | 95% | llm_judge |
| 08 | Memory Retrieval | context | 80% | automated |
| 09 | File Structure | file_ops | 100% | automated |
| 10 | API Workflow | complex | 87% | hybrid |
| 11 | Project Structure | file_ops | 100% | automated |
| 12 | Search & Replace | file_ops | 100% | automated |
| 13 | Image Generation | creative | 10% | hybrid |
| 14 | Humanize Blog | transformation | 98% | llm_judge |
| 15 | Daily Summary | synthesis | 88% | llm_judge |
| 16a | Email Triage | organization | 35% | hybrid |
| 17 | Email Search | comprehension | 98% | hybrid |
| 16b | Market Research | research | 88% | hybrid |
| 18 | Spreadsheet | data_analysis | 0% | hybrid |
| 20 | PDF Summary | comprehension | 84% | llm_judge |
| 21 | Report Comprehension | comprehension | 94% | automated |
| 22 | Second Brain | memory | 85% | hybrid |

---

## Model Comparison

| Metric | MiniMax M2.5 (Run 15) | MiniMax M2.7 (Run 32) |
|--------|----------------------|----------------------|
| Overall Score | 81.8% | 81.3% |
| Total cost/run | ~$1.03 | $0.655 |
| Input price/1M tokens | $0.20 | $0.30 |
| Output price/1M tokens | $1.17 | $1.20 |
| Mean latency/task | 15.8s | 33.2s |
| Tasks >= 80% | 15 | 16 |
| Tasks at 0% | 1 | 1 |

---

## Cost

### Model Pricing (OpenRouter, per 1M tokens)

| Model | Input | Output |
|-------|-------|--------|
| minimax/minimax-m2.5 | $0.20 | $1.17 |
| minimax/minimax-m2.7 | $0.30 | $1.20 |
| anthropic/claude-sonnet-4.6 (judge) | $3.00 | $15.00 |

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

| Run | Date | Model | Config | Score | Cost |
|-----|------|-------|--------|-------|------|
| 15 | 2026-03-26 | MiniMax M2.5 | PRs 1-3 + per-tool truncation | 81.8% | ~$1.03 |
| 32 | 2026-03-27 | MiniMax M2.7 | Post-refactoring validation | 81.3% | $0.655 |
