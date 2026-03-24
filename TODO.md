# ZeroClaw — Refactoring TODO

**Created:** 2026-03-23
**Context:** Architecture evaluation found 10 improvement areas. Organized by priority, with test plans for each.
**Rule:** One concern per PR. Do not mix refactoring with feature work.

---

## Priority Legend

| Priority | Meaning |
|----------|---------|
| P0 | Actively causes bugs or panics in production |
| P1 | High maintenance cost or architectural risk |
| P2 | Code quality / correctness improvement |
| P3 | Nice-to-have, do when convenient |

---

## 1. [P0] Fix Production `.unwrap()` Panics

**Risk tier:** High
**Estimated size:** S (per file), M (total)

Several `.unwrap()` calls in production code paths can panic on malformed input.

### 1.1 `src/channels/matrix.rs:606`

```rust
// CURRENT — panics on malformed recipient
message.recipient.split_once("||").unwrap().1.to_string()
```

**Fix:** Replace with pattern match or `.ok_or_else()` returning a channel error.

### 1.2 `src/tools/security_ops.rs:464,482,485,489`

```rust
// CURRENT — panics if subprocess returns non-JSON
serde_json::from_str(&output).unwrap()
```

**Fix:** Replace with `.map_err(|e| anyhow!("Failed to parse tool output as JSON: {e}"))`.

### 1.3 `src/channels/mattermost.rs:226,369,683`

```rust
// CURRENT — panics if JSON value isn't an object
value.as_object_mut().unwrap()
```

**Fix:** Replace with match or `.ok_or_else()`.

### Test plan

```bash
# 1. Ensure no regressions in existing tests
cargo test --lib -- matrix mattermost security_ops

# 2. Add targeted unit tests for each fix
#    - matrix: test with recipient missing "||" separator
#    - security_ops: test with non-JSON subprocess output (e.g. "command not found")
#    - mattermost: test with JSON value that is an array instead of object

# 3. Grep to verify no remaining high-risk unwraps
#    (exclude test modules, Regex::new on literals, and Option::unwrap in builders)
grep -rn '\.unwrap()' src/ --include='*.rs' \
  | grep -v '#\[cfg(test)\]' \
  | grep -v 'mod tests' \
  | grep -v 'Regex::new' \
  | grep -v '_test\.rs' \
  | head -50
# Review output manually for production-path panics.
```

---

## 2. [P1] Decompose Agent Loop (`src/agent/loop_.rs`)

**Risk tier:** High (8,661 lines, primary maintenance bottleneck)
**Estimated size:** L (split across 5-6 PRs)

The agent loop is a god object containing ~9 interleaved concerns. Split into focused modules. Each sub-task below is one PR.

### 2.1 Extract `src/agent/cost_tracking.rs`

**What moves:**

- `ToolLoopCostTrackingContext` struct + `task_local!` declaration (lines 29–55)
- `lookup_model_pricing()` (lines 56–69)
- `record_tool_loop_cost_usage()` (lines 73–118)
- `record_cost_for_streaming()` (lines 119–148)
- `check_tool_loop_budget()` (lines 150–173)

**Interface:** Public functions, same signatures. The task-local stays in this module; callers access it through the public API.

**Test plan:**

```bash
# 1. Move tests that exercise cost tracking into the new module's test block
# 2. Run full agent test suite — cost tracking is exercised by tool loop tests
cargo test --lib -- agent::cost_tracking
cargo test --lib -- agent::loop_
cargo test --test test_component

# 3. Verify the task-local works across async boundaries
#    Write a test that spawns a tokio task, sets cost context, runs a tool, verifies cost recorded.
```

### 2.2 Extract `src/agent/tool_call_parser.rs`

**What moves:**

- All `parse_*_tool_calls()` functions (lines 686–2068, ~1,400 lines)
- `parse_tool_calls()` main dispatcher
- Supporting types: `ParsedToolCall`, `ToolCallParseMode`
- `scrub_credentials()` and `SENSITIVE_KV_REGEX` (lines 319–376)

**Interface:**

```rust
pub fn parse_tool_calls(
    text: &str,
    native_calls: &[NativeToolCall],
    available_tools: &[ToolSpec],
    mode: ToolCallParseMode,
) -> Vec<ParsedToolCall>
```

**Test plan:**

```bash
# 1. This module has the most existing unit tests — move them all
cargo test --lib -- agent::tool_call_parser

# 2. Run the full parser test battery (these are the ~200 edge case tests)
cargo test --lib -- parse_tool_calls
cargo test --lib -- parse_xml
cargo test --lib -- parse_json
cargo test --lib -- scrub_credentials

# 3. Integration: ensure agent_turn() still works end-to-end
cargo test --test test_component -- agent
```

### 2.3 Extract `src/agent/history.rs`

**What moves:**

- `estimate_history_tokens()` (line 378)
- `trim_history()` (line 447)
- `build_compaction_transcript()` (line 465)
- `apply_compaction_summary()` (line 479)
- `auto_compact_history()` (line 489)
- `load_interactive_session_history()` (line 566)
- `save_interactive_session_history()` (line 582)

**Interface:** Functions that take `&mut Vec<ChatMessage>` and return `Result<()>`.

**Test plan:**

```bash
# 1. Unit tests for compaction edge cases
cargo test --lib -- agent::history

# 2. Verify interactive session save/load round-trips
cargo test --lib -- session_history

# 3. Integration: long conversation compaction still triggers correctly
cargo test --test test_component -- compaction
```

### 2.4 Extract `src/agent/tool_execution.rs`

**What moves:**

- `execute_one_tool()` (line 2468)
- `should_execute_tools_in_parallel()` (line 2565)
- `execute_tools_parallel()` (line 2592)
- `execute_tools_sequential()` (line 2617)

**Test plan:**

```bash
cargo test --lib -- agent::tool_execution
cargo test --lib -- execute_tools_parallel
cargo test --lib -- execute_tools_sequential

# Verify parallel execution still respects concurrency limits
cargo test --test test_component -- parallel_tool
```

### 2.5 Extract `src/agent/display.rs`

**What moves:**

- `strip_think_tags()` (line 2070)
- `strip_tool_result_blocks()` (line 2092)
- `resolve_display_text()` (line 2255)
- `build_native_assistant_history()` (line 2160)
- `detect_tool_call_parse_issue()` (line 2113)

**Test plan:**

```bash
cargo test --lib -- agent::display
cargo test --lib -- strip_think
cargo test --lib -- resolve_display
```

### 2.6 Final: Slim down `loop_.rs` to orchestration only

After 2.1–2.5, `loop_.rs` should contain only:

- `agent_turn()` — public entry point
- `run_tool_call_loop()` — core loop (~300-400 lines, down from 900)
- `run()` — CLI orchestrator
- `process_message()` — gateway handler

**Verification for the full decomposition:**

```bash
# Full test suite must pass identically before and after
cargo test 2>&1 | tail -5    # capture before
# ... do refactoring ...
cargo test 2>&1 | tail -5    # capture after, diff the two

# Also run clippy — moved code must not introduce new warnings
cargo clippy --all-targets -- -D warnings

# Binary size should not change meaningfully (same code, different files)
ls -la target/release/zeroclaw
```

---

## 3. [P1] Introduce Typed Error Enums

**Risk tier:** Medium
**Estimated size:** M (incremental, module by module)

Replace `anyhow::Result` at module boundaries with domain-specific error enums. Keep `anyhow` internally for ad-hoc context.

### 3.1 Define error types

Create `src/errors.rs` (or per-module `error.rs` files):

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("network timeout after {0:?}")]
    Timeout(Duration),
    #[error("rate limited, retry after {0:?}")]
    RateLimited(Option<Duration>),
    #[error("authentication failed")]
    AuthFailed,
    #[error("model not found: {0}")]
    ModelNotFound(String),
    #[error("streaming not supported by {0}")]
    StreamingNotSupported(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("timeout after {0:?}")]
    Timeout(Duration),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryError { /* ... */ }

#[derive(Debug, thiserror::Error)]
pub enum ConfigError { /* ... */ }
```

### 3.2 Migrate incrementally

Order: Provider → Tool → Memory → Config → Agent. Each is a separate PR.

The `Other(#[from] anyhow::Error)` variant allows gradual migration — callsites that haven't been updated yet still compile.

### Test plan

```bash
# For each module migrated:
# 1. All existing tests must pass without changes (Other variant catches them)
cargo test --lib -- providers
cargo test --lib -- tools
cargo test --lib -- memory

# 2. Add new tests that verify specific error variants are returned
#    Example: ProviderError::Timeout on network failure
#    Example: ToolError::NotFound for unknown tool name

# 3. Verify callers can now pattern-match on errors
#    (add a test that matches on ProviderError::RateLimited and retries)

# 4. Grep for remaining anyhow::bail! in migrated modules — should be minimal
grep -rn 'anyhow::bail\|\.unwrap()' src/providers/ --include='*.rs' | grep -v test
```

---

## 4. [P1] Add Per-Tool Security Policy (RBAC)

**Risk tier:** High (security boundary change)
**Estimated size:** M

### 4.1 Current state

`src/security/policy.rs` has three autonomy levels (ReadOnly, Supervised, Full) and a `CommandRiskLevel` enum for shell commands only. No per-tool overrides exist.

### 4.2 Changes

Add to `SecurityPolicy`:

```rust
pub struct ToolPolicy {
    pub tool_name: String,
    pub allowed: bool,
    pub require_approval: bool,
    pub rate_limit_per_hour: Option<u32>,
    pub max_autonomy: AutonomyLevel,
}

// In SecurityPolicy:
pub tool_policies: HashMap<String, ToolPolicy>,

// New method:
pub fn can_execute_tool(&self, tool_name: &str, current_autonomy: AutonomyLevel) -> ToolPolicyResult {
    // Check tool_policies map, fall back to global autonomy level
}
```

Add to config schema:

```toml
[[security.tool_policies]]
tool = "shell_execute"
require_approval = true
max_autonomy = "supervised"
rate_limit_per_hour = 50

[[security.tool_policies]]
tool = "memory_store"
allowed = true
require_approval = false
```

### 4.3 Integration point

Modify `run_tool_call_loop()` (or the extracted `tool_execution.rs`) to call `policy.can_execute_tool()` before `execute_one_tool()`.

### Test plan

```bash
# 1. Unit tests for ToolPolicy resolution
cargo test --lib -- security::policy::tool_policy

# 2. Test cases:
#    - Tool with explicit policy: respects rate limit
#    - Tool without policy: falls back to global autonomy
#    - Tool blocked by policy: returns PermissionDenied
#    - Tool requiring approval at Full autonomy: still asks for approval
#    - Rate limit exceeded: returns appropriate error

# 3. Integration: agent loop respects tool policy
cargo test --test test_component -- tool_policy

# 4. Verify backward compatibility: empty tool_policies = current behavior
#    Load config with no [[security.tool_policies]] entries, run agent, verify all tools work.
cargo test --test test_integration -- default_policy

# 5. Security-specific: verify policy cannot be bypassed
#    - Policy with allowed=false cannot be overridden by autonomy level
#    - Rate limit cannot be reset by restarting tool loop
```

---

## 5. [P2] Config Validation at Load Time

**Risk tier:** Medium
**Estimated size:** S

### 5.1 Add `Config::validate()` post-deserialization

```rust
impl Config {
    pub fn validate(&self) -> Result<(), ConfigError> {
        // api_key required for non-ollama providers (or warn)
        if self.default_provider != "ollama" && self.api_key.as_deref().unwrap_or("").is_empty() {
            return Err(ConfigError::MissingApiKey { provider: self.default_provider.clone() });
        }

        // At least one memory backend configured
        if self.memory.is_none() || /* all sub-backends are None */ {
            return Err(ConfigError::NoMemoryBackend);
        }

        // Gateway port in valid range
        if let Some(gw) = &self.gateway {
            if gw.port == 0 { return Err(ConfigError::InvalidPort(0)); }
        }

        Ok(())
    }
}
```

### 5.2 Call `validate()` in config loading path

In `src/config/mod.rs` (or wherever `Config::load()` lives), call `config.validate()?` after deserialization.

### Test plan

```bash
# 1. Unit tests for each validation rule
cargo test --lib -- config::validate

# 2. Test cases:
#    - Valid config: passes
#    - Missing api_key with provider=openrouter: fails with MissingApiKey
#    - Missing api_key with provider=ollama: passes (Ollama uses URL as key)
#    - No memory backends: fails with NoMemoryBackend
#    - Port 0: fails with InvalidPort

# 3. Backward compat: existing config files still load
#    Copy dev/config.template.toml, load and validate, must pass
cargo test --lib -- config_template_valid

# 4. Integration: invalid config produces helpful error at startup, not a runtime panic
#    Create a config with empty api_key + provider=anthropic, run `zeroclaw doctor`, verify error message.
```

---

## 6. [P2] Optimize Memory Namespace Queries

**Risk tier:** Low
**Estimated size:** S

### 6.1 Current problem

`src/memory/traits.rs` default `recall_namespaced()` fetches `limit * 2` entries and filters client-side. Wastes I/O when namespace is sparse.

### 6.2 Fix

Add capability query to `Memory` trait:

```rust
trait Memory {
    // New: override to return true if backend handles namespace filtering natively
    fn supports_namespace_filter(&self) -> bool { false }

    // Existing: update default to check capability
    async fn recall_namespaced(&self, namespace: &str, ...) -> Result<Vec<MemoryEntry>> {
        if self.supports_namespace_filter() {
            // Backend handles it — call with exact limit
            self.recall_with_namespace(namespace, query, limit, ...).await
        } else {
            // Fallback: oversample and filter
            let entries = self.recall(query, limit * 2, ...).await?;
            Ok(entries.into_iter().filter(|e| e.namespace == namespace).take(limit).collect())
        }
    }

    // New: override in backends with native namespace support
    async fn recall_with_namespace(&self, namespace: &str, ...) -> Result<Vec<MemoryEntry>> {
        unimplemented!("backend claimed supports_namespace_filter but didn't implement recall_with_namespace")
    }
}
```

### 6.3 Override in SQLite backend

Add `WHERE namespace = ?` to the SQLite backend's query. Override `supports_namespace_filter()` to return `true`.

### Test plan

```bash
# 1. Default behavior unchanged for backends that don't override
cargo test --lib -- memory::traits

# 2. SQLite backend: verify namespace filter is pushed to SQL
cargo test --lib -- memory::sqlite -- namespace

# 3. Performance: compare query count before/after for sparse namespace
#    (manual or add a test that asserts recall() is NOT called when backend supports native filter)

# 4. Full suite
cargo test --lib -- memory
cargo test --test test_component -- memory
```

---

## 7. [P2] Tighten Lock Granularity

**Risk tier:** Low
**Estimated size:** S

### 7.1 `activated_tools` lock in tool loop

In `loop_.rs` (~line 2744), the `activated_tools` mutex is held while iterating over tool specs. Clone the specs under the lock instead:

```rust
// BEFORE
for spec in at.lock().unwrap().tool_specs() { ... }

// AFTER
let specs: Vec<ToolSpec> = at.lock().unwrap().tool_specs().to_vec();
for spec in specs { ... }
```

### 7.2 Document lock ordering

Add a comment block at the top of `src/security/pairing.rs` and `src/agent/loop_.rs` documenting which locks exist and their acquisition order:

```rust
// Lock ordering (acquire in this order to prevent deadlock):
// 1. failed_attempts
// 2. pairing_code
// 3. paired_tokens
// Never hold more than one at a time if possible.
```

### Test plan

```bash
# 1. Existing tests pass
cargo test --lib -- security::pairing
cargo test --lib -- agent

# 2. Add a stress test: spawn 50 concurrent tool executions, verify no deadlock
#    Use tokio::time::timeout to detect hangs
cargo test --lib -- lock_stress -- --ignored  # mark as #[ignore] for CI speed

# 3. Run under thread sanitizer (if available)
RUSTFLAGS="-Z sanitizer=thread" cargo test --lib -- pairing
```

---

## 8. [P2] Add Streaming Backpressure

**Risk tier:** Medium
**Estimated size:** S

### 8.1 Current problem

`Provider::stream_chat_with_system()` returns `Pin<Box<dyn Stream<Item = StreamResult<StreamChunk>>>>`. Callers must drain the stream — there's no backpressure, and slow consumers can cause unbounded memory growth if chunks accumulate.

### 8.2 Fix

Use a bounded `tokio::sync::mpsc` channel internally:

```rust
// In provider implementations, replace unbounded streams with:
let (tx, rx) = tokio::sync::mpsc::channel(32); // 32 chunks buffer

// Sender side: tx.send(chunk).await blocks if consumer is slow
// Consumer side: ReceiverStream::new(rx) implements Stream
```

This is an implementation change inside providers, not a trait change. The trait signature stays the same — the `Stream` it returns just happens to have bounded buffering now.

### Test plan

```bash
# 1. Existing streaming tests pass
cargo test --lib -- providers -- stream

# 2. Add test: slow consumer doesn't cause OOM
#    Create a provider that emits 10,000 chunks instantly.
#    Consumer sleeps 1ms between reads.
#    Verify memory stays bounded (use a counter on in-flight chunks).

# 3. Integration: gateway WebSocket streaming still works
cargo test --test test_component -- streaming
```

---

## 9. [P3] Add Property-Based Tests

**Risk tier:** Low
**Estimated size:** S

### 9.1 Where they'd help most

1. **Tool call parser** — arbitrary strings should never panic, should always return `Vec<ParsedToolCall>` (possibly empty)
2. **Config deserialization** — arbitrary TOML should either parse or return an error, never panic
3. **History compaction** — arbitrary message sequences should compact without losing the last N messages
4. **Credential scrubbing** — arbitrary strings with embedded keys should always be scrubbed

### 9.2 Implementation

Add `proptest` as a dev-dependency:

```toml
[dev-dependencies]
proptest = "1"
```

Example for tool call parser:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn parse_never_panics(input in "\\PC{0,5000}") {
        // Should return Ok or empty vec, never panic
        let _ = parse_tool_calls(&input, &[], &[], ToolCallParseMode::Auto);
    }

    #[test]
    fn scrub_never_panics(input in "\\PC{0,2000}") {
        let _ = scrub_credentials(&input);
    }
}
```

### Test plan

```bash
# 1. Run property tests (these are slow, use --release)
cargo test --release --lib -- proptest

# 2. If any failures found, minimize and add as regression unit tests
# 3. Add to CI as a separate job (optional, can be nightly-only)
```

---

## 10. [P3] Reduce Transitive Dependencies

**Risk tier:** Low
**Estimated size:** S per change

### 10.1 Audit

```bash
# Count current transitive deps
cargo tree --depth 1 | wc -l        # direct
cargo tree | wc -l                    # transitive (905 currently)

# Find heaviest subtrees
cargo tree --depth 2 --edges normal | sort | uniq -c | sort -rn | head -20
```

### 10.2 Candidates

- `matrix-sdk` (with E2EE) — behind feature gate, already gated
- Ensure `fantoccini` is behind `browser-native` feature (already is)
- Check if `image` crate can use fewer features (only JPEG/PNG needed)
- Check if `lettre` can drop unused transports

### Test plan

```bash
# 1. After each dep reduction, full build + test
cargo build --release --no-default-features
cargo test --no-default-features

# 2. Verify feature-gated builds still work
cargo build --release --features "ci-all"
cargo test --features "ci-all"

# 3. Compare binary size before/after
ls -la target/release/zeroclaw
```

---

## Execution Order

Recommended sequence (respects dependencies and risk):

```
Phase 1 — Fix breakage
  └─ #1  Fix production .unwrap() panics              [P0, size S]

Phase 2 — Structural refactoring
  ├─ #2.1 Extract cost_tracking.rs                    [P1, size S]
  ├─ #2.2 Extract tool_call_parser.rs                 [P1, size M]
  ├─ #2.3 Extract history.rs                          [P1, size S]
  ├─ #2.4 Extract tool_execution.rs                   [P1, size S]
  ├─ #2.5 Extract display.rs                          [P1, size S]
  └─ #2.6 Slim loop_.rs to orchestration              [P1, size S]

Phase 3 — Type safety
  └─ #3  Typed error enums (incremental per module)   [P1, size M]

Phase 4 — Security hardening
  ├─ #4  Per-tool RBAC policy                         [P1, size M]
  └─ #5  Config validation at load time               [P2, size S]

Phase 5 — Optimization & quality
  ├─ #6  Memory namespace query optimization          [P2, size S]
  ├─ #7  Lock granularity + documented ordering       [P2, size S]
  ├─ #8  Streaming backpressure                       [P2, size S]
  ├─ #9  Property-based tests                         [P3, size S]
  └─ #10 Dependency audit                             [P3, size S]
```

---

## Verification Gate (Run After Each PR)

```bash
# Minimum: must pass before merging any PR
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test

# Full validation (recommended for P0/P1 changes)
./dev/ci.sh all
```

---

## Cleanup

Delete this file after all tasks are complete or transferred to an issue tracker.
Delete `HANDOFF.md` after reading.
