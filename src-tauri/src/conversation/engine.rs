use std::error::Error;
use std::fmt;

use crate::db::chat::Chat;
use crate::db::types::{ImageContent, Message, MessageRole};

use super::payload::{build_openai_messages, format_tool_result};
use super::tool_parser::parse_tool_calls;
use super::types::{ConversationToolCall, ConversationToolContent, ConversationToolResult};

#[derive(Debug, Clone, PartialEq)]
pub struct AssistantRound {
    pub text: String,
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConversationEngineConfig {
    pub max_tool_rounds: usize,
}

impl Default for ConversationEngineConfig {
    fn default() -> Self {
        Self { max_tool_rounds: 10 }
    }
}

#[derive(Debug)]
pub enum ConversationEngineError {
    Chat(crate::db::types::ChatError),
    Llm(String),
    Tool(String),
    MaxToolRounds,
}

impl fmt::Display for ConversationEngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConversationEngineError::Chat(error) => write!(f, "chat error: {error}"),

            ConversationEngineError::Llm(error) => write!(f, "llm error: {error}"),
            ConversationEngineError::Tool(error) => write!(f, "tool error: {error}"),
            ConversationEngineError::MaxToolRounds => write!(f, "max tool rounds reached"),
        }
    }
}

impl Error for ConversationEngineError {}

impl From<crate::db::types::ChatError> for ConversationEngineError {
    fn from(value: crate::db::types::ChatError) -> Self {
        Self::Chat(value)
    }
}



pub trait ConversationLlm {
    fn next_round(&mut self, messages: &[Message]) -> Result<AssistantRound, ConversationEngineError>;
}

pub trait ConversationToolRunner {
    fn execute(
        &mut self,
        call: ConversationToolCall,
    ) -> Result<ConversationToolCall, ConversationEngineError>;
}

pub struct ConversationEngine<'a, L, T> {
    chat: &'a mut Chat,
    llm: L,
    tools: T,
    config: ConversationEngineConfig,
}

impl<'a, L, T> ConversationEngine<'a, L, T>
where
    L: ConversationLlm,
    T: ConversationToolRunner,
{
    pub fn new(
        chat: &'a mut Chat,
        llm: L,
        tools: T,
        config: ConversationEngineConfig,
    ) -> Self {
        Self { chat, llm, tools, config }
    }

    pub fn send_user_message(
        &mut self,
        conversation_id: &str,
        parent_message_id: Option<&str>,
        user_message_id: &str,
        text: &str,
        images: Option<Vec<ImageContent>>,
    ) -> Result<String, ConversationEngineError> {
        let images_json = images
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| ConversationEngineError::Chat(crate::db::types::ChatError::Conversation(
                crate::db::types::ConversationError::InvalidOperation(error.to_string()),
            )))?;

        self.chat.add_message(
            conversation_id,
            user_message_id,
            text,
            None,
            &MessageRole::User.to_string(),
            parent_message_id,
            images_json.as_deref(),
            None,
        )?;

        self.continue_from_leaf(conversation_id, user_message_id)
    }

    pub fn continue_from_leaf(
        &mut self,
        conversation_id: &str,
        leaf_message_id: &str,
    ) -> Result<String, ConversationEngineError> {
        let mut current_leaf_id = leaf_message_id.to_string();
        for round in 0..=self.config.max_tool_rounds {
            let path = self.chat.get_message_path_to(conversation_id, &current_leaf_id)?;
            build_openai_messages(&path);

            let assistant = self.llm.next_round(&path)?;
            let parsed = parse_tool_calls(&assistant.text);
            let clean_text = parsed.clean_text;
            let calls = parsed.calls;
            let assistant_message_id = format!("assistant-{current_leaf_id}-{round}");
            let tool_calls_json = if calls.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&calls).map_err(|error| {
                    ConversationEngineError::Chat(crate::db::types::ChatError::Conversation(
                        crate::db::types::ConversationError::InvalidOperation(error.to_string()),
                    ))
                })?)
            };

            self.chat.add_message(
                conversation_id,
                &assistant_message_id,
                &clean_text,
                assistant.reasoning.as_deref(),
                &MessageRole::Assistant.to_string(),
                Some(&current_leaf_id),
                None,
                tool_calls_json.as_deref(),
            )?;

            if calls.is_empty() {
                return Ok(assistant_message_id);
            }

            if round == self.config.max_tool_rounds {
                return Err(ConversationEngineError::MaxToolRounds);
            }

            let mut completed_calls = Vec::new();
            for call in calls {
                completed_calls.push(self.tools.execute(call)?);
            }
            let completed_json = serde_json::to_string(&completed_calls).map_err(|error| {
                ConversationEngineError::Chat(crate::db::types::ChatError::Conversation(
                    crate::db::types::ConversationError::InvalidOperation(error.to_string()),
                ))
            })?;
            self.chat
                .messages_manager
                .update_tool_calls(&assistant_message_id, &completed_json)
                .map_err(crate::db::types::ChatError::from)?;

            for call in &completed_calls {
                let tool_result_text = format_tool_result(call);
                let tool_message_id = format!("tool-{}-{}", assistant_message_id, call.id);
                self.chat.add_message(
                    conversation_id,
                    &tool_message_id,
                    &tool_result_text,
                    None,
                    &MessageRole::Tool.to_string(),
                    Some(&assistant_message_id),
                    None,
                    None,
                )?;
                current_leaf_id = tool_message_id;
            }
        }

        Err(ConversationEngineError::MaxToolRounds)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::conversation::types::{ConversationToolContent, ConversationToolResult};
    use crate::db::create_memory_pool;

    struct FakeLlm {
        rounds: VecDeque<AssistantRound>,
        seen_roles: Vec<Vec<MessageRole>>,
    }

    impl FakeLlm {
        fn new(rounds: Vec<AssistantRound>) -> Self {
            Self { rounds: VecDeque::from(rounds), seen_roles: Vec::new() }
        }
    }

    impl ConversationLlm for FakeLlm {
        fn next_round(&mut self, messages: &[Message]) -> Result<AssistantRound, ConversationEngineError> {
            self.seen_roles.push(messages.iter().map(|message| message.sender.clone()).collect());
            self.rounds.pop_front().ok_or_else(|| ConversationEngineError::Llm("no more rounds".to_string()))
        }
    }

    struct FakeTools;

    impl ConversationToolRunner for FakeTools {
        fn execute(&mut self, call: ConversationToolCall) -> Result<ConversationToolCall, ConversationEngineError> {
            Ok(ConversationToolCall {
                result: Some(ConversationToolResult {
                    content: vec![ConversationToolContent::Text { text: "tool says hi".to_string() }],
                    is_error: false,
                }),
                ..call
            })
        }
    }

    fn chat() -> Chat {
        Chat::new_with_pool(create_memory_pool()).expect("chat")
    }

    #[test]
    fn simple_text_round_persists_user_and_assistant() {
        let mut chat = chat();
        chat.create_conversation("c1", "Conversation", "desc").expect("conversation");
        let llm = FakeLlm::new(vec![AssistantRound { text: "hello".to_string(), reasoning: None }]);
        let tools = FakeTools;
        let mut engine = ConversationEngine::new(&mut chat, llm, tools, ConversationEngineConfig::default());

        let leaf = engine
            .send_user_message("c1", None, "u1", "hi", None)
            .expect("send succeeds");

        assert_eq!(leaf, "assistant-u1-0");
        let path = engine.chat.get_message_path_to("c1", &leaf).expect("path");
        assert_eq!(path.iter().map(|message| message.sender.clone()).collect::<Vec<_>>(), vec![MessageRole::User, MessageRole::Assistant]);
        assert_eq!(path[1].text, "hello");
    }

    #[test]
    fn tool_round_persists_assistant_tool_call_tool_result_and_final_assistant() {
        let mut chat = chat();
        chat.create_conversation("c1", "Conversation", "desc").expect("conversation");
        let llm = FakeLlm::new(vec![
            AssistantRound {
                text: "<|tool_calls|>[{\"id\":\"call_1\",\"name\":\"server:tool\",\"arguments\":{}}]<|/tool_calls|>".to_string(),
                reasoning: None,
            },
            AssistantRound { text: "final answer".to_string(), reasoning: None },
        ]);
        let tools = FakeTools;
        let mut engine = ConversationEngine::new(&mut chat, llm, tools, ConversationEngineConfig::default());

        let leaf = engine
            .send_user_message("c1", None, "u1", "use tool", None)
            .expect("send succeeds");

        assert_eq!(leaf, "assistant-tool-assistant-u1-0-call_1-1");
        let path = engine.chat.get_message_path_to("c1", &leaf).expect("path");
        assert_eq!(path.iter().map(|message| message.sender.clone()).collect::<Vec<_>>(), vec![
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::Tool,
            MessageRole::Assistant,
        ]);
        assert!(path[1].tool_calls.as_ref().expect("tool calls").contains("tool says hi"));
        assert!(path[2].text.contains("tool says hi"));
        assert_eq!(path[3].text, "final answer");
    }

    #[test]
    fn max_tool_rounds_stops_infinite_tool_loop() {
        let mut chat = chat();
        chat.create_conversation("c1", "Conversation", "desc").expect("conversation");
        let llm = FakeLlm::new(vec![
            AssistantRound {
                text: "<|tool_calls|>[{\"id\":\"call_1\",\"name\":\"server:tool\",\"arguments\":{}}]<|/tool_calls|>".to_string(),
                reasoning: None,
            },
        ]);
        let tools = FakeTools;
        let mut engine = ConversationEngine::new(&mut chat, llm, tools, ConversationEngineConfig { max_tool_rounds: 0 });

        let err = engine
            .send_user_message("c1", None, "u1", "loop", None)
            .expect_err("max rounds reached");

        assert!(matches!(err, ConversationEngineError::MaxToolRounds));
    }
}
