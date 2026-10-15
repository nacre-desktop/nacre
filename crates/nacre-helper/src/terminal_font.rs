//! Projection of the registered desktop monoFont preference into Kitty.
use crate::{classic_state, paths::Paths, Result};

pub const DEFAULT: &str = "JetBrainsMono Nerd Font";

pub fn publish(paths: &Paths, family: &str) -> Result<()> {
    // A font name is data, never a second Kitty configuration directive.
    if family.chars().count() > 80 || family.chars().any(char::is_control) {
        return Err("Use a font family up to 80 characters without control characters".into());
    }
    let family = if family.is_empty() { DEFAULT } else { family };
    classic_state::atomic_write(
        &paths.config.join("kitty-font.conf"),
        &format!("# Managed by Nacre desktop monoFont; custom.conf may override.\nfont_family {family}\n"),
    )?;
    classic_state::reload_terminals(&paths.home);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projects_literal_family_and_reset_without_touching_user_overrides() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::new(temp.path());
        std::fs::create_dir_all(paths.home.join(".config/kitty"))?;
        let custom = paths.home.join(".config/kitty/custom.conf");
        std::fs::write(&custom, "font_size 15\n")?;
        publish(&paths, "FiraCode Nerd Font")?;
        let file = paths.config.join("kitty-font.conf");
        assert!(std::fs::read_to_string(&file)?.contains("font_family FiraCode Nerd Font\n"));
        let before = std::fs::read(&file)?;
        assert!(publish(&paths, "Mono\ninclude evil.conf").is_err());
        assert_eq!(std::fs::read(&file)?, before);
        publish(&paths, "")?;
        assert!(std::fs::read_to_string(file)?.contains(DEFAULT));
        assert_eq!(std::fs::read_to_string(custom)?, "font_size 15\n");
        Ok(())
    }
}
