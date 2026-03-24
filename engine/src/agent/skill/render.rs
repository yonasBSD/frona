use crate::agent::prompt::PromptLoader;

use super::resolver::Skill;

pub fn render_skills_section(skills: &[Skill], prompts: &PromptLoader) -> Option<String> {
    if skills.is_empty() {
        return None;
    }

    let mut skills_list = String::new();
    for skill in skills {
        let path_str = format!("{}/SKILL.md", skill.path);
        let name = &skill.name;
        let description = &skill.description;
        skills_list.push_str(&format!("- {name}: {description} (file: {path_str})\n"));
    }

    prompts.read_with_vars("SKILLS.md", &[("skills_list", &skills_list)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_loader() -> (tempfile::TempDir, PromptLoader) {
        let dir = tempfile::tempdir().unwrap();
        let resources = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("resources")
            .join("prompts");
        let template = std::fs::read_to_string(resources.join("SKILLS.md")).unwrap();
        std::fs::write(dir.path().join("SKILLS.md"), template).unwrap();
        let loader = PromptLoader::new(dir.path());
        (dir, loader)
    }

    #[test]
    fn returns_none_for_empty_skills() {
        let (_dir, prompts) = create_loader();
        assert!(render_skills_section(&[], &prompts).is_none());
    }

    #[test]
    fn renders_skills_with_paths() {
        let (_dir, prompts) = create_loader();
        let skills = vec![
            Skill {
                name: "weather".to_string(),
                description: "Get weather forecasts".to_string(),
                path: "/skills/weather".to_string(),
            },
            Skill {
                name: "deploy".to_string(),
                description: "Deploy applications".to_string(),
                path: "/skills/deploy".to_string(),
            },
        ];

        let result = render_skills_section(&skills, &prompts).unwrap();

        assert!(result.contains("## Skills"));
        assert!(result.contains("### Available skills"));
        assert!(result.contains("- weather: Get weather forecasts (file: /skills/weather/SKILL.md)"));
        assert!(result.contains("- deploy: Deploy applications (file: /skills/deploy/SKILL.md)"));
        assert!(result.contains("### How to use skills"));
        assert!(result.contains("progressive disclosure"));
    }
}
