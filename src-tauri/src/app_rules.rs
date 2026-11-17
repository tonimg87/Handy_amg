//! Fork amg: per-app dictation rules.
//!
//! When enabled, the plain transcribe shortcut looks at the app that had focus
//! when recording started and decides from the user's rules whether to paste
//! the raw transcription or post-process it with a given prompt. The
//! post-process shortcut still forces post-processing.

use log::{debug, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Mutex;
use tauri::AppHandle;

use crate::settings::{self, AppSettings};

/// The app that had focus when a recording started.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct ForegroundApp {
    /// Executable file name, e.g. `chrome.exe`.
    pub process_name: String,
    pub window_title: String,
}

/// One per-app rule. The first rule that matches wins.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct AppRule {
    pub id: String,
    /// Executable name, with or without `.exe`, case-insensitive.
    /// Empty matches any app.
    #[serde(default)]
    pub process_name: String,
    /// Text the window title must contain, case-insensitive. Empty matches
    /// any title.
    #[serde(default)]
    pub title_contains: String,
    /// Prompt to post-process with; `None` pastes the raw transcription.
    #[serde(default)]
    pub prompt_id: Option<String>,
}

/// What a finished dictation should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub post_process: bool,
    /// Prompt to use instead of the selected one, when post-processing.
    pub prompt_override: Option<String>,
}

static LAST_TARGET: Mutex<Option<ForegroundApp>> = Mutex::new(None);

/// Reads the focused app and remembers it for the current recording.
pub fn capture_target() {
    let target = read_foreground();
    debug!("Dictation target app: {:?}", target);
    if let Ok(mut last) = LAST_TARGET.lock() {
        *last = target;
    }
}

/// The app captured when the last recording started.
pub fn last_target() -> Option<ForegroundApp> {
    LAST_TARGET.lock().ok().and_then(|last| last.clone())
}

fn normalize_process(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    lower
        .strip_suffix(".exe")
        .map(str::to_string)
        .unwrap_or(lower)
}

fn rule_matches(rule: &AppRule, app: &ForegroundApp) -> bool {
    let process = normalize_process(&rule.process_name);
    let title = rule.title_contains.trim().to_lowercase();
    if process.is_empty() && title.is_empty() {
        return false;
    }
    let process_ok = process.is_empty() || process == normalize_process(&app.process_name);
    let title_ok = title.is_empty() || app.window_title.to_lowercase().contains(&title);
    process_ok && title_ok
}

/// The first rule that matches `app`.
pub fn find_rule<'a>(rules: &'a [AppRule], app: &ForegroundApp) -> Option<&'a AppRule> {
    rules.iter().find(|rule| rule_matches(rule, app))
}

/// Decides how to finish a dictation. `forced` is true for the post-process
/// shortcut. Without rules (or with post-processing off) this keeps Handy's
/// original behavior.
pub fn decide(settings: &AppSettings, target: Option<&ForegroundApp>, forced: bool) -> Decision {
    if !settings.app_rules_enabled || !settings.post_process_enabled {
        return Decision {
            post_process: forced,
            prompt_override: None,
        };
    }

    let rule = target.and_then(|app| find_rule(&settings.app_rules, app));
    let wanted = match rule {
        Some(rule) => {
            debug!("App rule '{}' matched {:?}", rule.id, target);
            rule.prompt_id.clone()
        }
        None => settings.app_rules_default_prompt_id.clone(),
    };

    // A rule can point at a prompt that was deleted since.
    let prompt = wanted.filter(|id| {
        let exists = settings.post_process_prompts.iter().any(|p| &p.id == id);
        if !exists {
            warn!("App rule prompt '{}' no longer exists; pasting raw", id);
        }
        exists
    });

    Decision {
        post_process: forced || prompt.is_some(),
        prompt_override: prompt,
    }
}

#[cfg(target_os = "windows")]
fn read_foreground() -> Option<ForegroundApp> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    };

    // SAFETY: the window handle is checked before use, buffers are sized by
    // the calls that fill them, and the process handle is closed before return.
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() {
            return None;
        }

        let title_len = GetWindowTextLengthW(window).max(0) as usize;
        let mut title_buf = vec![0u16; title_len + 1];
        let copied = GetWindowTextW(window, &mut title_buf).max(0) as usize;
        let window_title = String::from_utf16_lossy(&title_buf[..copied.min(title_buf.len())]);

        let mut process_id = 0u32;
        if GetWindowThreadProcessId(window, Some(&mut process_id)) == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
        let mut path_buf = [0u16; 1024];
        let mut size = path_buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path_buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);
        result.ok()?;

        let full_path = String::from_utf16_lossy(&path_buf[..(size as usize).min(path_buf.len())]);
        let process_name = full_path
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(&full_path)
            .to_string();

        Some(ForegroundApp {
            process_name,
            window_title,
        })
    }
}

#[cfg(not(target_os = "windows"))]
fn read_foreground() -> Option<ForegroundApp> {
    None
}

#[tauri::command]
#[specta::specta]
pub fn change_app_rules_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.app_rules_enabled = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_app_rules(app: AppHandle, rules: Vec<AppRule>) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.app_rules = rules;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_app_rules_default_prompt(app: AppHandle, id: Option<String>) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.app_rules_default_prompt_id = id;
    settings::write_settings(&app, settings);
    Ok(())
}

/// The app that had focus when the last recording started.
#[tauri::command]
#[specta::specta]
pub fn get_last_target_app() -> Option<ForegroundApp> {
    last_target()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{get_default_settings, LLMPrompt};

    fn app(process: &str, title: &str) -> ForegroundApp {
        ForegroundApp {
            process_name: process.to_string(),
            window_title: title.to_string(),
        }
    }

    fn rule(id: &str, process: &str, title: &str, prompt: Option<&str>) -> AppRule {
        AppRule {
            id: id.to_string(),
            process_name: process.to_string(),
            title_contains: title.to_string(),
            prompt_id: prompt.map(str::to_string),
        }
    }

    fn settings_with_rules() -> AppSettings {
        let mut s = get_default_settings();
        s.post_process_enabled = true;
        s.app_rules_enabled = true;
        s.post_process_prompts = vec![
            LLMPrompt {
                id: "clean".into(),
                name: "Limpieza".into(),
                prompt: "x".into(),
            },
            LLMPrompt {
                id: "mail".into(),
                name: "Correo".into(),
                prompt: "y".into(),
            },
        ];
        s.post_process_selected_prompt_id = Some("clean".into());
        s.app_rules_default_prompt_id = Some("clean".into());
        s.app_rules = vec![
            rule("claude", "Claude.exe", "", None),
            rule("gmail", "chrome.exe", "Gmail", Some("mail")),
            rule("outlook", "olk", "", Some("mail")),
        ];
        s
    }

    #[test]
    fn process_match_ignores_case_and_exe() {
        let s = settings_with_rules();
        let d = decide(&s, Some(&app("claude.EXE", "Claude")), false);
        assert_eq!(
            d,
            Decision {
                post_process: false,
                prompt_override: None
            }
        );
        let d = decide(&s, Some(&app("olk.exe", "Bandeja")), false);
        assert_eq!(d.prompt_override.as_deref(), Some("mail"));
    }

    #[test]
    fn title_filter_applies_within_process() {
        let s = settings_with_rules();
        let gmail = decide(&s, Some(&app("chrome.exe", "Recibidos - Gmail")), false);
        assert_eq!(gmail.prompt_override.as_deref(), Some("mail"));
        let other = decide(&s, Some(&app("chrome.exe", "Morningstar")), false);
        assert_eq!(other.prompt_override.as_deref(), Some("clean"));
        assert!(other.post_process);
    }

    #[test]
    fn forced_shortcut_still_post_processes_raw_apps() {
        let s = settings_with_rules();
        let d = decide(&s, Some(&app("Claude.exe", "")), true);
        assert!(d.post_process);
        assert_eq!(d.prompt_override, None);
    }

    #[test]
    fn disabled_rules_keep_original_behavior() {
        let mut s = settings_with_rules();
        s.app_rules_enabled = false;
        let d = decide(&s, Some(&app("olk.exe", "")), false);
        assert!(!d.post_process);
        s.app_rules_enabled = true;
        s.post_process_enabled = false;
        let d = decide(&s, Some(&app("olk.exe", "")), false);
        assert!(!d.post_process);
    }

    #[test]
    fn missing_prompt_falls_back_to_raw() {
        let mut s = settings_with_rules();
        s.app_rules[2].prompt_id = Some("deleted".into());
        let d = decide(&s, Some(&app("olk.exe", "")), false);
        assert!(!d.post_process);
    }

    #[test]
    fn empty_rule_never_matches() {
        let r = rule("empty", " ", "", Some("mail"));
        assert!(!rule_matches(&r, &app("chrome.exe", "x")));
    }

    #[test]
    fn unknown_target_uses_default() {
        let s = settings_with_rules();
        let d = decide(&s, None, false);
        assert_eq!(d.prompt_override.as_deref(), Some("clean"));
    }
}
