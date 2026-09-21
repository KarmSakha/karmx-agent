use anyhow::Result;
use goose::config::profile::{extensions_allowlist, PromptProfile, PROMPT_PROFILE_KEY};
use goose::config::Config;

pub fn handle_context(profile: Option<&str>) -> Result<()> {
    let config = Config::global();
    if let Some(profile) = profile {
        if profile == "auto" {
            config.delete(PROMPT_PROFILE_KEY)?;
        } else {
            config.set_param(PROMPT_PROFILE_KEY, profile)?;
        }
        println!("Saved context profile: {profile}. Restart the app or CLI session to apply consistently.");
    }

    let active = if PromptProfile::current().is_compact() {
        "compact"
    } else {
        "default"
    };
    println!("Effective context profile: {active}");
    match extensions_allowlist() {
        Some(names) => println!("Allowed extensions: {}", names.join(", ")),
        None => println!("Allowed extensions: all enabled extensions"),
    }
    println!("Compact uses a shorter prompt and a limited tool set, including browser and Jev screen tools when enabled. Other extensions, including skills, may be excluded.");
    println!("Environment overrides and a custom extension allowlist take precedence. Jev screen tasks require macOS permissions and a TypeSafe API key.");
    println!("Choose: karmx context compact | default | auto");
    Ok(())
}
