#[macro_export]
macro_rules! prompt_vars {
    () => {
        std::collections::HashMap::new()
    };

    ($key:ident = $value:expr) => {
        std::collections::HashMap::from([(stringify!($key), $value)])
    };

    ($($key:ident = $value:expr),+ $(,)?) => {
        {
            let mut map = std::collections::HashMap::new();
            $(
                map.insert(stringify!($key), $value);
            )+
            map
        }
    };
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    #[test]
    fn test_empty_prompt_vars() {
        let vars: HashMap<&str, &str> = prompt_vars!();
        assert!(vars.is_empty(), "The hashmap should be empty.");
    }

    #[test]
    fn test_single_prompt_var() {
        let vars = prompt_vars!(name = "tom");
        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("name"), Some(&"tom"));
    }

    #[test]
    fn test_multiple_prompt_vars() {
        let vars = prompt_vars!(name = "tom", adjective = "funny", content = "chickens");
        assert_eq!(vars.len(), 3);
        assert_eq!(vars.get("name"), Some(&"tom"));
        assert_eq!(vars.get("adjective"), Some(&"funny"));
        assert_eq!(vars.get("content"), Some(&"chickens"));
    }

    #[test]
    fn test_trailing_comma() {
        let vars = prompt_vars!(name = "tom", adjective = "funny", content = "chickens",);
        assert_eq!(vars.len(), 3);
        assert_eq!(vars.get("name"), Some(&"tom"));
        assert_eq!(vars.get("adjective"), Some(&"funny"));
        assert_eq!(vars.get("content"), Some(&"chickens"));
    }

    #[test]
    fn test_overwriting_keys() {
        let vars = prompt_vars!(name = "tom", name = "jerry");
        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("name"), Some(&"jerry"));
    }
}
