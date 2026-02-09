use std::path::PathBuf;

use include_dir::{Dir, include_dir};

static BUILTIN_PROMPTS: Dir = include_dir!("$CARGO_MANIFEST_DIR/config/prompts");

#[derive(Clone)]
pub struct PromptLoader {
    override_dir: PathBuf,
}

impl PromptLoader {
    pub fn new(override_dir: impl Into<PathBuf>) -> Self {
        Self {
            override_dir: override_dir.into(),
        }
    }

    pub fn read(&self, name: &str) -> Option<String> {
        let override_path = self.override_dir.join(name);
        if let Ok(content) = std::fs::read_to_string(&override_path) {
            return Some(content);
        }

        BUILTIN_PROMPTS
            .get_file(name)
            .and_then(|f| f.contents_utf8())
            .map(|s| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_builtin_prompt() {
        let loader = PromptLoader::new("/nonexistent");
        let content = loader.read("CHAT_COMPACTION.md");
        assert!(content.is_some());
        assert!(content.unwrap().contains("conversation summarizer"));
    }

    #[test]
    fn returns_none_for_missing_prompt() {
        let loader = PromptLoader::new("/nonexistent");
        assert!(loader.read("DOES_NOT_EXIST.md").is_none());
    }

    #[test]
    fn filesystem_override_shadows_builtin() {
        let tmp = std::env::temp_dir().join("frona_prompt_loader_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("CHAT_COMPACTION.md"), "Custom prompt").unwrap();

        let loader = PromptLoader::new(&tmp);
        let content = loader.read("CHAT_COMPACTION.md").unwrap();
        assert_eq!(content, "Custom prompt");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
