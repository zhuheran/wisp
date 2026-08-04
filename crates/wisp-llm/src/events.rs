//! Streaming event mapping and the cancellation-aware drain loop.
//!
//! rig already aggregates streamed tool-call deltas into complete
//! `ToolCall`s on `StreamingCompletionResponse.choice`, so this module only
//! maps text/reasoning events; tool-call events are deliberately ignored here
//! (the aggregation happens on the rig side and is surfaced by `stream()`).

use futures::StreamExt;
use rig_core::completion::CompletionError;
use rig_core::streaming::StreamedAssistantContent;
use tokio_util::sync::CancellationToken;

use crate::backend::{StreamCallbacks, StreamOutcome};
use crate::error::{LlmError, map_completion_error};

/// Map one streamed assistant event onto the outcome. Tool-call events are
/// ignored: rig aggregates them into `StreamingCompletionResponse.choice`.
pub fn handle_stream_event<R>(
    event: StreamedAssistantContent<R>,
    outcome: &mut StreamOutcome,
    callbacks: &StreamCallbacks,
) {
    match event {
        StreamedAssistantContent::Text(text) => {
            outcome.text.push_str(&text.text);
            (callbacks.on_content)(&text.text);
        },
        StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
            outcome.reasoning.push_str(&reasoning);
            (callbacks.on_reasoning)(&reasoning);
        },
        StreamedAssistantContent::Reasoning(reasoning) => {
            let display = reasoning.display_text();
            if !display.is_empty() {
                outcome.reasoning.push_str(&display);
                (callbacks.on_reasoning)(&display);
            }
        },
        // Tool-call deltas and complete calls are aggregated by rig on
        // `StreamingCompletionResponse.choice` — rebuilding them here would
        // double-count arguments when concatenated.
        StreamedAssistantContent::ToolCallDelta { .. }
        | StreamedAssistantContent::ToolCall { .. }
        | StreamedAssistantContent::Final(_)
        | StreamedAssistantContent::Unknown(_) => {},
    }
}

/// Drain a stream until it ends or the cancellation token fires.
///
/// On cancellation the underlying stream is no longer polled (dropping it
/// aborts the in-flight request), partial content is preserved and
/// `outcome.cancelled` is set — cancellation never surfaces as an `Err`.
pub async fn drain_stream<S, R>(
    stream: &mut S,
    cancel: CancellationToken,
    callbacks: &StreamCallbacks,
) -> Result<StreamOutcome, LlmError>
where
    S: futures::Stream<Item = Result<StreamedAssistantContent<R>, CompletionError>> + Unpin,
{
    let mut outcome = StreamOutcome::default();

    let mut cancelled = false;
    loop {
        let item = tokio::select! {
            _ = cancel.cancelled() => {
                cancelled = true;
                break;
            }
            item = stream.next() => item,
        };
        let Some(item) = item else { break };
        let event = item.map_err(map_completion_error)?;
        handle_stream_event(event, &mut outcome, callbacks);
    }
    outcome.cancelled = cancelled;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::ChunkCallback;
    use rig_core::message::{ToolCall, ToolFunction};
    use rig_core::streaming::StreamedAssistantContent;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn captured_callbacks() -> (StreamCallbacks, Arc<Mutex<Vec<String>>>, Arc<Mutex<Vec<String>>>) {
        let content_log = Arc::new(Mutex::new(Vec::new()));
        let reasoning_log = Arc::new(Mutex::new(Vec::new()));
        let on_content: ChunkCallback = {
            let log = content_log.clone();
            Arc::new(move |chunk: &str| log.lock().unwrap().push(chunk.to_string()))
        };
        let on_reasoning: ChunkCallback = {
            let log = reasoning_log.clone();
            Arc::new(move |chunk: &str| log.lock().unwrap().push(chunk.to_string()))
        };
        (StreamCallbacks { on_content, on_reasoning }, content_log, reasoning_log)
    }

    fn noop_callbacks() -> StreamCallbacks {
        StreamCallbacks {
            on_content: Arc::new(|_| {}),
            on_reasoning: Arc::new(|_| {}),
        }
    }

    #[test]
    fn text_event_appends_text_and_invokes_callback() {
        let (callbacks, content_log, _) = captured_callbacks();
        let mut outcome = StreamOutcome::default();
        handle_stream_event(
            StreamedAssistantContent::<()>::text("hello "),
            &mut outcome,
            &callbacks,
        );
        handle_stream_event(
            StreamedAssistantContent::<()>::text("world"),
            &mut outcome,
            &callbacks,
        );
        assert_eq!(outcome.text, "hello world");
        assert_eq!(*content_log.lock().unwrap(), vec!["hello ", "world"]);
    }

    #[test]
    fn reasoning_delta_appends_and_invokes_callback() {
        let (callbacks, _, reasoning_log) = captured_callbacks();
        let mut outcome = StreamOutcome::default();
        handle_stream_event(
            StreamedAssistantContent::<()>::ReasoningDelta {
                id: None,
                reasoning: "think".to_string(),
            },
            &mut outcome,
            &callbacks,
        );
        assert_eq!(outcome.reasoning, "think");
        assert_eq!(*reasoning_log.lock().unwrap(), vec!["think"]);
    }

    #[test]
    fn full_reasoning_block_appends_display_text() {
        let (callbacks, _, reasoning_log) = captured_callbacks();
        let mut outcome = StreamOutcome::default();
        handle_stream_event(
            StreamedAssistantContent::<()>::Reasoning(rig_core::message::Reasoning::new("full")),
            &mut outcome,
            &callbacks,
        );
        assert_eq!(outcome.reasoning, "full");
        assert_eq!(*reasoning_log.lock().unwrap(), vec!["full"]);
    }

    #[test]
    fn tool_call_events_are_ignored_in_stream_mapping() {
        // rig aggregates tool calls on StreamingCompletionResponse.choice;
        // the stream mapping must not rebuild them (double-counting would
        // corrupt arguments when concatenated).
        let callbacks = noop_callbacks();
        let mut outcome = StreamOutcome::default();
        let mut before = outcome.clone();
        handle_stream_event(
            StreamedAssistantContent::<()>::ToolCallDelta {
                id: "call_1".to_string(),
                internal_call_id: "rig-call_1".to_string(),
                content: rig_core::streaming::ToolCallDeltaContent::Name("search".to_string()),
            },
            &mut outcome,
            &callbacks,
        );
        handle_stream_event(
            StreamedAssistantContent::<()>::ToolCall {
                tool_call: ToolCall::new(
                    "call_1".to_string(),
                    ToolFunction {
                        name: "search".to_string(),
                        arguments: serde_json::json!({"q": "weather"}),
                    },
                ),
                internal_call_id: "rig-call_1".to_string(),
            },
            &mut outcome,
            &callbacks,
        );
        assert_eq!(outcome, before);
    }

    #[test]
    fn final_and_unknown_events_are_ignored() {
        let callbacks = noop_callbacks();
        let mut outcome = StreamOutcome::default();
        let before = outcome.clone();
        handle_stream_event(
            StreamedAssistantContent::<()>::final_response(()),
            &mut outcome,
            &callbacks,
        );
        handle_stream_event(
            StreamedAssistantContent::<()>::Unknown(serde_json::json!({"raw": true})),
            &mut outcome,
            &callbacks,
        );
        assert_eq!(outcome, before);
    }

    #[tokio::test]
    async fn drains_all_events_when_not_cancelled() {
        let mut stream = futures::stream::iter(vec![
            Ok(StreamedAssistantContent::<()>::text("hello ")),
            Ok(StreamedAssistantContent::<()>::text("world")),
        ]);
        let outcome = drain_stream(&mut stream, CancellationToken::new(), &noop_callbacks())
            .await
            .unwrap();
        assert!(!outcome.cancelled);
        assert_eq!(outcome.text, "hello world");
    }

    #[tokio::test]
    async fn cancellation_returns_partial_outcome_not_error() {
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            cancel_for_task.cancel();
        });
        let mut stream = futures::stream::iter(vec![Ok(StreamedAssistantContent::<()>::text("hello"))])
            .chain(futures::stream::pending::<Result<
                StreamedAssistantContent<()>,
                CompletionError,
            >>());
        let outcome = drain_stream(&mut stream, cancel, &noop_callbacks()).await.unwrap();
        assert!(outcome.cancelled, "cancellation must not be an error");
        assert_eq!(outcome.text, "hello");
    }

    #[tokio::test]
    async fn cancellation_after_all_events_keeps_full_text() {
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            cancel_for_task.cancel();
        });
        let mut stream = futures::stream::iter(vec![
            Ok(StreamedAssistantContent::<()>::text("a")),
            Ok(StreamedAssistantContent::<()>::text("b")),
            Ok(StreamedAssistantContent::<()>::text("c")),
        ])
        .chain(futures::stream::pending::<Result<
            StreamedAssistantContent<()>,
            CompletionError,
        >>());
        let outcome = drain_stream(&mut stream, cancel, &noop_callbacks()).await.unwrap();
        assert!(outcome.cancelled);
        assert_eq!(outcome.text, "abc");
    }
}
