# Post-Refactoring Validation: Rebuild Docker + Re-run PinchBench

**Purpose:** Confirm the agent loop refactoring (splitting `loop_.rs` into 5 focused modules) did not break any functionality. Re-run PinchBench 23-task suite on MiniMax M2.7 and compare against the pre-refactoring baseline (Run 14: 74.4%).

**Prerequisites already in place:**
- OpenRouter API keys configured in `.env`
- FAL_API_KEY configured for image generation tasks
- PinchBench adapted for ZeroClaw (see `EVALUATION.md` Phase 3)
- Docker installed and working

---

## What Changed in the Refactoring

The monolithic `src/agent/loop_.rs` (8,695 lines) was split into 6 files:

| New Module | Lines | What It Contains |
|---|---|---|
| `src/agent/tool_parsing.rs` | 1,612 | All LLM response parsing — JSON, XML, MiniMax invoke, GLM, Perl-style, FunctionCall formats. Includes `ParsedToolCall`, display helpers, sanitization. |
| `src/agent/tool_execution.rs` | 296 | Single and batch tool dispatch — `execute_one_tool`, `execute_tools_parallel`, `execute_tools_sequential`, cancellation handling, credential scrubbing delegation. |
| `src/agent/tool_filter.rs` | 170 | Per-turn tool filtering by MCP groups/keywords, capability allowlists, and `scrub_credentials` for redacting secrets in tool output. |
| `src/agent/cost_tracking.rs` | 138 | Token cost tracking per tool loop — budget checking, streaming cost recording, model pricing lookup. |
| `src/agent/history.rs` | 185 | Conversation history compaction, trim, persistence. Interactive session state save/load. |
| `src/agent/loop_.rs` | 6,381 | Remaining orchestration: `run_tool_call_loop`, `run`, `process_message`, context builders, constants, all tests. |

**No public API changed.** All external callers (`channels/mod.rs`, `agent/agent.rs`, `tools/delegate.rs`, `tools/model_switch.rs`, `providers/anthropic.rs`) continue to work. Import paths were updated where functions moved.

**Additional fix included: `pdf_read` URL support**

`src/tools/pdf_read.rs` was extended to accept HTTP/HTTPS URLs in the `path` parameter. When given a URL, the tool downloads the PDF to `workspace/.pdf_downloads/` before extraction. This fixes the "share a link to a PDF and ask ZeroClaw to read it" use case that previously failed because `web_fetch` rejects PDFs and `shell` blocks `curl`. Safety checks: 50 MB size limit, content-type validation, `%PDF` magic-number check, 60s download timeout, rate-limit budget consumed.

**Additional fix: Excel/XLSX reading via `file_read`**

`src/tools/file_read.rs` was extended to extract tabular data from Excel spreadsheets (.xlsx, .xls, .xlsm, .xlsb, .ods) using the `calamine` crate (behind `--features rag-xlsx`). When `file_read` encounters a spreadsheet binary file, it renders each sheet as a markdown pipe-delimited table with a `## Sheet: <name>` header. This fixes task_18_spreadsheet_summary which requires reading multi-sheet XLSX files. `Dockerfile.scratch` updated to build with `--features rag-pdf,rag-xlsx`.

**Pre-refactoring verification:**
- `cargo check` — clean (0 errors, 0 warnings)
- `cargo fmt --all -- --check` — clean
- Clippy has 4 pre-existing warnings in `src/tools/web_search_tool.rs` (unrelated to refactoring)

---

## Step 1: Verify Local Compilation

Before touching Docker, confirm the code compiles:

```bash
cargo check
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings  # expect 4 pre-existing warnings in web_search_tool.rs
```

If `cargo check` fails, STOP. The refactoring introduced a regression — investigate before proceeding.

---

## Step 2: Rebuild Docker Image

```bash
# Full rebuild — no cache, to ensure the new module structure compiles clean in the container
docker compose -f docker-compose.minimal.yml build --no-cache

# Start the container
docker compose -f docker-compose.minimal.yml up -d
```

Wait for startup, then verify health:

```bash
# Wait up to 30 seconds for healthy status
for i in $(seq 1 30); do
  STATUS=$(docker inspect --format='{{.State.Health.Status}}' zeroclaw-minimal 2>/dev/null)
  if [ "$STATUS" = "healthy" ]; then echo "Healthy after ${i}s"; break; fi
  sleep 1
done

# Confirm via HTTP
curl -s http://localhost:42617/health | python3 -m json.tool
# Expected: {"status": "ok", ...}
```

If the container fails to start or health check fails:
```bash
docker logs zeroclaw-minimal --tail 50
```

Common issues:
- Compilation failure in container → check `docker logs` for Rust errors
- Port conflict → `lsof -i :42617` and kill conflicting process
- Config issue → verify `.env` has all required keys

---

## Step 3: Smoke Test — Tool Execution

Verify the agent can still execute tools (this is the critical path the refactoring touched):

```bash
# Test 1: Simple file creation (exercises tool dispatch pipeline)
curl -s -X POST http://localhost:42617/webhook \
  -H "Content-Type: application/json" \
  -d '{"message": "Create a file at /tmp/pinchbench/refactor-test.txt containing: refactoring validation passed"}' \
  | python3 -m json.tool

# Verify the file was created
docker exec zeroclaw-minimal cat /tmp/pinchbench/refactor-test.txt 2>/dev/null || \
  cat /tmp/pinchbench/refactor-test.txt
# Expected content: "refactoring validation passed"
```

```bash
# Test 2: Multi-tool interaction (exercises tool parsing + sequential execution)
curl -s -X POST http://localhost:42617/webhook \
  -H "Content-Type: application/json" \
  -d '{"message": "Search the web for \"rust programming language\" and save a one-paragraph summary to /tmp/pinchbench/rust-summary.txt"}' \
  | python3 -m json.tool
```

If either test fails, check the response for error details. The most likely failure points after refactoring:
- Tool call parsing (moved to `tool_parsing.rs`) — would show "no tool calls parsed" in response
- Tool execution (moved to `tool_execution.rs`) — would show tool errors in response
- Credential scrubbing (moved to `tool_filter.rs`) — would show raw API keys in response (check carefully!)

---

## Step 4: Run PinchBench Sanity Check

```bash
cd /path/to/pinchbench

ZEROCLAW_GATEWAY_URL=http://localhost:42617 \
ZEROCLAW_WORKSPACE=/tmp/pinchbench \
  uv run scripts/benchmark.py \
    --model openrouter/minimax/minimax-m2.7 \
    --suite task_00_sanity \
    --no-upload --verbose
```

Expected: `task_00_sanity: 1.0/1.0 (100%)`. If this fails, the issue is likely in the benchmark adapter (`lib_agent.py`), not the refactoring.

---

## Step 5: Run Full PinchBench Suite on MiniMax M2.7

```bash
ZEROCLAW_GATEWAY_URL=http://localhost:42617 \
ZEROCLAW_WORKSPACE=/tmp/pinchbench \
  uv run scripts/benchmark.py \
    --model openrouter/minimax/minimax-m2.7 \
    --judge openrouter/anthropic/claude-sonnet-4.6 \
    --suite all \
    --timeout-multiplier 5 \
    --no-upload --verbose
```

**Baseline to compare against (pre-refactoring Run 14):**

| Category | Pre-Refactoring Score |
|---|---|
| Overall | 74.4% |
| Basic/Sanity | 100% |
| File Operations | High |
| Research | Medium |
| Writing | Medium-High |
| Comprehension | Variable |

**Success criteria:**
- Overall score >= 72% (within 2.4% of baseline, accounting for model variance)
- No task that previously scored >80% now scores 0%
- No new tool dispatch failures (agent says "I can't use tools" or similar)
- No credential leaks in any task output

**Failure criteria — investigate immediately:**
- Overall score drops >10% from baseline → likely a regression in tool parsing or execution
- Multiple tasks fail with "no tool calls" → `tool_parsing.rs` extraction broke something
- Tasks fail with "permission denied" or tool errors → `tool_execution.rs` issue
- Any task output contains raw API keys → `tool_filter.rs` credential scrubbing broken

---

## Step 6: Record Results

Save benchmark output to `eval/run_XX_m2.7_post_refactor.json` and update `EVALUATION_RESULTS.md` with a new section:

```markdown
## Run XX: MiniMax M2.7 — Post-Refactoring Validation

| Metric | Value |
|---|---|
| Model | minimax/minimax-m2.7 |
| Date | YYYY-MM-DD |
| Overall Score | XX.X% |
| Pre-refactoring baseline | 74.4% (Run 14) |
| Delta | +/- X.X% |
| Verdict | PASS / INVESTIGATE / FAIL |
```

**Verdict guide:**
- **PASS**: Score within ±5% of baseline. Refactoring validated.
- **INVESTIGATE**: Score dropped 5-10%. Check per-task breakdown for specific regressions.
- **FAIL**: Score dropped >10% or new 0-score tasks appeared. Roll back and debug.

---

## Step 7: Cleanup

```bash
# Remove test files
rm -f /tmp/pinchbench/refactor-test.txt /tmp/pinchbench/rust-summary.txt

# If validation passed, the refactoring is confirmed safe.
# Proceed with committing changes and opening a PR.
```

---

## Quick Reference: Module Responsibilities

If debugging a specific failure, here's where to look:

| Symptom | Module to Check | Key Function |
|---|---|---|
| "No tool calls parsed" | `tool_parsing.rs` | `parse_tool_calls()` |
| Tool execution errors | `tool_execution.rs` | `execute_one_tool()`, `execute_tools_parallel()` |
| Wrong tools offered to model | `tool_filter.rs` | `filter_tool_specs_for_turn()` |
| Budget/cost errors | `cost_tracking.rs` | `check_tool_loop_budget()` |
| Context too long / compaction issues | `history.rs` | `auto_compact_history()` |
| Main loop logic | `loop_.rs` | `run_tool_call_loop()` |
| Credential leak in output | `tool_filter.rs` | `scrub_credentials()` |
