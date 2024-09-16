use std::{collections::HashMap, ops::Add, sync::Arc};

use messageforge::{BaseMessage, MessageEnum};

use crate::{
    message_like::MessageLike, MessagesPlaceholder, PromptTemplate, Role, Template, TemplateError,
    TemplateFormat,
};

#[derive(Debug, Clone)]
pub struct ChatPromptTemplate {
    pub messages: Vec<MessageLike>,
}

impl ChatPromptTemplate {
    pub fn from_messages(messages: &[(Role, &str)]) -> Result<Self, TemplateError> {
        let mut result = Vec::new();

        for &(role, tmpl) in messages {
            if role == Role::Placeholder {
                let placeholder = MessagesPlaceholder::try_from(tmpl)?;
                result.push(MessageLike::from_placeholder(placeholder));
                continue;
            }

            let prompt_template = PromptTemplate::from_template(tmpl)?;

            match prompt_template.template_format() {
                TemplateFormat::PlainText => {
                    let base_message = role
                        .to_message(tmpl)
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

        Ok(ChatPromptTemplate { messages: result })
    }

    pub fn invoke(
        &self,
        variables: &HashMap<&str, &str>,
    ) -> Result<Vec<Arc<dyn BaseMessage>>, TemplateError> {
        self.format_messages(variables)
    }

    pub fn format_messages(
        &self,
        variables: &HashMap<&str, &str>,
    ) -> Result<Vec<Arc<dyn BaseMessage>>, TemplateError> {
        self.messages
            .iter()
            .map(|message_like| match message_like {
                MessageLike::BaseMessage(base_message) => Ok(vec![base_message.clone()]),

                MessageLike::RolePromptTemplate(role, template) => {
                    let formatted_message = template
                        .format(variables.clone())
                        .map_err(|e| TemplateError::MalformedTemplate(e.to_string()))?;
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
            })
            .flat_map(|result| match result {
                Ok(messages) => messages.into_iter().map(Ok).collect::<Vec<_>>(),
                Err(e) => vec![Err(e)],
            })
            .collect::<Result<Vec<_>, _>>()
    }
}

impl Add for ChatPromptTemplate {
    type Output = ChatPromptTemplate;
    fn add(mut self, other: ChatPromptTemplate) -> ChatPromptTemplate {
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
    use crate::{chat_templates, prompt_vars};

    #[test]
    fn test_from_messages_plaintext() {
        let templates = chat_templates!(
            System = "This is a system message.",
            Human = "Hello, human!",
        );

        let chat_prompt = ChatPromptTemplate::from_messages(templates);
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

    #[test]
    fn test_from_messages_formatted_template() {
        let templates = chat_templates!(
            System = "You are a helpful AI bot. Your name is {name}.",
            Ai = "I'm doing well, thank you.",
        );

        let chat_prompt = ChatPromptTemplate::from_messages(templates);
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

    #[test]
    fn test_from_messages_placeholder() {
        let templates = chat_templates!(
            System = "This is a valid system message.",
            Placeholder = "{history}",
        );

        let chat_prompt = ChatPromptTemplate::from_messages(templates).unwrap();
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

    #[test]
    fn test_invoke_with_base_messages() {
        let templates = chat_templates!(
            System = "This is a system message.",
            Human = "Hello, human!"
        );

        let chat_prompt = ChatPromptTemplate::from_messages(templates).unwrap();

        assert_eq!(chat_prompt.messages.len(), 2);

        let variables = HashMap::new();
        let result = chat_prompt.invoke(&variables).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content(), "This is a system message.");
        assert_eq!(result[1].content(), "Hello, human!");
    }

    #[test]
    fn test_invoke_with_role_prompt_template() {
        let templates = chat_templates!(
            System = "System maintenance is scheduled.",
            Human = "Hello, {name}!"
        );

        let chat_prompt = ChatPromptTemplate::from_messages(templates).unwrap();
        assert_eq!(chat_prompt.messages.len(), 2);

        let variables = prompt_vars!(name = "Alice");
        let result = chat_prompt.invoke(&variables).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content(), "System maintenance is scheduled.");
        assert_eq!(result[1].content(), "Hello, Alice!");
    }

    #[test]
    fn test_invoke_with_placeholder_and_role_templates() {
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

        let templates = chat_templates!(
            System = "This is a system message.",
            Placeholder = "{history}",
            Human = "How can I help you, {name}?"
        );

        let chat_prompt = ChatPromptTemplate::from_messages(templates).unwrap();
        assert_eq!(chat_prompt.messages.len(), 3);

        let variables = prompt_vars!(history = history_json.as_str(), name = "Bob");
        let result = chat_prompt.invoke(&variables).unwrap();

        assert_eq!(result.len(), 4);
        assert_eq!(result[0].content(), "This is a system message.");
        assert_eq!(result[1].content(), "Hello, AI.");
        assert_eq!(result[2].content(), "Hi, how can I assist you today?");
        assert_eq!(result[3].content(), "How can I help you, Bob?");
    }

    #[test]
    fn test_invoke_with_invalid_json_history() {
        let invalid_history_json = "invalid json string";

        let templates = chat_templates!(
            System = "This is a system message.",
            Placeholder = "{history}",
            Human = "How can I help you, {name}?"
        );

        let chat_prompt = ChatPromptTemplate::from_messages(templates).unwrap();
        let variables = prompt_vars!(history = invalid_history_json, name = "Bob");

        let result = chat_prompt.invoke(&variables);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_templates() {
        let templates = chat_templates!();
        let chat_prompt = ChatPromptTemplate::from_messages(templates);
        assert!(chat_prompt.is_ok());
        assert!(chat_prompt.unwrap().messages.is_empty());
    }

    #[test]
    fn test_invoke_with_empty_variables_map() {
        let templates = chat_templates!(
            System = "System maintenance is scheduled.",
            Human = "Hello, {name}!"
        );

        let chat_prompt = ChatPromptTemplate::from_messages(templates).unwrap();
        let variables = prompt_vars!();

        let result = chat_prompt.invoke(&variables);
        assert!(result.is_err());
    }

    #[test]
    fn test_invoke_with_multiple_placeholders_in_one_template() {
        let templates = chat_templates!(
            Human = "Hello, {name}. How are you on this {day}?",
            System = "Today is {day}. Have a great {day}."
        );

        let chat_prompt = ChatPromptTemplate::from_messages(templates).unwrap();
        let variables = prompt_vars!(name = "Alice", day = "Monday");

        let result = chat_prompt.invoke(&variables).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0].content(),
            "Hello, Alice. How are you on this Monday?"
        );
        assert_eq!(result[1].content(), "Today is Monday. Have a great Monday.");
    }

    #[test]
    fn test_add_two_templates() {
        let template1 =
            ChatPromptTemplate::from_messages(&[(System, "You are a helpful AI bot.")]).unwrap();
        let template2 =
            ChatPromptTemplate::from_messages(&[(Human, "What is the weather today?")]).unwrap();

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

    #[test]
    fn test_add_multiple_templates() {
        let system_template =
            ChatPromptTemplate::from_messages(&[(System, "System message.")]).unwrap();
        let user_template = ChatPromptTemplate::from_messages(&[(Human, "User message.")]).unwrap();
        let ai_template = ChatPromptTemplate::from_messages(&[(Ai, "AI message.")]).unwrap();

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

    #[test]
    fn test_add_empty_template() {
        let empty_template = ChatPromptTemplate::from_messages(&[]).unwrap();
        let filled_template =
            ChatPromptTemplate::from_messages(&[(System, "This is a system message.")]).unwrap();

        let combined_template = empty_template + filled_template;

        assert_eq!(combined_template.messages.len(), 1);
        if let MessageLike::BaseMessage(message) = &combined_template.messages[0] {
            assert_eq!(message.content(), "This is a system message.");
        } else {
            panic!("Expected a BaseMessage for the system message.");
        }
    }

    #[test]
    fn test_add_to_empty_template() {
        let filled_template =
            ChatPromptTemplate::from_messages(&[(System, "This is a system message.")]).unwrap();
        let empty_template = ChatPromptTemplate::from_messages(&[]).unwrap();

        let combined_template = filled_template + empty_template;

        assert_eq!(combined_template.messages.len(), 1);
        if let MessageLike::BaseMessage(message) = &combined_template.messages[0] {
            assert_eq!(message.content(), "This is a system message.");
        } else {
            panic!("Expected a BaseMessage for the system message.");
        }
    }
}
