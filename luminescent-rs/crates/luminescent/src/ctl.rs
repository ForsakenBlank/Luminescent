//! The commands `luminescent ctl` understands.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CtlCommand {
    Ping,
    Status,
    /// Open the overlay, on a page if one is named.
    Overlay(Option<String>),
    Palette,
    Settings(Option<String>),
    /// Run `"<owner>:<action>"` from the action registry.
    Action(String),
    /// Switch theme: `next`, `previous`, `back` (the one before) or a theme's name.
    Theme(String),
    Quit,
}

fn rest_or(arg: Option<String>, error: &str) -> Result<String, String> {
    arg.ok_or_else(|| error.to_owned())
}

pub fn parse(line: &str) -> Result<CtlCommand, String> {
    let mut words = line.split_whitespace();
    let verb = words.next().unwrap_or_default().to_lowercase();
    let rest = words.collect::<Vec<_>>().join(" ");
    let arg = (!rest.is_empty()).then(|| rest.clone());
    match verb.as_str() {
        "ping" => Ok(CtlCommand::Ping),
        "status" => Ok(CtlCommand::Status),
        "overlay" | "open" | "show" => Ok(CtlCommand::Overlay(arg)),
        "palette" => Ok(CtlCommand::Palette),
        "settings" => Ok(CtlCommand::Settings(arg)),
        "action" if rest.contains(':') => Ok(CtlCommand::Action(rest)),
        "action" => Err("action needs an id like notes:new_note".to_owned()),
        "theme" => rest_or(arg, "theme needs next, previous, back or a theme's name").map(CtlCommand::Theme),
        "quit" => Ok(CtlCommand::Quit),
        "" => Err("no command given".to_owned()),
        other => Err(format!(
            "unknown command '{other}' (try ping, status, overlay, palette, settings, action, theme, quit)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        assert_eq!(parse("ping"), Ok(CtlCommand::Ping));
        assert_eq!(parse(" Overlay  notes "), Ok(CtlCommand::Overlay(Some("notes".into()))));
        assert_eq!(parse("show"), Ok(CtlCommand::Overlay(None)));
        assert_eq!(parse("action notes:new_note"), Ok(CtlCommand::Action("notes:new_note".into())));
        assert!(parse("action").is_err());
        assert_eq!(parse("theme next"), Ok(CtlCommand::Theme("next".into())));
        assert_eq!(parse("theme Tokyo Night"), Ok(CtlCommand::Theme("Tokyo Night".into())));
        assert!(parse("theme").is_err());
        assert!(parse("dance").unwrap_err().contains("unknown command 'dance'"));
    }
}
