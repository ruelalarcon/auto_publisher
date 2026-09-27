use std::{ffi::OsString, path::Path};

const TOKEN_ERROR: &str = "Set DISCORD_TOKEN to your bot token in .env or the environment.";

pub fn token() -> Result<String, &'static str> {
    load(std::env::var_os("DISCORD_TOKEN"), Path::new(".env"))
}

fn load(environment: Option<OsString>, path: &Path) -> Result<String, &'static str> {
    if let Some(token) = environment {
        return validate(&token.into_string().map_err(|_| TOKEN_ERROR)?);
    }
    let source = std::fs::read_to_string(path).map_err(
        |_| "Cannot read .env. Copy .env.example to .env or set DISCORD_TOKEN in the environment.",
    )?;
    parse(&source)
}

fn parse(source: &str) -> Result<String, &'static str> {
    let mut token = None;
    // Parse without mutating the process environment. Never print parser errors:
    // they can contain the secret. Accept a UTF-8 BOM from Windows editors.
    for entry in dotenvy::from_read_iter(source.trim_start_matches('\u{feff}').as_bytes()) {
        let (key, value) =
            entry.map_err(|_| "Invalid .env syntax. Check the DISCORD_TOKEN entry.")?;
        if key == "DISCORD_TOKEN" && token.is_none() {
            token = Some(value);
        }
    }
    validate(&token.ok_or(TOKEN_ERROR)?)
}

fn validate(token: &str) -> Result<String, &'static str> {
    let token = token.trim();
    if token.is_empty()
        || token == "YOUR_BOT_TOKEN_HERE"
        || !token.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(TOKEN_ERROR);
    }
    Ok(token.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_quoted_and_windows_env_files() {
        for source in [
            "DISCORD_TOKEN=test-secret",
            "# Bot\nDISCORD_TOKEN=' test-secret '\n",
            "\u{feff}DISCORD_TOKEN=\"test-secret\"\r\n",
            "export DISCORD_TOKEN=test-secret # comment",
            "UNRELATED=value\nDISCORD_TOKEN=test-secret",
            "DISCORD_TOKEN=test-secret\nDISCORD_TOKEN=second",
        ] {
            assert_eq!(parse(source), Ok("test-secret".into()));
        }
    }

    #[test]
    fn rejects_missing_invalid_and_placeholder_tokens_without_leaks() {
        for source in [
            "",
            "OTHER=test-secret",
            "DISCORD_TOKEN=",
            "DISCORD_TOKEN='test-secret",
            "DISCORD_TOKEN=YOUR_BOT_TOKEN_HERE",
            "DISCORD_TOKEN=' YOUR_BOT_TOKEN_HERE '",
            "DISCORD_TOKEN='test-secret with spaces'",
            "DISCORD_TOKEN='test-secret\nnewline'",
        ] {
            assert!(!parse(source).unwrap_err().contains("test-secret"));
        }
    }

    #[test]
    fn environment_takes_precedence_and_needs_no_file() {
        let missing = Path::new("nonexistent-config-test/.env");
        assert_eq!(
            load(Some("test-secret".into()), missing),
            Ok("test-secret".into())
        );
        assert_eq!(load(Some("".into()), missing), Err(TOKEN_ERROR));
        assert!(
            load(None, missing)
                .unwrap_err()
                .contains("Cannot read .env")
        );
    }
}
