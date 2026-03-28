use crate::config::schema::ModelPricing;
use crate::cost::types::{BudgetCheck, TokenUsage as CostTokenUsage};
use crate::cost::CostTracker;
use std::sync::Arc;

/// Context for cost tracking within the tool call loop.
/// Scoped via `tokio::task_local!` at call sites (channels, gateway).
#[derive(Clone)]
pub(crate) struct ToolLoopCostTrackingContext {
    pub tracker: Arc<CostTracker>,
    pub prices: Arc<std::collections::HashMap<String, ModelPricing>>,
}

impl ToolLoopCostTrackingContext {
    pub(crate) fn new(
        tracker: Arc<CostTracker>,
        prices: Arc<std::collections::HashMap<String, ModelPricing>>,
    ) -> Self {
        Self { tracker, prices }
    }
}

tokio::task_local! {
    pub(crate) static TOOL_LOOP_COST_TRACKING_CONTEXT: Option<ToolLoopCostTrackingContext>;
}

/// 3-tier model pricing lookup:
/// 1. Direct model name
/// 2. Qualified `provider/model`
/// 3. Suffix after last `/`
fn lookup_model_pricing<'a>(
    prices: &'a std::collections::HashMap<String, ModelPricing>,
    provider_name: &str,
    model: &str,
) -> Option<&'a ModelPricing> {
    prices
        .get(model)
        .or_else(|| prices.get(&format!("{provider_name}/{model}")))
        .or_else(|| {
            model
                .rsplit_once('/')
                .and_then(|(_, suffix)| prices.get(suffix))
        })
}

/// Record token usage from an LLM response via the task-local cost tracker.
/// Returns `(total_tokens, cost_usd)` on success, `None` when not scoped or no usage.
pub(crate) fn record_tool_loop_cost_usage(
    provider_name: &str,
    model: &str,
    usage: &crate::providers::traits::TokenUsage,
) -> Option<(u64, f64)> {
    let input_tokens = usage.input_tokens.unwrap_or(0);
    let output_tokens = usage.output_tokens.unwrap_or(0);
    let total_tokens = input_tokens.saturating_add(output_tokens);
    if total_tokens == 0 {
        return None;
    }

    let ctx = TOOL_LOOP_COST_TRACKING_CONTEXT
        .try_with(Clone::clone)
        .ok()
        .flatten()?;
    let pricing = lookup_model_pricing(&ctx.prices, provider_name, model);
    let cost_usage = CostTokenUsage::new(
        model,
        input_tokens,
        output_tokens,
        pricing.map_or(0.0, |entry| entry.input),
        pricing.map_or(0.0, |entry| entry.output),
    );

    if pricing.is_none() {
        tracing::debug!(
            provider = provider_name,
            model,
            "Cost tracking recorded token usage with zero pricing (no pricing entry found)"
        );
    }

    if let Err(error) = ctx.tracker.record_usage(cost_usage.clone()) {
        tracing::warn!(
            provider = provider_name,
            model,
            "Failed to record cost tracking usage: {error}"
        );
    }

    Some((cost_usage.total_tokens, cost_usage.cost_usd))
}

/// Record token usage via the global cost tracker (for streaming paths
/// like `Agent::turn_streamed` that bypass the task-local scoping).
pub(crate) fn record_cost_for_streaming(
    provider_name: &str,
    model: &str,
    usage: &crate::providers::traits::TokenUsage,
) {
    let input_tokens = usage.input_tokens.unwrap_or(0);
    let output_tokens = usage.output_tokens.unwrap_or(0);
    let total_tokens = input_tokens.saturating_add(output_tokens);
    if total_tokens == 0 {
        return;
    }

    // Use the global singleton tracker directly
    let tracker = match CostTracker::get_global() {
        Some(t) => t,
        None => return,
    };

    let pricing = lookup_model_pricing(tracker.prices(), provider_name, model);
    let cost_usage = CostTokenUsage::new(
        model,
        input_tokens,
        output_tokens,
        pricing.map_or(0.0, |entry| entry.input),
        pricing.map_or(0.0, |entry| entry.output),
    );

    if let Err(error) = tracker.record_usage(cost_usage) {
        tracing::warn!(
            provider = provider_name,
            model,
            "Failed to record streaming cost usage: {error}"
        );
    }
}

/// Check budget before an LLM call. Returns `None` when no cost tracking
/// context is scoped (tests, delegate, CLI without cost config).
pub(crate) fn check_tool_loop_budget() -> Option<BudgetCheck> {
    TOOL_LOOP_COST_TRACKING_CONTEXT
        .try_with(Clone::clone)
        .ok()
        .flatten()
        .map(|ctx| {
            ctx.tracker
                .check_budget(0.0)
                .unwrap_or(BudgetCheck::Allowed)
        })
}

/// Minimum characters per chunk when relaying LLM text to a streaming draft.
pub(crate) const STREAM_CHUNK_MIN_CHARS: usize = 80;
