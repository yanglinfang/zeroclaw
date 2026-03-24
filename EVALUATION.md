# ZeroClaw Framework Evaluation via PinchBench

**Goal:** Measure how effectively ZeroClaw harnesses LLM capabilities as an agent runtime, using PinchBench's 23-task real-world benchmark suite.

**What we're measuring:**
- Framework effectiveness — for tasks ZeroClaw can attempt, how well does it score?
- Framework coverage — what percentage of tasks can ZeroClaw attempt?
- Cost efficiency — score per dollar spent

**What we're NOT measuring:**
- Raw model capability (that's what PinchBench's leaderboard already does with OpenClaw)
- OpenClaw-specific features

---

## Phase 1: ZeroClaw Setup (from scratch)

### 1.1 Prerequisites

```bash
# Rust toolchain (if building natively)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Docker (if containerized)
docker --version  # needs 20+

# Python + uv (for PinchBench)
python3 --version  # needs 3.10+
curl -LsSf https://astral.sh/uv/install.sh | sh
```

### 1.2 API keys required

| Key | Purpose | Where to get |
|-----|---------|-------------|
| `API_KEY` | LLM provider (OpenRouter recommended) | https://openrouter.ai/keys |
| `FAL_API_KEY` | Image generation (task_13) | https://fal.ai/dashboard/keys |

Optional but recommended:
| Key | Purpose |
|-----|---------|
| `BRAVE_API_KEY` | Better web search results (default DuckDuckGo works too) |

### 1.3 Create `.env`

```bash
cd zeroclaw/

cat > .env << 'EOF'
# Provider
PROVIDER=openrouter
API_KEY=<your-openrouter-key>
MODEL=google/gemini-2.5-flash-preview

# Image generation
FAL_API_KEY=<your-fal-key>

# Evaluation settings
ZEROCLAW_MAX_TOOL_ITERATIONS=25
ZEROCLAW_GATEWAY_TIMEOUT_SECS=300
ZEROCLAW_WORKSPACE=/tmp/pinchbench
ZEROCLAW_ALLOW_PUBLIC_BIND=true
ZEROCLAW_GATEWAY_HOST=[::]
ZEROCLAW_GATEWAY_PORT=42617
EOF
```

### 1.4 Build with full features

The minimal Docker build uses `--no-default-features` which strips PDF reading and other tools. For evaluation, build with `rag-pdf` enabled.

**Option A: Docker (recommended)**

```bash
docker compose -f docker-compose.minimal.yml up -d --build
```

> Note: `Dockerfile.scratch` needs `--features rag-pdf` instead of `--no-default-features` for PDF tasks. See Phase 2.

**Option B: Native**

```bash
cargo build --release --features rag-pdf
./target/release/zeroclaw daemon
```

### 1.5 Verify health

```bash
curl -s http://localhost:42617/health | python3 -m json.tool
# Should show: "status": "ok"
```

**STOP HERE.** Verify the health check passes before proceeding. If it fails, check Docker logs: `docker logs zeroclaw-minimal`.

---

## Phase 2: Apply ZeroClaw Code Fixes

These are required fixes found during prior evaluation. Apply before benchmarking.

### 2.1 Webhook must use tools (P0 — blocks all tool tasks)

The webhook endpoint uses `run_gateway_chat_simple()` which has no tools. Change to `run_gateway_chat_with_tools()`.

**File:** `src/gateway/mod.rs`

Find:
```rust
match run_gateway_chat_simple(&state, message).await {
```

Replace with:
```rust
match run_gateway_chat_with_tools(&state, message, session_id.as_deref()).await {
```

### 2.2 Streaming token usage tracking

Without this fix, the cost page shows no data for streaming responses.

**Files to change:**
- `src/providers/traits.rs` — add `usage: Option<TokenUsage>` field to `StreamChunk`
- `src/providers/compatible.rs` — parse `usage` from SSE final chunks
- `src/agent/agent.rs` — accumulate stream usage into `ChatResponse`
- `src/agent/loop_.rs` — add `record_cost_for_streaming()` function
- `src/cost/tracker.rs` — add `CostTracker::get_global()` method

See git diff for exact changes (these were implemented in the prior session).

### 2.3 Fix production `.unwrap()` panics (P0)

**Files:**
- `src/channels/matrix.rs:605` — `split_once("||").unwrap()` → `if let Some`
- `src/channels/mattermost.rs:226,369` — `as_object_mut().unwrap()` → `if let Some`
- `src/tools/security_ops.rs:155` — `as_str().unwrap()` → `if let Some`

### 2.4 Add DeepSeek/Gemini to default pricing table

**File:** `src/config/schema.rs` — `get_default_pricing()` function

Add entries for models you'll evaluate so cost tracking reports real dollar amounts.

### 2.5 Build with features for evaluation

**File:** `Dockerfile.scratch` — change the cargo build line:

```dockerfile
# FROM:
cargo build --release --locked --no-default-features

# TO:
cargo build --release --locked --features rag-pdf
```

### 2.6 Config for evaluation

**File:** `Dockerfile.scratch` — baked-in config must include:

```toml
[gateway]
require_pairing = false
allow_public_bind = true
host = "[::]"
port = 42617

[autonomy]
level = "full"
require_approval_for_medium_risk = false
block_high_risk_commands = false
allowed_roots = ["/tmp/pinchbench"]

[cost]
enabled = true
```

### 2.7 Docker compose for evaluation

**File:** `docker-compose.minimal.yml` — must include:

```yaml
volumes:
  - zeroclaw-data:/zeroclaw-data
  - /tmp/pinchbench:/tmp/pinchbench
```

**STOP HERE.** Rebuild: `docker compose -f docker-compose.minimal.yml up -d --build`. Verify health check passes. Then manually test one tool call:

```bash
curl -s -X POST http://localhost:42617/webhook \
  -H "Content-Type: application/json" \
  -d '{"message": "Create a file at /tmp/pinchbench/test.txt with the content hello"}' \
  | python3 -m json.tool

# Verify:
cat /tmp/pinchbench/test.txt
# Should contain: hello
```

If this fails, debug before proceeding. Common issues:
- Config has `require_pairing = true` → check persisted config at `/tmp/pinchbench/config.toml`
- Security policy blocks writes → check `autonomy.level` and `allowed_roots`
- Gateway timeout → check `ZEROCLAW_GATEWAY_TIMEOUT_SECS` is set

---

## Phase 3: Adapt PinchBench

### 3.1 Clone PinchBench

```bash
git clone https://github.com/pinchbench/skill.git pinchbench
cd pinchbench
```

### 3.2 Replace agent execution in `lib_agent.py`

The core change: replace `subprocess.run(["openclaw", ...])` with HTTP calls to ZeroClaw's `/webhook`.

**What to change in `execute_openclaw_task()`:**

```python
# BEFORE: subprocess call
result = subprocess.run(
    ["openclaw", "agent", "--agent", agent_id, "--session-id", session_id, "--message", task.prompt],
    capture_output=True, text=True, cwd=str(workspace), timeout=timeout_seconds, check=False,
)

# AFTER: HTTP call to ZeroClaw webhook
import urllib.request, urllib.error
url = os.environ.get("ZEROCLAW_GATEWAY_URL", "http://localhost:42617") + "/webhook"
payload = json.dumps({"message": task.prompt}).encode("utf-8")
headers = {"Content-Type": "application/json"}
req = urllib.request.Request(url, data=payload, headers=headers, method="POST")
with urllib.request.urlopen(req, timeout=timeout_seconds) as resp:
    data = json.loads(resp.read())
    response_text = data.get("response", "")
```

**Build transcript in-memory** instead of reading JSONL from disk:

```python
transcript = [
    {"type": "message", "message": {"role": "user", "content": task.prompt}},
    {"type": "message", "message": {"role": "assistant", "content": response_text}},
]
```

**Replace `ensure_agent_exists()`** with a health check:

```python
def ensure_agent_exists(agent_id, model_id, workspace_dir):
    workspace_dir.mkdir(parents=True, exist_ok=True)
    # Verify ZeroClaw is running
    try:
        resp = urllib.request.urlopen(f"{ZEROCLAW_URL}/health", timeout=5)
        return True
    except Exception:
        raise RuntimeError("ZeroClaw not running at " + ZEROCLAW_URL)
```

**Replace `run_openclaw_prompt()`** (used by judge) with the same HTTP pattern.

**Replace `_get_agent_workspace()`** to return ZeroClaw's workspace:

```python
def _get_agent_workspace(agent_id):
    workspace = os.environ.get("ZEROCLAW_WORKSPACE", "/tmp/pinchbench")
    return Path(workspace) / "workspace"
```

**Remove:** `_get_agent_store_dir()`, `_load_transcript()`, `_resolve_session_id_from_store()`, `_find_transcript_path_from_sessions_store()`, `_find_recent_session_path()`, `cleanup_agent_sessions()`. These are all OpenClaw filesystem conventions that don't apply.

### 3.3 Exclude OpenClaw-specific task

**File:** `scripts/benchmark.py`

Add near the top:

```python
# Tasks that test OpenClaw-specific knowledge, not general agent capability
EXCLUDED_TASKS = {"task_21_openclaw_comprehension"}
```

Apply in the task loading/filtering logic:

```python
tasks_to_run = [t for t in tasks_to_run if t.task_id not in EXCLUDED_TASKS]
```

### 3.4 Fix task_14 prompt

**File:** `tasks/task_14_humanizer.md`

Remove or comment out the OpenClaw skill reference (`/install humanizer`). Keep the fallback instruction ("manually rewrite it to sound more human").

### 3.5 Verify with sanity check

**STOP HERE.** Run only the sanity task first:

```bash
ZEROCLAW_GATEWAY_URL=http://localhost:42617 \
ZEROCLAW_WORKSPACE=/tmp/pinchbench \
  uv run scripts/benchmark.py \
    --model openrouter/google/gemini-2.5-flash-preview \
    --suite task_00_sanity \
    --no-upload --verbose
```

Expected: `task_00_sanity: 1.0/1.0 (100%)`. If this fails, debug the `lib_agent.py` changes before proceeding.

### 3.6 Verify file creation with one tool task

```bash
ZEROCLAW_GATEWAY_URL=http://localhost:42617 \
ZEROCLAW_WORKSPACE=/tmp/pinchbench \
  uv run scripts/benchmark.py \
    --model openrouter/google/gemini-2.5-flash-preview \
    --suite task_09_files \
    --no-upload --verbose
```

Expected: `task_09_files: 1.0/1.0 (100%)` with workspace files listed in verbose output. If files don't appear, check the workspace mount and `allowed_roots`.

---

## Phase 4: Run Automated-Only Suite

Run the 8 automated tasks (no judge cost) to validate the full pipeline.

```bash
ZEROCLAW_GATEWAY_URL=http://localhost:42617 \
ZEROCLAW_WORKSPACE=/tmp/pinchbench \
  uv run scripts/benchmark.py \
    --model openrouter/google/gemini-2.5-flash-preview \
    --suite automated-only \
    --no-upload --timeout-multiplier 5 --verbose
```

**Expected baseline:** 7-8 out of 8 tasks passing (task_08_memory may fail depending on workspace file handling).

**STOP HERE.** Review results. If automated score is below 70%, debug individual task failures before spending money on LLM judge runs.

---

## Phase 5: Full Suite with LLM Judge

### 5.1 Choose models

| Role | Recommended | Cost |
|------|-------------|------|
| **Agent model** (under test) | `google/gemini-2.5-flash-preview` | ~$0.10/1M input |
| **Judge model** | `anthropic/claude-sonnet-4.6` | ~$3/1M input |

Start with the cheapest capable agent model. Judge model should be high-quality since it determines scores.

### 5.2 Run full suite

```bash
ZEROCLAW_GATEWAY_URL=http://localhost:42617 \
ZEROCLAW_WORKSPACE=/tmp/pinchbench \
  uv run scripts/benchmark.py \
    --model openrouter/google/gemini-2.5-flash-preview \
    --judge openrouter/anthropic/claude-sonnet-4.6 \
    --suite all \
    --timeout-multiplier 5 \
    --no-upload --verbose
```

**Estimated cost:** $1-3 total (agent calls + judge evaluations for 22 tasks).

### 5.3 Multi-run for statistical significance

```bash
# 3 runs to get mean + stddev
  uv run scripts/benchmark.py \
    --model openrouter/google/gemini-2.5-flash-preview \
    --judge openrouter/anthropic/claude-sonnet-4.6 \
    --suite all --runs 3 \
    --timeout-multiplier 5 \
    --no-upload
```

**Estimated cost:** $5-8 for 3 runs.

---

## Phase 6: Compare Models

Run the same suite across multiple models to isolate framework vs model performance.

```bash
for model in \
  "openrouter/google/gemini-2.5-flash-preview" \
  "openrouter/minimax/minimax-m2.1" \
  "openrouter/deepseek/deepseek-v3.2" \
  "openrouter/anthropic/claude-sonnet-4.6"; do

  echo "=== Testing: $model ==="
  uv run scripts/benchmark.py \
    --model "$model" \
    --judge openrouter/anthropic/claude-sonnet-4.6 \
    --suite all --runs 2 \
    --timeout-multiplier 5 \
    --no-upload
done
```

### Interpreting results

| If you see... | It means... |
|---|---|
| All models score similarly | Framework is the bottleneck (tool dispatch, prompt construction, etc.) |
| Scores scale with model capability | Framework is transparent — it lets the model shine |
| One category fails across all models | Framework has a capability gap in that area |
| One model scores much lower | Model-specific issue (tool call format, instruction following) |

---

## Phase 7: Record Results

### Metrics to capture per run

| Metric | Source |
|--------|--------|
| Overall score (%) | PinchBench summary output |
| Per-category scores | PinchBench summary output |
| Per-task scores | `results/*.json` |
| Total cost ($) | OpenRouter dashboard + PinchBench usage output |
| Cost per task | Total / task count |
| Score per dollar | Overall score / total cost |
| Execution time per task | PinchBench verbose output |
| Container memory during run | `docker stats --no-stream` (sample periodically) |

### Results format

Save to `results/zeroclaw_evaluation.json`:

```json
{
  "framework": "zeroclaw",
  "framework_version": "<git hash>",
  "pinchbench_version": "<git hash>",
  "date": "2026-03-24",
  "tasks_evaluable": 22,
  "tasks_excluded": ["task_21_openclaw_comprehension"],
  "exclusion_reason": "Tests knowledge of a different product",
  "runs": [
    {
      "model": "google/gemini-2.5-flash-preview",
      "judge": "anthropic/claude-sonnet-4.6",
      "overall_score": 0.82,
      "category_scores": { ... },
      "total_cost_usd": 1.50,
      "execution_time_seconds": 420
    }
  ]
}
```

---

## Task Coverage Matrix

22 of 23 tasks evaluable with full ZeroClaw setup.

| # | Task | Category | Grading | ZeroClaw Tools Used |
|---|------|----------|---------|---------------------|
| 00 | Sanity Check | basic | automated | (none) |
| 01 | Calendar Event Creation | calendar | automated | file_write |
| 02 | Stock Price Research | research | automated | web_search, file_write |
| 03 | Blog Post Writing | writing | llm_judge | file_write |
| 04 | Weather Script Creation | coding | automated | file_write |
| 05 | Document Summarization | comprehension | llm_judge | file_read, file_write |
| 06 | Tech Conference Research | research | llm_judge | web_search, file_write |
| 07 | Professional Email Drafting | writing | llm_judge | file_write |
| 08 | Memory Retrieval | context | automated | file_read, file_write |
| 09 | File Structure Creation | file_ops | automated | file_write, shell |
| 10 | Multi-step API Workflow | complex | hybrid | file_read, file_write |
| 11 | Project Structure Creation | file_ops | automated | file_write |
| 12 | Search and Replace | file_ops | automated | file_read, file_write |
| 13 | Image Generation | creative | hybrid | image_gen (fal.ai) |
| 14 | Humanize Blog | transformation | llm_judge | file_read, file_write |
| 15 | Daily Research Summary | synthesis | llm_judge | file_read, file_write |
| 16a | Email Inbox Triage | organization | hybrid | file_read, file_write |
| 16b | Market Research | research | hybrid | web_search, file_write |
| 17 | Email Search & Summary | comprehension | hybrid | file_read, file_write |
| 18 | Spreadsheet Summary | data_analysis | hybrid | file_read, shell, file_write |
| 20 | ELI5 PDF Summary | comprehension | llm_judge | pdf_read, file_write |
| 22 | Second Brain | memory | hybrid | file_read, file_write, memory |

**Excluded:**
| 21 | OpenClaw Comprehension | — | — | Tests OpenClaw-specific knowledge |

---

## Estimated Total Cost

| Phase | Cost |
|-------|------|
| Phase 4: Automated-only (1 run) | ~$0.10-0.30 |
| Phase 5: Full suite + judge (1 run) | ~$1-3 |
| Phase 5: Full suite (3 runs) | ~$5-8 |
| Phase 6: 4-model comparison (2 runs each) | ~$15-25 |
| **Total for complete evaluation** | **~$20-35** |

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `[HTTP 408]` responses | Gateway timeout too low | Set `ZEROCLAW_GATEWAY_TIMEOUT_SECS=300` in `.env`, do full `down/up` |
| Files created but grader says 0% | Workspace mismatch | Ensure `_get_agent_workspace()` returns ZeroClaw's actual workspace path |
| "Permission denied" in agent response | Security policy blocking | Check `autonomy.level = "full"` and `allowed_roots` in persisted config |
| Gateway unreachable from host | Binding to localhost inside container | Set `ZEROCLAW_GATEWAY_HOST=[::]` |
| "Unauthorized" on webhook | Pairing still enabled | Set `require_pairing = false` in config |
| Agent describes actions but doesn't execute | Webhook using simple chat (no tools) | Apply Phase 2.1 fix |
| Cost page shows $0.00 | Model not in pricing table | Add model to `get_default_pricing()` |
| PDF task fails | `rag-pdf` feature not enabled | Rebuild with `--features rag-pdf` |
| Image gen task fails | Missing API key | Set `FAL_API_KEY` in `.env` |

---

## Cleanup

After evaluation is complete:
- Delete this file or move to `docs/`
- Commit the ZeroClaw code fixes (Phase 2) as a proper PR
- Fork PinchBench changes (Phase 3) into a `zeroclaw` branch or separate repo
- Archive results in `results/`
