use futures::future::join_all;
use std::{collections::HashMap, ops::Add, sync::Arc};

use messageforge::{BaseMessage, MessageEnum};

use crate::{
    message_like::MessageLike, Formattable, MessagesPlaceholder, Role, Templatable, Template,
    TemplateError, TemplateFormat,
};

#[derive(Debug, Clone)]
pub struct ChatTemplate {
    pub messages: Vec<MessageLike>,
}

impl ChatTemplate {
    pub async fn from_messages<I>(messages: I) -> Result<Self, TemplateError>
    where
        I: IntoIterator<Item = (Role, String)>,
    {
        let mut result = Vec::new();

        for (role, tmpl) in messages {
            if role == Role::Placeholder {
                let placeholder = MessagesPlaceholder::try_from(tmpl)?;
                result.push(MessageLike::from_placeholder(placeholder));
                continue;
            }

            let prompt_template = Template::from_template(tmpl.as_str())?;

            match prompt_template.template_format() {
                TemplateFormat::PlainText => {
                    let base_message = role
                        .to_message(tmpl.as_str())
                        .map_err(|_| TemplateError::InvalidRoleError)?;
                    result.push(MessageLike::from_base_message(base_message))
                }
                _ => {
                    result.push(MessageLike::from_role_prompt_template(
                        role,
                        prompt_template,
                    ));
                }
            }
        }

        Ok(ChatTemplate { messages: result })
    }

    pub async fn invoke(
        &self,
        variables: &HashMap<&str, &str>,
    ) -> Result<Vec<Arc<dyn BaseMessage>>, TemplateError> {
        self.format_messages(variables).await
    }

    pub async fn format_messages(
        &self,
        variables: &HashMap<&str, &str>,
    ) -> Result<Vec<Arc<dyn BaseMessage>>, TemplateError> {
        let futures: Vec<_> = self
            .messages
            .iter()
            .map(|message_like| async move {
                match message_like {
                    MessageLike::BaseMessage(base_message) => Ok(vec![base_message.clone()]),

                    MessageLike::RolePromptTemplate(role, template) => {
                        let formatted_message = template.format(&variables.clone())?;

                        let base_message = role
                            .to_message(&formatted_message)
                            .map_err(|_| TemplateError::InvalidRoleError)?;

                        Ok(vec![base_message])
                    }

                    MessageLike::Placeholder(placeholder) => {
                        if placeholder.optional() {
                            Ok(vec![])
                        } else {
                            let messages =
                                variables.get(placeholder.variable_name()).ok_or_else(|| {
                                    TemplateError::MissingVariable(
                                        placeholder.variable_name().to_string(),
                                    )
                                })?;

                            let deserialized_messages: Vec<MessageEnum> =
                                serde_json::from_str(messages).map_err(|e| {
                                    TemplateError::MalformedTemplate(format!(
                                        "Failed to deserialize placeholder: {}",
                                        e
                                    ))
                                })?;

                            let limited_messages = if placeholder.n_messages() > 0 {
                                deserialized_messages
                                    .into_iter()
                                    .take(placeholder.n_messages())
                                    .collect()
                            } else {
                                deserialized_messages
                            };

                            Ok(limited_messages
                                .into_iter()
                                .map(|message_enum| Arc::new(message_enum) as Arc<dyn BaseMessage>)
                                .collect())
                        }
                    }
                }
            })
            .collect();

        let results = join_all(futures).await;

        results
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map(|vecs| vecs.into_iter().flatten().collect())
    }
}

impl Formattable<&str, &str> for ChatTemplate {
    fn format(&self, variables: &HashMap<&str, &str>) -> Result<String, TemplateError> {
        // Use the existing format_messages method to format the chat messages
        let formatted_messages = futures::executor::block_on(self.format_messages(variables))?;

        // Combine all formatted messages into a single string, separated by newlines
        let combined_result = formatted_messages
            .iter()
            .map(|message| message.content().to_string()) // Extract the content from each message
            .collect::<Vec<_>>()
            .join("\n");

        Ok(combined_result)
    }
}

impl Add for ChatTemplate {
    type Output = ChatTemplate;
    fn add(mut self, other: ChatTemplate) -> ChatTemplate {
        self.messages.extend(other.messages);
        self
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::message_like::MessageLike;
    use crate::Role::{Ai, Human, Placeholder, System};
    use crate::{chats, vars};

    #[tokio::test]
    async fn test_from_messages_plaintext() {
        let templates = chats!(
            System = "This is a system message.",
            Human = "Hello, human!",
        );

        let chat_prompt = ChatTemplate::from_messages(templates).await;
        let chat_prompt = chat_prompt.unwrap();
        assert_eq!(chat_prompt.messages.len(), 2);

        if let MessageLike::BaseMessage(message) = &chat_prompt.messages[0] {
            assert_eq!(message.content(), "This is a system message.");
        } else {
            panic!("Expected a BaseMessage for the system message.");
        }

        if let MessageLike::BaseMessage(message) = &chat_prompt.messages[1] {
            assert_eq!(message.content(), "Hello, human!");
        } else {
            panic!("Expected a BaseMessage for the human message.");
        }
    }

    #[tokio::test]
    async fn test_from_messages_formatted_template() {
        let templates = chats!(
            System = "You are a helpful AI bot. Your name is {name}.",
            Ai = "I'm doing well, thank you.",
        );

        let chat_prompt = ChatTemplate::from_messages(templates).await;
        let chat_prompt = chat_prompt.unwrap();
        assert_eq!(chat_prompt.messages.len(), 2);

        if let MessageLike::RolePromptTemplate(role, template) = &chat_prompt.messages[0] {
            assert_eq!(
                template.template(),
                "You are a helpful AI bot. Your name is {name}."
            );
            assert_eq!(role, &System);
        } else {
            panic!("Expected a PromptTemplate for the system message.");
        }

        if let MessageLike::BaseMessage(message) = &chat_prompt.messages[1] {
            assert_eq!(message.content(), "I'm doing well, thank you.");
        } else {
            panic!("Expected a BaseMessage for the AI message.");
        }
    }

    #[tokio::test]
    async fn test_from_messages_placeholder() {
        let templates = chats!(
            System = "This is a valid system message.",
            Placeholder = "{history}",
        );

        let chat_prompt = ChatTemplate::from_messages(templates).await.unwrap();
        assert_eq!(chat_prompt.messages.len(), 2);

        if let MessageLike::BaseMessage(system_message) = &chat_prompt.messages[0] {
            assert_eq!(system_message.content(), "This is a valid system message.");
        } else {
            panic!("Expected BaseMessage for the system role.");
        }

        if let MessageLike::Placeholder(placeholder) = &chat_prompt.messages[1] {
            assert_eq!(placeholder.variable_name(), "history");
            assert!(!placeholder.optional());
            assert_eq!(placeholder.n_messages(), MessagesPlaceholder::DEFAULT_LIMIT);
        } else {
            panic!("Expected MessagesPlaceholder for the placeholder role.");
        }
    }

    #[tokio::test]
    async fn test_invoke_with_base_messages() {
        let templates = chats!(
            System = "This is a system message.",
            Human = "Hello, human!"
        );

        let chat_prompt = ChatTemplate::from_messages(templates).await.unwrap();

        assert_eq!(chat_prompt.messages.len(), 2);

        let variables = HashMap::new();
        let result = chat_prompt.invoke(&variables).await.unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content(), "This is a system message.");
        assert_eq!(result[1].content(), "Hello, human!");
    }

    #[tokio::test]
    async fn test_invoke_with_role_prompt_template() {
        let templates = chats!(
            System = "System maintenance is scheduled.",
            Human = "Hello, {name}!"
        );

        let chat_prompt = ChatTemplate::from_messages(templates).await.unwrap();
        assert_eq!(chat_prompt.messages.len(), 2);

        let variables = vars!(name = "Alice");
        let result = chat_prompt.invoke(&variables).await.unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content(), "System maintenance is scheduled.");
        assert_eq!(result[1].content(), "Hello, Alice!");
    }

    #[tokio::test]
    async fn test_invoke_with_placeholder_and_role_templates() {
        let history_json = json!([
            {
                "role": "human",
                "content": "Hello, AI.",
            },
            {
                "role": "ai",
                "content": "Hi, how can I assist you today?",
            }
        ])
        .to_string();

        let templates = chats!(
            System = "This is a system message.",
            Placeholder = "{history}",
            Human = "How can I help you, {name}?"
        );

        let chat_prompt = ChatTemplate::from_messages(templates).await.unwrap();
        assert_eq!(chat_prompt.messages.len(), 3);

        let variables = &vars!(history = history_json.as_str(), name = "Bob");
        let result = chat_prompt.invoke(variables).await.unwrap();

        assert_eq!(result.len(), 4);
        assert_eq!(result[0].content(), "This is a system message.");
        assert_eq!(result[1].content(), "Hello, AI.");
        assert_eq!(result[2].content(), "Hi, how can I assist you today?");
        assert_eq!(result[3].content(), "How can I help you, Bob?");
    }

    #[tokio::test]
    async fn test_invoke_with_invalid_json_history() {
        let invalid_history_json = "invalid json string";

        let templates = chats!(
            System = "This is a system message.",
            Placeholder = "{history}",
            Human = "How can I help you, {name}?"
        );

        let chat_prompt = ChatTemplate::from_messages(templates).await.unwrap();
        let variables = vars!(history = invalid_history_json, name = "Bob");

        let result = chat_prompt.invoke(&variables).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_templates() {
        let templates = chats!();
        let chat_prompt = ChatTemplate::from_messages(templates);
        assert!(chat_prompt.await.unwrap().messages.is_empty());
    }

    #[tokio::test]
    async fn test_invoke_with_empty_variables_map() {
        let templates = chats!(
            System = "System maintenance is scheduled.",
            Human = "Hello, {name}!"
        );

        let chat_prompt = ChatTemplate::from_messages(templates).await.unwrap();
        let variables = vars!();

        let result = chat_prompt.invoke(&variables).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invoke_with_multiple_placeholders_in_one_template() {
        let templates = chats!(
            Human = "Hello, {name}. How are you on this {day}?",
            System = "Today is {day}. Have a great {day}."
        );

        let chat_prompt = ChatTemplate::from_messages(templates).await.unwrap();
        let variables = vars!(name = "Alice", day = "Monday");

        let result = chat_prompt.invoke(&variables).await.unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0].content(),
            "Hello, Alice. How are you on this Monday?"
        );
        assert_eq!(result[1].content(), "Today is Monday. Have a great Monday.");
    }

    #[tokio::test]
    async fn test_add_two_templates() {
        let template1 = ChatTemplate::from_messages(chats!(System = "You are a helpful AI bot."))
            .await
            .unwrap();
        let template2 = ChatTemplate::from_messages(chats!(Human = "What is the weather today?"))
            .await
            .unwrap();

        let combined_template = template1 + template2;

        assert_eq!(combined_template.messages.len(), 2);

        if let MessageLike::BaseMessage(message) = &combined_template.messages[0] {
            assert_eq!(message.content(), "You are a helpful AI bot.");
        } else {
            panic!("Expected a BaseMessage for the system message.");
        }

        if let MessageLike::BaseMessage(message) = &combined_template.messages[1] {
            assert_eq!(message.content(), "What is the weather today?");
        } else {
            panic!("Expected a BaseMessage for the human message.");
        }
    }

    #[tokio::test]
    async fn test_add_multiple_templates() {
        let system_template = ChatTemplate::from_messages(chats!(System = "System message."))
            .await
            .unwrap();
        let user_template = ChatTemplate::from_messages(chats!(Human = "User message."))
            .await
            .unwrap();
        let ai_template = ChatTemplate::from_messages(chats!(Ai = "AI message."))
            .await
            .unwrap();

        let combined_template = system_template + user_template + ai_template;

        assert_eq!(combined_template.messages.len(), 3);

        if let MessageLike::BaseMessage(message) = &combined_template.messages[0] {
            assert_eq!(message.content(), "System message.");
        } else {
            panic!("Expected a BaseMessage for the system message.");
        }

        if let MessageLike::BaseMessage(message) = &combined_template.messages[1] {
            assert_eq!(message.content(), "User message.");
        } else {
            panic!("Expected a BaseMessage for the human message.");
        }

        if let MessageLike::BaseMessage(message) = &combined_template.messages[2] {
            assert_eq!(message.content(), "AI message.");
        } else {
            panic!("Expected a BaseMessage for the AI message.");
        }
    }

    #[tokio::test]
    async fn test_add_empty_template() {
        let empty_template = ChatTemplate::from_messages(chats!()).await.unwrap();
        let filled_template =
            ChatTemplate::from_messages(chats!(System = "This is a system message."))
                .await
                .unwrap();

        let combined_template = empty_template + filled_template;

        assert_eq!(combined_template.messages.len(), 1);
        if let MessageLike::BaseMessage(message) = &combined_template.messages[0] {
            assert_eq!(message.content(), "This is a system message.");
        } else {
            panic!("Expected a BaseMessage for the system message.");
        }
    }

    #[tokio::test]
    async fn test_add_to_empty_template() {
        let filled_template =
            ChatTemplate::from_messages(chats!(System, "This is a system message."))
                .await
                .unwrap();
        let empty_template = ChatTemplate::from_messages(chats!()).await.unwrap();

        let combined_template = filled_template + empty_template;

        assert_eq!(combined_template.messages.len(), 1);
        if let MessageLike::BaseMessage(message) = &combined_template.messages[0] {
            assert_eq!(message.content(), "This is a system message.");
        } else {
            panic!("Expected a BaseMessage for the system message.");
        }
    }

    #[test]
    fn test_format_with_basic_messages() {
        let templates = chats!(
            System = "System message.",
            Human = "Hello, {name}!",
            Ai = "Hi {name}, how can I assist you today?"
        );

        let chat_template =
            futures::executor::block_on(ChatTemplate::from_messages(templates)).unwrap();
        let variables = &vars!(name = "Alice");

        let formatted_output = chat_template.format(variables).unwrap();

        let expected_output = "\
System message.
Hello, Alice!
Hi Alice, how can I assist you today?";

        assert_eq!(formatted_output, expected_output);
    }

    #[test]
    fn test_format_with_placeholders() {
        let history_json = json!([
            {
                "role": "human",
                "content": "What is the capital of France?",
            },
            {
                "role": "ai",
                "content": "The capital of France is Paris.",
            }
        ])
        .to_string();

        let templates = chats!(
            System = "This is a system message.",
            Placeholder = "{history}",
            Human = "Can I help you with anything else, {name}?"
        );

        let chat_template =
            futures::executor::block_on(ChatTemplate::from_messages(templates)).unwrap();
        let variables = &vars!(history = history_json.as_str(), name = "Bob");

        let formatted_output = chat_template.format(variables).unwrap();

        let expected_output = "\
This is a system message.
What is the capital of France?
The capital of France is Paris.
Can I help you with anything else, Bob?";

        assert_eq!(formatted_output, expected_output);
    }

    #[test]
    fn test_format_with_empty_chat_template() {
        let templates = chats!(); // Empty chat template

        let chat_template =
            futures::executor::block_on(ChatTemplate::from_messages(templates)).unwrap();
        let variables = &vars!();

        let formatted_output = chat_template.format(variables).unwrap();

        // Expect an empty output as the chat template has no messages
        let expected_output = "";
        assert_eq!(formatted_output, expected_output);
    }

    #[test]
    fn test_format_with_missing_variable_error() {
        let templates = chats!(
            System = "You are a helpful assistant.",
            Human = "Hello, {name}.",
            Ai = "How can I assist you today, {name}?"
        );

        let chat_template =
            futures::executor::block_on(ChatTemplate::from_messages(templates)).unwrap();
        // Missing the "name" variable in the vars map
        let variables = &vars!();

        let result = chat_template.format(variables);

        // Expect an error due to the missing "name" variable
        assert!(result.is_err());
        if let Err(TemplateError::MissingVariable(missing_var)) = result {
            assert_eq!(
                missing_var,
                "Variable 'name' is missing. Expected: [\"name\"], but received: []"
            );
        } else {
            panic!("Expected MissingVariable error");
        }
    }

    #[test]
    fn test_format_with_malformed_placeholder() {
        let templates = chats!(
            System = "System maintenance is scheduled.",
            Placeholder = "{invalid_placeholder}",
            Human = "Hello, {name}!"
        );

        let chat_template =
            futures::executor::block_on(ChatTemplate::from_messages(templates)).unwrap();
        let variables = &vars!(name = "Alice");

        let result = chat_template.format(variables);

        // Expect an error due to the invalid placeholder
        assert!(result.is_err());
        if let Err(TemplateError::MissingVariable(missing_var)) = result {
            assert_eq!(missing_var, "invalid_placeholder");
        } else {
            panic!("Expected MissingVariable error");
        }
    }

    #[test]
    fn test_format_with_repeated_variables() {
        let templates = chats!(
            System = "Hello {name}.",
            Ai = "{name}, how can I assist you today?"
        );

        let chat_template =
            futures::executor::block_on(ChatTemplate::from_messages(templates)).unwrap();
        let variables = &vars!(name = "Bob");

        let formatted_output = chat_template.format(variables).unwrap();

        let expected_output = "\
Hello Bob.
Bob, how can I assist you today?";

        assert_eq!(formatted_output, expected_output);
    }

    #[test]
    fn test_format_with_plain_text_messages() {
        let templates = chats!(
            System = "Welcome to the system.",
            Human = "This is a plain text message.",
            Ai = "No variables or placeholders here."
        );

        let chat_template =
            futures::executor::block_on(ChatTemplate::from_messages(templates)).unwrap();
        let variables = &vars!(); // No variables needed

        let formatted_output = chat_template.format(variables).unwrap();

        let expected_output = "\
Welcome to the system.
This is a plain text message.
No variables or placeholders here.";

        assert_eq!(formatted_output, expected_output);
    }

    #[test]
    fn test_format_with_mixed_placeholders_and_plain_text() {
        let templates = chats!(
            System = "System notification: {event}.",
            Ai = "You have {unread_messages} unread messages.",
            Human = "Thanks, AI."
        );

        let chat_template =
            futures::executor::block_on(ChatTemplate::from_messages(templates)).unwrap();
        let variables = &vars!(event = "System update", unread_messages = "5");

        let formatted_output = chat_template.format(variables).unwrap();

        let expected_output = "\
System notification: System update.
You have 5 unread messages.
Thanks, AI.";

        assert_eq!(formatted_output, expected_output);
    }
}
