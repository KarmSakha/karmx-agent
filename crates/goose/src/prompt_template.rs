use crate::config::paths::Paths;
use crate::config::profile::PromptProfile;
use include_dir::{include_dir, Dir};
use minijinja::{Environment, Error as MiniJinjaError, Value as MJValue};
use serde::Serialize;
use std::path::PathBuf;

static CORE_PROMPTS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/prompts");

static TEMPLATE_REGISTRY: &[(&str, &str)] = &[
    (
        "system.md",
        "Main system prompt that defines goose's personality and behavior",
    ),
    (
        "system.compact.md",
        "Compact system prompt used when KARMX_PROMPT_PROFILE=compact (small local models)",
    ),
    (
        "compaction.md",
        "Prompt for summarizing conversation history when context limits are reached",
    ),
    (
        "compaction_summary.md",
        "Renders the structured compaction output into the post-compaction context",
    ),
    (
        "subagent_system.md",
        "System prompt for subagents spawned to handle specific tasks",
    ),
    (
        "apps_create.md",
        "Prompt for generating new Goose apps based on the user instructions",
    ),
    (
        "apps_iterate.md",
        "Prompt for updating existing Goose apps based on feedback",
    ),
    (
        "permission_judge.md",
        "Prompt for analyzing tool operations for read-only detection",
    ),
    (
        "plan.md",
        "Prompt used when goose creates step-by-step plans. CLI only",
    ),
    (
        "tiny_model_system.md",
        "System prompt for tiny local models using shell command emulation",
    ),
    (
        "session_name.md",
        "System prompt for generating short session names from conversation history",
    ),
];

/// Information about a template including its content and customization status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Template {
    pub name: String,
    pub description: String,
    pub default_content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_content: Option<String>,
    pub is_customized: bool,
}

fn builtin_content(name: &str) -> Option<String> {
    CORE_PROMPTS_DIR
        .get_file(name)
        .map(|file| String::from_utf8_lossy(file.contents()).to_string())
        .or_else(|| goose_context_management::templates::builtin_template(name))
}

fn user_prompts_dir() -> PathBuf {
    Paths::config_dir().join("prompts")
}

fn is_registered(name: &str) -> bool {
    TEMPLATE_REGISTRY.iter().any(|(n, _)| *n == name)
}

/// Wrap code in a markdown fence longer than any backtick run it contains,
/// so embedded fences cannot close the block early.
fn code_fence(code: String) -> String {
    let longest_run = code
        .chars()
        .fold((0usize, 0usize), |(max, run), c| {
            if c == '`' {
                (max.max(run + 1), run + 1)
            } else {
                (max, 0)
            }
        })
        .0;
    let fence = "`".repeat((longest_run + 1).max(3));
    format!("{fence}\n{}\n{fence}", code.trim_end_matches('\n'))
}

pub fn render_string<T: Serialize>(
    template_str: &str,
    context: &T,
) -> Result<String, MiniJinjaError> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.add_filter("code_fence", code_fence);
    env.add_template("template", template_str)?;
    let tmpl = env.get_template("template")?;
    let ctx = MJValue::from_serialize(context);
    let rendered = tmpl.render(ctx)?;
    Ok(rendered.trim().to_string())
}

/// Applies the active prompt profile: under `KARMX_PROMPT_PROFILE=compact` a
/// registered template `x.md` resolves to `x.compact.md` when a user or builtin
/// version of it exists and the user has not overridden `x.md` itself.
fn resolve_profile_name(name: &str) -> String {
    PromptProfile::current().resolve_template_name(
        name,
        |candidate| user_prompts_dir().join(candidate).exists(),
        |candidate| builtin_content(candidate).is_some(),
    )
}

pub fn render_template<T: Serialize>(name: &str, context: &T) -> Result<String, MiniJinjaError> {
    if !is_registered(name) {
        return Err(MiniJinjaError::new(
            minijinja::ErrorKind::TemplateNotFound,
            format!("Template '{}' is not registered", name),
        ));
    }

    let name = &resolve_profile_name(name);
    let user_path = user_prompts_dir().join(name);
    let template_str = if user_path.exists() {
        std::fs::read_to_string(&user_path).map_err(|e| {
            MiniJinjaError::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("Failed to read user template: {}", e),
            )
        })?
    } else {
        builtin_content(name).ok_or_else(|| {
            MiniJinjaError::new(
                minijinja::ErrorKind::TemplateNotFound,
                format!("Built-in template '{}' not found", name),
            )
        })?
    };

    render_string(&template_str, context)
}

pub fn template_source(name: &str) -> Result<String, MiniJinjaError> {
    let name = &resolve_profile_name(name);
    let user_path = user_prompts_dir().join(name);
    if user_path.exists() {
        return std::fs::read_to_string(&user_path).map_err(|e| {
            MiniJinjaError::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("Failed to read user template: {}", e),
            )
        });
    }
    builtin_content(name).ok_or_else(|| {
        MiniJinjaError::new(
            minijinja::ErrorKind::TemplateNotFound,
            format!("Built-in template '{}' not found", name),
        )
    })
}

pub fn get_template(name: &str) -> Option<Template> {
    let (_, description) = TEMPLATE_REGISTRY.iter().find(|(n, _)| *n == name)?;

    let default_content = builtin_content(name)?;

    let user_path = user_prompts_dir().join(name);
    let user_content = if user_path.exists() {
        std::fs::read_to_string(&user_path).ok()
    } else {
        None
    };

    let is_customized = user_content.is_some();

    Some(Template {
        name: name.to_string(),
        description: description.to_string(),
        default_content,
        user_content,
        is_customized,
    })
}

pub fn save_template(name: &str, content: &str) -> std::io::Result<()> {
    if !is_registered(name) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Template '{}' is not registered", name),
        ));
    }

    let prompts_dir = user_prompts_dir();
    std::fs::create_dir_all(&prompts_dir)?;
    let path = prompts_dir.join(name);
    std::fs::write(path, content)
}

/// Reset a template to its default by removing the user customization.
pub fn reset_template(name: &str) -> std::io::Result<()> {
    if !is_registered(name) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Template '{}' is not registered", name),
        ));
    }

    let path = user_prompts_dir().join(name);
    if path.exists() {
        std::fs::remove_file(path)
    } else {
        Ok(())
    }
}

pub fn list_templates() -> Vec<Template> {
    TEMPLATE_REGISTRY
        .iter()
        .filter_map(|(name, description)| {
            let default_content = builtin_content(name)?;

            let user_path = user_prompts_dir().join(name);
            let user_content = if user_path.exists() {
                std::fs::read_to_string(&user_path).ok()
            } else {
                None
            };

            let is_customized = user_content.is_some();

            Some(Template {
                name: name.to_string(),
                description: description.to_string(),
                default_content,
                user_content,
                is_customized,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_get_template() {
        let template = get_template("system.md");
        assert!(template.is_some(), "system.md should be registered");

        let template = template.unwrap();
        assert_eq!(template.name, "system.md");
        assert!(!template.description.is_empty());
        assert!(!template.default_content.is_empty());
        assert!(!template.is_customized);
    }

    #[test]
    fn test_render_template() {
        let context: HashMap<String, String> = HashMap::new();
        let result = render_template("system.md", &context);
        assert!(result.is_ok(), "Should be able to render system.md");
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn compact_system_template_renders_with_the_prompt_manager_context() {
        let source = template_source("system.compact.md").unwrap();
        let context = serde_json::json!({
            "extensions": [
                {"name": "developer", "instructions": "Use the developer tools.", "has_resources": false},
                {"name": "apps", "instructions": "", "has_resources": true}
            ],
            "include_extensions": true,
            "code_execution_mode": false,
            "enable_subagents": true,
            "parallel_tool_calls": true,
            "moim_system_prompt_block": "# Turn Context\n\nblock"
        });
        let rendered = render_string(&source, &context).unwrap();
        assert!(rendered.starts_with("You are karmX"));
        assert!(rendered.contains("# Turn Context"));
        assert!(rendered.contains("## developer"));
        assert!(rendered.contains("Use the developer tools."));
        assert!(rendered.contains("apps supports resources."));
        assert!(rendered
            .contains("Independent tool calls (reads, searches, checks) may share one turn."));
        assert!(!rendered.contains("{%"));
    }

    #[test]
    fn compact_system_template_gates_batched_tool_calls_and_separates_extension_lines() {
        let source = template_source("system.compact.md").unwrap();
        let context = serde_json::json!({
            "extensions": [
                {"name": "apps", "instructions": "Create apps.", "has_resources": true}
            ],
            "include_extensions": true,
            "code_execution_mode": false,
            "enable_subagents": false,
            "parallel_tool_calls": false
        });
        let rendered = render_string(&source, &context).unwrap();
        assert!(!rendered.contains("may share one turn"));
        assert!(!rendered.contains("Sidekick"));
        let resources = rendered.find("apps supports resources.").unwrap();
        let instructions = rendered.find("Create apps.").unwrap();
        assert!(resources < instructions);
        assert!(
            rendered[resources..instructions].contains('\n'),
            "resources line and instructions must be separate lines"
        );
    }

    #[test]
    fn compact_system_template_is_registered_and_much_shorter() {
        let full = template_source("system.md").unwrap();
        let compact = template_source("system.compact.md").unwrap();
        assert!(is_registered("system.compact.md"));
        assert!(compact.len() * 3 < full.len());
    }

    #[test]
    fn test_list_templates() {
        let templates = list_templates();
        assert_eq!(templates.len(), TEMPLATE_REGISTRY.len());

        let has_system = templates.iter().any(|t| t.name == "system.md");
        assert!(has_system, "system.md should be in the template list");

        for template in templates {
            assert!(
                !template.description.is_empty(),
                "Each template should have a description"
            );
            assert!(
                !template.default_content.is_empty(),
                "Each template should have content"
            );
        }
    }
}
