use super::loop_::ToolLoopCancelled;
use super::tool_filter::scrub_credentials;
use super::tool_parsing::{find_tool, ParsedToolCall};
use crate::approval::ApprovalManager;
use crate::observability::{Observer, ObserverEvent};
use crate::tools::Tool;
use crate::util::truncate_with_ellipsis;
use anyhow::Result;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Tool execution outcome containing output, success status, error details, and duration.
pub(crate) struct ToolExecutionOutcome {
    pub output: String,
    pub success: bool,
    pub error_reason: Option<String>,
    pub duration: Duration,
}

/// Injects default channel delivery configuration for cron_add jobs when called from a channel.
///
/// This ensures that agent jobs created via channel commands automatically deliver their results
/// back to the originating channel and target (e.g., thread ID, user ID).
pub(crate) fn maybe_inject_channel_delivery_defaults(
    tool_name: &str,
    tool_args: &mut serde_json::Value,
    channel_name: &str,
    channel_reply_target: Option<&str>,
) {
    if tool_name != "cron_add" {
        return;
    }

    if !matches!(
        channel_name,
        "telegram" | "discord" | "slack" | "mattermost" | "matrix"
    ) {
        return;
    }

    let Some(reply_target) = channel_reply_target
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };

    let Some(args) = tool_args.as_object_mut() else {
        return;
    };

    let is_agent_job = args
        .get("job_type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|job_type| job_type.eq_ignore_ascii_case("agent"))
        || args
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|prompt| !prompt.trim().is_empty());
    if !is_agent_job {
        return;
    }

    let default_delivery = || {
        serde_json::json!({
            "mode": "announce",
            "channel": channel_name,
            "to": reply_target,
        })
    };

    match args.get_mut("delivery") {
        None => {
            args.insert("delivery".to_string(), default_delivery());
        }
        Some(serde_json::Value::Null) => {
            *args.get_mut("delivery").expect("delivery key exists") = default_delivery();
        }
        Some(serde_json::Value::Object(delivery)) => {
            if delivery
                .get("mode")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|mode| mode.eq_ignore_ascii_case("none"))
            {
                return;
            }

            delivery
                .entry("mode".to_string())
                .or_insert_with(|| serde_json::Value::String("announce".to_string()));

            let needs_channel = delivery
                .get("channel")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|value| value.trim().is_empty());
            if needs_channel {
                delivery.insert(
                    "channel".to_string(),
                    serde_json::Value::String(channel_name.to_string()),
                );
            }

            let needs_target = delivery
                .get("to")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|value| value.trim().is_empty());
            if needs_target {
                delivery.insert(
                    "to".to_string(),
                    serde_json::Value::String(reply_target.to_string()),
                );
            }
        }
        Some(_) => {}
    }
}

/// Executes a single tool call and returns the outcome.
///
/// Handles tool lookup, execution, result capture, and credential scrubbing. Respects
/// cancellation tokens for graceful shutdown.
pub(crate) async fn execute_one_tool(
    call_name: &str,
    call_arguments: serde_json::Value,
    tools_registry: &[Box<dyn Tool>],
    activated_tools: Option<&std::sync::Arc<std::sync::Mutex<crate::tools::ActivatedToolSet>>>,
    observer: &dyn Observer,
    cancellation_token: Option<&CancellationToken>,
) -> Result<ToolExecutionOutcome> {
    let args_summary = truncate_with_ellipsis(&call_arguments.to_string(), 300);
    observer.record_event(&ObserverEvent::ToolCallStart {
        tool: call_name.to_string(),
        arguments: Some(args_summary),
    });
    let start = Instant::now();

    let static_tool = find_tool(tools_registry, call_name);
    let activated_arc = if static_tool.is_none() {
        activated_tools.and_then(|at| at.lock().unwrap().get_resolved(call_name))
    } else {
        None
    };
    let Some(tool) = static_tool.or(activated_arc.as_deref()) else {
        let reason = format!("Unknown tool: {call_name}");
        let duration = start.elapsed();
        observer.record_event(&ObserverEvent::ToolCall {
            tool: call_name.to_string(),
            duration,
            success: false,
        });
        return Ok(ToolExecutionOutcome {
            output: reason.clone(),
            success: false,
            error_reason: Some(scrub_credentials(&reason)),
            duration,
        });
    };

    let tool_future = tool.execute(call_arguments);
    let tool_result = if let Some(token) = cancellation_token {
        tokio::select! {
            () = token.cancelled() => return Err(ToolLoopCancelled.into()),
            result = tool_future => result,
        }
    } else {
        tool_future.await
    };

    match tool_result {
        Ok(r) => {
            let duration = start.elapsed();
            observer.record_event(&ObserverEvent::ToolCall {
                tool: call_name.to_string(),
                duration,
                success: r.success,
            });
            if r.success {
                Ok(ToolExecutionOutcome {
                    output: scrub_credentials(&r.output),
                    success: true,
                    error_reason: None,
                    duration,
                })
            } else {
                let reason = r.error.unwrap_or(r.output);
                Ok(ToolExecutionOutcome {
                    output: format!("Error: {reason}"),
                    success: false,
                    error_reason: Some(scrub_credentials(&reason)),
                    duration,
                })
            }
        }
        Err(e) => {
            let duration = start.elapsed();
            observer.record_event(&ObserverEvent::ToolCall {
                tool: call_name.to_string(),
                duration,
                success: false,
            });
            let reason = format!("Error executing {call_name}: {e}");
            Ok(ToolExecutionOutcome {
                output: reason.clone(),
                success: false,
                error_reason: Some(scrub_credentials(&reason)),
                duration,
            })
        }
    }
}

/// Determines whether tool calls should be executed in parallel.
///
/// Returns false (sequential) if:
/// - There's only one or fewer tool calls
/// - tool_search is in the batch (deferred MCP activation race condition)
/// - Any tool requires approval (for consistent CLI prompt policy)
pub(crate) fn should_execute_tools_in_parallel(
    tool_calls: &[ParsedToolCall],
    approval: Option<&ApprovalManager>,
) -> bool {
    if tool_calls.len() <= 1 {
        return false;
    }

    // tool_search activates deferred MCP tools into ActivatedToolSet.
    // Running tool_search in parallel with the tools it activates causes a
    // race condition where the tool lookup happens before activation completes.
    // Force sequential execution whenever tool_search is in the batch.
    if tool_calls.iter().any(|call| call.name == "tool_search") {
        return false;
    }

    if let Some(mgr) = approval {
        if tool_calls.iter().any(|call| mgr.needs_approval(&call.name)) {
            // Approval-gated calls must keep sequential handling so the caller can
            // enforce CLI prompt/deny policy consistently.
            return false;
        }
    }

    true
}

/// Executes multiple tool calls concurrently using tokio::join_all.
pub(crate) async fn execute_tools_parallel(
    tool_calls: &[ParsedToolCall],
    tools_registry: &[Box<dyn Tool>],
    activated_tools: Option<&std::sync::Arc<std::sync::Mutex<crate::tools::ActivatedToolSet>>>,
    observer: &dyn Observer,
    cancellation_token: Option<&CancellationToken>,
) -> Result<Vec<ToolExecutionOutcome>> {
    let futures: Vec<_> = tool_calls
        .iter()
        .map(|call| {
            execute_one_tool(
                &call.name,
                call.arguments.clone(),
                tools_registry,
                activated_tools,
                observer,
                cancellation_token,
            )
        })
        .collect();

    let results = futures_util::future::join_all(futures).await;
    results.into_iter().collect()
}

/// Executes multiple tool calls sequentially.
pub(crate) async fn execute_tools_sequential(
    tool_calls: &[ParsedToolCall],
    tools_registry: &[Box<dyn Tool>],
    activated_tools: Option<&std::sync::Arc<std::sync::Mutex<crate::tools::ActivatedToolSet>>>,
    observer: &dyn Observer,
    cancellation_token: Option<&CancellationToken>,
) -> Result<Vec<ToolExecutionOutcome>> {
    let mut outcomes = Vec::with_capacity(tool_calls.len());

    for call in tool_calls {
        outcomes.push(
            execute_one_tool(
                &call.name,
                call.arguments.clone(),
                tools_registry,
                activated_tools,
                observer,
                cancellation_token,
            )
            .await?,
        );
    }

    Ok(outcomes)
}
