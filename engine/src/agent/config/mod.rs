use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ConfigEntry {
    pub template: String,
    pub metadata: HashMap<String, String>,
}

pub fn parse_frontmatter(content: &str) -> ConfigEntry {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return ConfigEntry {
            template: content.to_string(),
            metadata: HashMap::new(),
        };
    }

    let after_first = &trimmed[3..];
    if let Some(end_idx) = after_first.find("\n---") {
        let yaml_str = &after_first[..end_idx];
        let body = &after_first[end_idx + 4..];
        let body = body.strip_prefix('\n').unwrap_or(body);

        let metadata: HashMap<String, String> = serde_yaml::from_str(yaml_str)
            .unwrap_or_default();

        ConfigEntry {
            template: body.to_string(),
            metadata,
        }
    } else {
        ConfigEntry {
            template: content.to_string(),
            metadata: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_with_yaml() {
        let content = "---\nmodel: anthropic/claude-sonnet-4-5\n---\nHello world";
        let entry = parse_frontmatter(content);
        assert_eq!(entry.template, "Hello world");
        assert_eq!(
            entry.metadata.get("model"),
            Some(&"anthropic/claude-sonnet-4-5".to_string())
        );
    }

    #[test]
    fn test_parse_frontmatter_empty_yaml() {
        let content = "---\nmodel:\n---\nHello world";
        let entry = parse_frontmatter(content);
        assert_eq!(entry.template, "Hello world");
        assert!(
            !entry.metadata.contains_key("model")
                || entry.metadata.get("model") == Some(&"".to_string())
        );
    }

    #[test]
    fn test_parse_frontmatter_no_yaml() {
        let content = "Just plain text";
        let entry = parse_frontmatter(content);
        assert_eq!(entry.template, "Just plain text");
        assert!(entry.metadata.is_empty());
    }
}
