# PinchBench Evaluation Plan for ZeroClaw

**Created:** 2026-03-24
**Goal:** Run the PinchBench 23-task suite against ZeroClaw to get accuracy, speed, and cost scores comparable to the public leaderboard.

---

## 1. The Compatibility Problem

PinchBench was built for OpenClaw. It doesn't use HTTP APIs — it shells out to the `openclaw` CLI via Python's `subprocess.run()`. Specifically:

```python
# How PinchBench submits tasks:
subprocess.run(["openclaw", "agent", "--agent", agent_id, "--session-id", session_id, "--message", prompt])

# How it reads results:
# Parses JSONL files from ~/.openclaw/agents/{agent_id}/sessions/
```

ZeroClaw has a different CLI surface (`zeroclaw daemon`, `zeroclaw gateway`, etc.) and different workspace paths (`~/.zeroclaw/`). So we need a bridge.

### Three options:

| Option | Effort | Fidelity | Recommended |
|--------|--------|----------|-------------|
| **A. CLI shim** — create an `openclaw` wrapper that translates to `zeroclaw` calls | Low | High | Yes |
| **B. Fork PinchBench** — modify `lib_agent.py` to call ZeroClaw's gateway API | Medium | Highest | If shim doesn't work |
| **C. OpenClaw compat mode** — add `openclaw`-compatible CLI aliases to ZeroClaw | High | Perfect | Long-term |

**Recommendation: Start with Option A (CLI shim).** If session/transcript format differences cause issues, fall back to Option B.

---

## 2. Prerequisites

### 2.1 On the host machine

```bash
# Python + uv
python3 --version    # needs 3.10+
pip install uv       # or: curl -LsSf https://astral.sh/uv/install.sh | sh

# ZeroClaw running (Docker or native)
docker ps | grep zeroclaw    # should show zeroclaw-minimal
curl http://localhost:42617/health

# Clone PinchBench
git clone https://github.com/pinchbench/skill.git pinchbench
cd pinchbench
```

### 2.2 Environment

```bash
# .env file for PinchBench (create in pinchbench/ directory)
export PROVIDER=openrouter                              # or deepseek, anthropic
export API_KEY=sk-or-...                                # your OpenRouter key
export ZEROCLAW_GATEWAY_URL=http://localhost:42617       # ZeroClaw gateway
```

---

## 3. Option A: CLI Shim (`openclaw` → `zeroclaw`)

Create a script that PinchBench calls as `openclaw`, which translates commands to ZeroClaw equivalents.

### 3.1 Investigate CLI compatibility

First, map the OpenClaw commands PinchBench uses to ZeroClaw equivalents:

```bash
# PinchBench calls these openclaw CLI commands:
openclaw agent --create --name {name} --workspace {path}
openclaw agent --agent {id} --session-id {sid} --message {prompt}
openclaw agent --list

# Check what ZeroClaw supports:
zeroclaw --help
zeroclaw daemon --help
# Look for: agent subcommand, session management, message passing
```

### 3.2 Create the shim

```bash
#!/usr/bin/env bash
# File: /usr/local/bin/openclaw (or add to PATH)
# Translates openclaw CLI calls to ZeroClaw gateway API calls

set -euo pipefail

ZEROCLAW_URL="${ZEROCLAW_GATEWAY_URL:-http://localhost:42617}"

case "${1:-}" in
  agent)
    shift
    # Parse openclaw agent flags and translate to ZeroClaw gateway API
    # Key endpoints to map:
    #   POST /api/v1/chat    — send message
    #   GET  /api/v1/status  — check agent status
    #   GET  /api/v1/history — retrieve transcript
    # ... (implementation depends on ZeroClaw gateway API surface)
    ;;
  *)
    echo "Unsupported openclaw command: $*" >&2
    exit 1
    ;;
esac
```

### 3.3 Transcript compatibility

PinchBench expects JSONL transcripts at `~/.openclaw/agents/{id}/sessions/` with this schema:

```jsonl
{"type": "message", "message": {"role": "assistant", "content": "..."}}
{"type": "tool_call", "tool": {"name": "web_search", "args": {...}}, "result": "..."}
{"type": "message", "message": {"role": "assistant", "content": "...", "usage": {"input": 1234, "output": 567, "cost": {"total": 0.003}}}}
```

**Action:** Check ZeroClaw's transcript format and write a converter if needed:

```bash
# Find ZeroClaw's transcript/session storage
find ~/.zeroclaw -name "*.jsonl" -o -name "*.ndjson" | head -5
cat ~/.zeroclaw/workspace/sessions/*.jsonl | head -20
```

---

## 4. Option B: Fork PinchBench (fallback)

If the CLI shim hits too many format mismatches, fork PinchBench and modify `lib_agent.py` to use ZeroClaw's gateway API directly.

### 4.1 Fork and patch

```bash
cd pinchbench
cp scripts/lib_agent.py scripts/lib_agent_zeroclaw.py
```

### 4.2 Replace subprocess calls with HTTP

```python
# BEFORE (OpenClaw CLI):
subprocess.run(["openclaw", "agent", "--agent", agent_id, "--message", prompt])

# AFTER (ZeroClaw Gateway API):
import requests

def execute_zeroclaw_task(task, model_id, workspace_path, timeout):
    url = f"{ZEROCLAW_URL}/api/v1/chat"
    payload = {
        "message": task["prompt"],
        "model": model_id,
        "session_id": str(uuid.uuid4()),
        "workspace": workspace_path,
    }
    resp = requests.post(url, json=payload, timeout=timeout)
    return resp.json()
```

### 4.3 Adapt transcript parsing

```python
# Map ZeroClaw response format to PinchBench's expected transcript schema
def convert_zeroclaw_transcript(zeroclaw_response):
    transcript = []
    for entry in zeroclaw_response.get("history", []):
        transcript.append({
            "type": "message",
            "message": {
                "role": entry["role"],
                "content": entry["content"],
            }
        })
        # Include usage if present
        if "usage" in entry:
            transcript[-1]["message"]["usage"] = entry["usage"]
    return transcript
```

---

## 5. Execution Plan

### Phase 1: Reconnaissance (30 min)

```bash
# 1. Check ZeroClaw's gateway API surface
curl http://localhost:42617/ | python3 -m json.tool
curl http://localhost:42617/api/ | python3 -m json.tool
# Try common OpenClaw-compatible endpoints:
curl -X POST http://localhost:42617/api/v1/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "hello"}'

# 2. Check ZeroClaw CLI for openclaw-compatible subcommands
zeroclaw --help 2>&1
zeroclaw agent --help 2>&1

# 3. Examine transcript storage format
find ~/.zeroclaw -name "*.jsonl" -o -name "*.json" 2>/dev/null | head -10
ls -la ~/.zeroclaw/workspace/

# 4. Document the delta between openclaw and zeroclaw CLI/API
```

### Phase 2: Build Adapter (1-2 hours)

Based on Phase 1 findings, implement either Option A (shim) or Option B (fork).

### Phase 3: Sanity Test (15 min)

```bash
cd pinchbench

# Run ONLY the sanity check task first
./scripts/run.sh --model openrouter/deepseek/deepseek-v3.2 \
  --suite task_00_sanity \
  --no-upload

# Expected: score > 0 means the pipeline works end to end
# If score = 0: check adapter, transcript format, API connection
```

### Phase 4: Automated-Only Suite (30 min)

```bash
# Run tasks that use automated grading only (no LLM judge cost)
./scripts/run.sh --model openrouter/deepseek/deepseek-v3.2 \
  --suite automated-only \
  --no-upload

# Review results
cat results/latest/*.json | python3 -m json.tool
```

### Phase 5: Full Suite (1-2 hours)

```bash
# Full 23-task run with LLM judge
# NOTE: Judge model costs extra tokens — budget ~$1-3 for judging
./scripts/run.sh --model openrouter/deepseek/deepseek-v3.2 \
  --judge openrouter/anthropic/claude-opus-4.5 \
  --runs 3 \
  --timeout-multiplier 2 \
  --no-upload

# Multiple runs (--runs 3) give mean + stddev for reliability scoring
```

### Phase 6: Compare Models (optional, 2-4 hours)

```bash
# Run the same suite across multiple models for comparison
for model in \
  "openrouter/deepseek/deepseek-v3.2" \
  "openrouter/google/gemini-2.0-flash" \
  "openrouter/qwen/qwen3.5-27b" \
  "openrouter/anthropic/claude-sonnet-4.6"; do

  echo "=== Testing: $model ==="
  ./scripts/run.sh --model "$model" --runs 2 --no-upload
done

# Compare results
python3 -c "
import json, glob
for f in sorted(glob.glob('results/**/summary.json', recursive=True)):
    data = json.load(open(f))
    print(f'{data[\"model\"]:50s} success={data[\"success_rate\"]:.1%} score={data[\"avg_score\"]:.1%} cost=\${data[\"total_cost\"]:.4f}')
"
```

---

## 6. What to Measure

### Primary metrics (from PinchBench output)

| Metric | What it tells you |
|--------|-------------------|
| **Success rate** | % of tasks completed (binary pass/fail) |
| **Average score** | Quality of completions (0.0–1.0 per task) |
| **Total cost** | API spend for full suite |
| **Cost per task** | Average API cost per task |
| **Score per dollar** | Efficiency: quality / cost |
| **Score per 1K tokens** | How efficiently the model uses context |
| **Execution time** | Wall-clock per task |

### ZeroClaw-specific metrics (measure manually)

| Metric | How to measure |
|--------|---------------|
| **Container memory during benchmark** | `docker stats zeroclaw-minimal --no-stream` (sample every 30s) |
| **Gateway response latency** | Add timing to adapter: `time curl ...` |
| **Tool loop iterations per task** | Parse from ZeroClaw logs: `docker logs zeroclaw-minimal \| grep "tool_loop"` |
| **Token overhead vs raw model** | Compare PinchBench token count to OpenRouter dashboard |

---

## 7. Expected Results

Based on our test session ($0.00646/request with DeepSeek V3.2):

| Prediction | Value | Reasoning |
|------------|-------|-----------|
| Sanity task | 100% | Any response passes |
| Success rate | 70-80% | DeepSeek V3.2 is capable but not frontier |
| Average score | 65-75% | Lower than Claude/GPT on nuanced tasks |
| Total cost (23 tasks) | ~$0.15-0.30 | At ~$0.007-0.013 per task |
| Execution time | 2-5 min per task | Network + LLM latency dominates |

If DeepSeek scores below 70% success rate, consider re-running with Claude Sonnet 4.6 (~$0.50-1.00 for full suite) to isolate whether the bottleneck is the model or the runtime.

---

## 8. Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| CLI interface mismatch too large for shim | Fall back to Option B (gateway API fork) |
| Transcript format incompatible | Write converter script; validate against `task_00_sanity` first |
| Judge model costs more than expected | Use `--suite automated-only` to skip LLM-judged tasks initially |
| ZeroClaw tool names differ from OpenClaw | Map tool names in adapter (e.g., `shell_execute` → `shell`) |
| Timeout too short for complex tasks | Use `--timeout-multiplier 2` or `3` |
| Docker container OOMs during heavy tasks | Monitor with `docker stats`; raise limit to 512M if needed |

---

## 9. Handoff to VS Code Claude

Tell your VS Code Claude:

> Read PINCHBENCH_PLAN.md. Start with Phase 1 (reconnaissance) — check the ZeroClaw gateway API surface and CLI, then determine whether Option A (shim) or Option B (fork) is the better adapter strategy. Run the sanity test before anything else.

---

## 10. Cleanup

Delete this file after evaluation is complete or transferred to an issue tracker.
