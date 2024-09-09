use crate::braces::{
    count_left_braces, count_right_braces, has_multiple_words_between_braces, has_no_braces,
    has_only_double_braces, has_only_single_braces,
};

#[derive(Debug, Clone, PartialEq)]
pub enum TemplateError {
    MalformedTemplate(String),
    UnsupportedFormat(String),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::MalformedTemplate(msg) => write!(f, "Malformed template: {}", msg),
            TemplateError::UnsupportedFormat(msg) => write!(f, "Unsupported format: {}", msg),
        }
    }
}

impl std::error::Error for TemplateError {}

#[derive(Debug, PartialEq)]
pub enum TemplateFormat {
    PlainText,
    FmtString,
    Mustache,
}

pub fn is_plain_text(s: &str) -> bool {
    has_no_braces(s)
}

pub fn is_mustache(s: &str) -> bool {
    has_only_double_braces(s) && !has_multiple_words_between_braces(s)
}

pub fn is_fmtstring(s: &str) -> bool {
    has_only_single_braces(s) && !has_multiple_words_between_braces(s)
}

pub fn is_valid_template(s: &str) -> bool {
    if has_no_braces(s) {
        return true;
    }

    count_left_braces(s) == count_right_braces(s)
        && (has_only_double_braces(s) || has_only_single_braces(s))
}

pub fn detect_template(s: &str) -> Result<TemplateFormat, TemplateError> {
    if !is_valid_template(s) {
        return Err(TemplateError::MalformedTemplate(s.to_string()));
    }

    if is_plain_text(s) {
        Ok(TemplateFormat::PlainText)
    } else if is_mustache(s) {
        Ok(TemplateFormat::Mustache)
    } else if is_fmtstring(s) {
        Ok(TemplateFormat::FmtString)
    } else {
        Err(TemplateError::UnsupportedFormat(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_plain_text() {
        assert!(is_plain_text("No placeholders"));
        assert!(is_plain_text("This has no placeholders"));

        assert!(!is_plain_text("{var}"));
        assert!(!is_plain_text("{{var}}"));
        assert!(!is_plain_text("{var words another}}"));
    }

    #[test]
    fn test_is_mustache() {
        assert!(is_mustache("{{var}}"));
        assert!(is_mustache("{{var}} words {{ another }}"));

        assert!(!is_mustache("{var}"));
        assert!(!is_mustache("This has no placeholders"));
        assert!(!is_mustache("{{var"));
        assert!(!is_mustache("var}}"));
        assert!(!is_mustache("{var} words {{another}}"));
        assert!(!is_mustache("{{ hello world }}"));
    }

    #[test]
    fn test_is_fmtstring() {
        assert!(is_fmtstring("{var}"));
        assert!(is_fmtstring("Here is a {var}"));
        assert!(is_fmtstring("{var} and { another }"));

        assert!(!is_fmtstring("{{var}}"));
        assert!(!is_fmtstring("{{var}"));
        assert!(!is_fmtstring("{var}}"));
        assert!(!is_fmtstring("No placeholders"));
        assert!(!is_fmtstring("{var} words {{another}}"));
        assert!(!is_fmtstring("{ hello world }"));
    }

    #[test]
    fn test_is_valid_template() {
        assert!(is_valid_template("{var}"));
        assert!(is_valid_template("Here is a {var}"));
        assert!(is_valid_template("{var} and {another}"));
        assert!(is_valid_template("{{var}}"));
        assert!(is_valid_template("{{var}} words {{another}}"));

        assert!(!is_valid_template("{{var}"));
        assert!(!is_valid_template("{var}}"));
        assert!(!is_valid_template("{var} words {{another}}"));

        assert!(is_valid_template("No placeholders"));
    }

    #[test]
    fn test_detect_template() {
        assert_eq!(
            detect_template("No placeholders").unwrap(),
            TemplateFormat::PlainText
        );

        assert_eq!(detect_template("{var}").unwrap(), TemplateFormat::FmtString);
        assert_eq!(
            detect_template("Here is a {var}").unwrap(),
            TemplateFormat::FmtString
        );
        assert_eq!(
            detect_template("{var} and {another}").unwrap(),
            TemplateFormat::FmtString
        );
        assert_eq!(
            detect_template("{{var}}").unwrap(),
            TemplateFormat::Mustache
        );
        assert_eq!(
            detect_template("{{var}} and {{another}}").unwrap(),
            TemplateFormat::Mustache
        );

        assert_eq!(
            detect_template("{{var}").unwrap_err(),
            TemplateError::MalformedTemplate("{{var}".to_string())
        );
        assert_eq!(
            detect_template("{var}}").unwrap_err(),
            TemplateError::MalformedTemplate("{var}}".to_string())
        );
        assert_eq!(
            detect_template("{var} words {{another}}").unwrap_err(),
            TemplateError::MalformedTemplate("{var} words {{another}}".to_string())
        );
        assert_eq!(
            detect_template("{var words}").unwrap_err(),
            TemplateError::UnsupportedFormat("{var words}".to_string())
        );
    }
}
