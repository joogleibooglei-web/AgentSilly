//! Context builder — assembles the LLM prompt from system prompt, conversation history,
//! reference chunks, and tool definitions.
//!
//! Handles token counting via tiktoken-rs and truncation to stay within budget.

use crate::llm::{ChatMessage, ToolDefinition};

/// A chunk of reference document content for context injection.
#[derive(Debug, Clone)]
pub struct DocumentChunk {
    /// Source attribution (e.g., filename or document title).
    pub source: String,
    /// The text content of this chunk.
    pub content: String,
}

/// Assembles the chat-completion messages array for the LLM.
///
/// Combines the ENI personality system prompt, optional post-card prompt,
/// relevant reference document chunks, and conversation history. Enforces
/// a token budget by truncating oldest messages while preserving the system
/// prompt and the most recent messages.
pub struct ContextBuilder {
    /// ENI's base personality system prompt.
    system_prompt: String,
    /// User-editable post-card prompt appended after the personality.
    post_card_prompt: String,
    /// Maximum token budget for the assembled context.
    max_tokens: usize,
    /// Minimum number of recent messages to always preserve during truncation.
    preserve_recent: usize,
}

impl ContextBuilder {
    /// Create a new context builder.
    ///
    /// # Arguments
    /// - `system_prompt` — ENI's base personality prompt
    /// - `post_card_prompt` — User-editable addition (can be empty)
    /// - `max_tokens` — Token budget for the full context
    pub fn new(system_prompt: String, post_card_prompt: String, max_tokens: usize) -> Self {
        Self {
            system_prompt,
            post_card_prompt,
            max_tokens,
            preserve_recent: 4,
        }
    }

    /// Update the post-card prompt at runtime.
    pub fn set_post_card_prompt(&mut self, prompt: String) {
        self.post_card_prompt = prompt;
    }

    /// Update the system prompt at runtime.
    pub fn set_system_prompt(&mut self, prompt: String) {
        self.system_prompt = prompt;
    }

    /// Build the messages array for a chat completion request.
    ///
    /// Assembles: system message (personality + post-card + reference chunks)
    /// followed by conversation history, truncated to fit within the token budget.
    ///
    /// Post-tool instruction messages (system messages starting with `[System:`)
    /// are pruned so that only the most recent one is kept, preventing token bloat
    /// from accumulated tool guidance across many iterations.
    pub fn build_messages(
        &self,
        conversation: &[ChatMessage],
        relevant_chunks: &[DocumentChunk],
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // Build the system message content
        let system_message = self.build_system_content(relevant_chunks);
        messages.push(system_message.clone());

        // If no conversation history, return just the system message
        if conversation.is_empty() {
            return messages;
        }

        // Strip older post-tool instructions, keeping only the most recent one.
        let pruned_conversation = Self::prune_post_tool_instructions(conversation);

        // Truncate conversation to fit within token budget
        let system_tokens = count_message_tokens(&system_message);
        let remaining_budget = self.max_tokens.saturating_sub(system_tokens);

        let truncated = self.truncate_to_budget(&pruned_conversation, remaining_budget);
        messages.extend(truncated.into_iter().cloned());

        messages
    }

    /// Format tool definitions in the OpenAI function-calling format.
    ///
    /// Returns a Vec of `ToolDefinition` ready to be passed to the LLM client.
    /// This is a pass-through since our `ToolDefinition` type already matches
    /// the OpenAI format, but this method provides a place to add any
    /// additional formatting or filtering logic.
    pub fn format_tools(tools: &[ToolDefinition]) -> Vec<ToolDefinition> {
        tools.to_vec()
    }

    /// Remove all post-tool instruction system messages except the most recent one.
    ///
    /// Post-tool instructions are identified by being system-role messages whose
    /// content starts with `[System:`. This prevents token bloat from accumulating
    /// guidance messages across many tool-call iterations.
    fn prune_post_tool_instructions(conversation: &[ChatMessage]) -> Vec<ChatMessage> {
        // Find the index of the last post-tool instruction
        let last_instruction_idx = conversation
            .iter()
            .enumerate()
            .rev()
            .find(|(_, msg)| Self::is_post_tool_instruction(msg))
            .map(|(i, _)| i);

        match last_instruction_idx {
            Some(keep_idx) => {
                conversation
                    .iter()
                    .enumerate()
                    .filter(|(i, msg)| {
                        // Keep everything that isn't a post-tool instruction,
                        // plus the one we want to keep
                        *i == keep_idx || !Self::is_post_tool_instruction(msg)
                    })
                    .map(|(_, msg)| msg.clone())
                    .collect()
            }
            None => conversation.to_vec(),
        }
    }

    /// Check if a message is a post-tool instruction injected by the agent loop.
    fn is_post_tool_instruction(msg: &ChatMessage) -> bool {
        (msg.role == "system" || msg.role == "user")
            && msg
                .content
                .as_deref()
                .map(|c| c.starts_with("[System:"))
                .unwrap_or(false)
    }

    /// Build the system message content from personality + post-card + reference chunks.
    fn build_system_content(&self, relevant_chunks: &[DocumentChunk]) -> ChatMessage {
        let mut content = self.system_prompt.clone();

        if !self.post_card_prompt.is_empty() {
            content.push_str("\n\n");
            content.push_str(&self.post_card_prompt);
        }

        if !relevant_chunks.is_empty() {
            content.push_str("\n\n## Reference Context\n");
            for chunk in relevant_chunks {
                content.push_str(&format!("\n[{}]: {}\n", chunk.source, chunk.content));
            }
        }

        ChatMessage::system(content)
    }

    /// Truncate conversation history to fit within the given token budget.
    ///
    /// Strategy: always preserve the most recent `preserve_recent` messages.
    /// Remove oldest messages first until the total fits within budget.
    fn truncate_to_budget<'a>(
        &self,
        conversation: &'a [ChatMessage],
        budget: usize,
    ) -> Vec<&'a ChatMessage> {
        let len = conversation.len();

        // If we can fit everything, return all messages
        let total_tokens: usize = conversation.iter().map(|m| count_message_tokens(m)).sum();
        if total_tokens <= budget {
            return conversation.iter().collect();
        }

        // Always preserve the last `preserve_recent` messages
        let preserve_count = self.preserve_recent.min(len);
        let preserved_start = len.saturating_sub(preserve_count);

        // Calculate tokens used by preserved messages
        let preserved_tokens: usize = conversation[preserved_start..]
            .iter()
            .map(|m| count_message_tokens(m))
            .sum();

        // If preserved messages alone exceed budget, return only them (can't truncate further)
        if preserved_tokens >= budget {
            return conversation[preserved_start..].iter().collect();
        }

        // Fill remaining budget from oldest to newest (before the preserved section)
        let available_budget = budget - preserved_tokens;
        let mut included_from_start = Vec::new();
        let mut used_tokens = 0;

        for msg in &conversation[..preserved_start] {
            let msg_tokens = count_message_tokens(msg);
            if used_tokens + msg_tokens > available_budget {
                break;
            }
            included_from_start.push(msg);
            used_tokens += msg_tokens;
        }

        // If we couldn't include any older messages, just return preserved
        if included_from_start.is_empty() {
            return conversation[preserved_start..].iter().collect();
        }

        // Combine: included older messages + preserved recent messages
        let mut result = included_from_start;
        result.extend(conversation[preserved_start..].iter());
        result
    }
}

/// Count the approximate number of tokens in a chat message using tiktoken.
///
/// Uses the cl100k_base tokenizer (GPT-4 compatible). Adds overhead for
/// message framing (role, separators) per the OpenAI token counting docs.
pub fn count_message_tokens(message: &ChatMessage) -> usize {
    // Per OpenAI docs: every message has ~4 tokens of overhead (role, separators)
    const MESSAGE_OVERHEAD: usize = 4;

    let bpe = tiktoken_rs::cl100k_base().unwrap();

    let content_tokens = message
        .content
        .as_deref()
        .map(|c| bpe.encode_with_special_tokens(c).len())
        .unwrap_or(0);

    // Count tool_calls if present (for assistant messages with tool calls)
    let tool_call_tokens = message
        .tool_calls
        .as_ref()
        .map(|calls: &Vec<crate::llm::ChatToolCall>| {
            calls
                .iter()
                .map(|tc| {
                    let name_tokens = bpe.encode_with_special_tokens(&tc.function.name).len();
                    let args_tokens =
                        bpe.encode_with_special_tokens(&tc.function.arguments).len();
                    name_tokens + args_tokens + 3 // overhead for tool call structure
                })
                .sum::<usize>()
        })
        .unwrap_or(0);

    MESSAGE_OVERHEAD + content_tokens + tool_call_tokens
}

/// Count tokens in a plain string using tiktoken cl100k_base.
pub fn count_tokens(text: &str) -> usize {
    let bpe = tiktoken_rs::cl100k_base().unwrap();
    bpe.encode_with_special_tokens(text).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user_msg(content: &str) -> ChatMessage {
        ChatMessage::user(content)
    }

    fn make_assistant_msg(content: &str) -> ChatMessage {
        ChatMessage::assistant(content)
    }

    #[test]
    fn test_system_prompt_always_preserved() {
        let builder = ContextBuilder::new(
            "You are ENI, a creative writing assistant.".to_string(),
            String::new(),
            100, // very small budget
        );

        let conversation = vec![
            make_user_msg("Hello"),
            make_assistant_msg("Hi there!"),
        ];

        let messages = builder.build_messages(&conversation, &[]);

        // System prompt should always be the first message
        assert_eq!(messages[0].role, "system");
        assert!(messages[0]
            .content
            .as_ref()
            .unwrap()
            .contains("You are ENI"));
    }

    #[test]
    fn test_last_4_messages_preserved() {
        // Use a very small budget that can't fit all messages
        let builder = ContextBuilder::new(
            "System".to_string(),
            String::new(),
            50, // tiny budget
        );

        let conversation = vec![
            make_user_msg("Message 1 - this is a long message that takes up tokens"),
            make_assistant_msg("Response 1 - also quite long to consume token budget"),
            make_user_msg("Message 2 - more content here"),
            make_assistant_msg("Response 2 - even more content"),
            make_user_msg("Message 3 - recent"),
            make_assistant_msg("Response 3 - recent"),
            make_user_msg("Message 4 - most recent"),
            make_assistant_msg("Response 4 - most recent"),
        ];

        let messages = builder.build_messages(&conversation, &[]);

        // Should have system + at least the last 4 messages
        // The last 4 conversation messages should always be present
        let conv_messages: Vec<_> = messages.iter().skip(1).collect(); // skip system
        assert!(conv_messages.len() >= 4);

        // Verify the last 4 are the most recent ones
        let last_4: Vec<_> = conv_messages.iter().rev().take(4).rev().collect();
        assert!(last_4[0]
            .content
            .as_ref()
            .unwrap()
            .contains("Message 3")
            || last_4[0]
                .content
                .as_ref()
                .unwrap()
                .contains("Response 2")
            || last_4[0]
                .content
                .as_ref()
                .unwrap()
                .contains("Message 4")
            || last_4[0]
                .content
                .as_ref()
                .unwrap()
                .contains("Response 3"));
    }

    #[test]
    fn test_truncation_removes_oldest_first() {
        // Budget that can fit system + ~4-5 short messages
        let builder = ContextBuilder::new(
            "Sys".to_string(),
            String::new(),
            200,
        );

        let conversation = vec![
            make_user_msg("Old message 1"),
            make_assistant_msg("Old response 1"),
            make_user_msg("Old message 2"),
            make_assistant_msg("Old response 2"),
            make_user_msg("Old message 3"),
            make_assistant_msg("Old response 3"),
            make_user_msg("Recent 1"),
            make_assistant_msg("Recent 2"),
            make_user_msg("Recent 3"),
            make_assistant_msg("Recent 4"),
        ];

        let messages = builder.build_messages(&conversation, &[]);
        let conv_messages: Vec<_> = messages.iter().skip(1).collect(); // skip system

        // The last 4 messages must be present
        let last_msg = conv_messages.last().unwrap();
        assert_eq!(
            last_msg.content.as_ref().unwrap(),
            "Recent 4"
        );

        // If truncation happened, older messages should be removed first
        // Check that "Recent 4" is always present
        let contents: Vec<&str> = conv_messages
            .iter()
            .filter_map(|m| m.content.as_deref())
            .collect();
        assert!(contents.contains(&"Recent 4"));
        assert!(contents.contains(&"Recent 3"));
        assert!(contents.contains(&"Recent 2"));
        assert!(contents.contains(&"Recent 1"));
    }

    #[test]
    fn test_post_card_prompt_included() {
        let builder = ContextBuilder::new(
            "Base personality.".to_string(),
            "Write in a noir style.".to_string(),
            4096,
        );

        let messages = builder.build_messages(&[], &[]);

        let system_content = messages[0].content.as_ref().unwrap();
        assert!(system_content.contains("Base personality."));
        assert!(system_content.contains("Write in a noir style."));
    }

    #[test]
    fn test_reference_chunks_included() {
        let builder = ContextBuilder::new(
            "System prompt.".to_string(),
            String::new(),
            4096,
        );

        let chunks = vec![
            DocumentChunk {
                source: "campaign-notes.md".to_string(),
                content: "The Undercity is a lawless zone beneath Sector 7.".to_string(),
            },
            DocumentChunk {
                source: "characters.md".to_string(),
                content: "Kael is a street doc with military training.".to_string(),
            },
        ];

        let messages = builder.build_messages(&[], &chunks);

        let system_content = messages[0].content.as_ref().unwrap();
        assert!(system_content.contains("## Reference Context"));
        assert!(system_content.contains("[campaign-notes.md]"));
        assert!(system_content.contains("The Undercity is a lawless zone"));
        assert!(system_content.contains("[characters.md]"));
        assert!(system_content.contains("Kael is a street doc"));
    }

    #[test]
    fn test_empty_conversation() {
        let builder = ContextBuilder::new(
            "System".to_string(),
            String::new(),
            4096,
        );

        let messages = builder.build_messages(&[], &[]);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "system");
    }

    #[test]
    fn test_all_messages_fit_within_budget() {
        let builder = ContextBuilder::new(
            "Sys".to_string(),
            String::new(),
            10000, // large budget
        );

        let conversation = vec![
            make_user_msg("Hello"),
            make_assistant_msg("Hi!"),
            make_user_msg("How are you?"),
            make_assistant_msg("Great!"),
        ];

        let messages = builder.build_messages(&conversation, &[]);
        // System + all 4 conversation messages
        assert_eq!(messages.len(), 5);
    }

    #[test]
    fn test_token_counting_basic() {
        let msg = ChatMessage::user("Hello, world!");
        let tokens = count_message_tokens(&msg);
        // Should be > 0 (content tokens + overhead)
        assert!(tokens > 4); // at least the overhead
    }

    #[test]
    fn test_count_tokens_string() {
        let tokens = count_tokens("Hello, world!");
        assert!(tokens > 0);
        assert!(tokens < 10); // "Hello, world!" is about 4 tokens
    }
}
