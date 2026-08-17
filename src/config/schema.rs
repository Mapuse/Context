use serde::{Deserialize, Serialize};
use serde::de::{self,Deserializer};

fn default_zero() -> u32 { 0 }
fn default_one() -> u32 { 1 }
fn default_256() -> u32 { 256 }
fn default_40() -> u32 { 40 }
fn default_hex_primary() -> String { "#ffffff".into() }
fn default_hex_accent() -> String { "#d1d5db".into() }
fn default_hex_info() -> String { "#9ca3af".into() }
fn default_hex_success() -> String { "#22c55e".into() }
fn default_hex_err() -> String { "#ef4444".into() }
fn default_hex_warning() -> String { "#f59e0b".into() }
fn default_hex_dim() -> String { "#6b7280".into() }
fn default_hex_text() -> String { "#f9fafb".into() }
fn default_empty_string() -> String { String::new() }
fn default_prompt_char() -> String { "❯".into() }
fn default_note_char() -> String { "▸".into() }
fn default_error_char() -> String { "✘".into() }
fn default_exit_prefix() -> String { "exit".into() }
fn default_success_char() -> String { "✓".into() }
fn default_warning_char() -> String { "⚠".into() }
fn default_arrow_char() -> String { "→".into() }
fn default_separator_char() -> String { "·".into() }
fn default_corner_tl() -> String { "╭".into() }
fn default_corner_tr() -> String { "╮".into() }
fn default_corner_bl() -> String { "╰".into() }
fn default_corner_br() -> String { "╯".into() }
fn default_horizontal() -> String { "─".into() }
fn default_vertical() -> String { "│".into() }
fn default_exit_label() -> String { "exit".into() }
fn default_history_path() -> String { "~/.ctx/.history".into() }
fn default_shell() -> String { "/bin/context".into() }
fn default_user_host_format() -> String { "{user}@{host}".into() }
fn default_cursor_symbol() -> String { "_".into() }

/// Multi-line format text: accepts a single string or array of strings in TOML.
/// Empty = don't show. Multiple entries = multiple lines.
/// Any UTF-8 character is accepted (emoji, CJK, Arabic, etc.)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MultiLineText(#[serde(deserialize_with = "deserialize_multiline")] Vec<String>);

impl MultiLineText {
    pub fn empty() -> Self { Self(Vec::new()) }
    pub fn is_empty(&self) -> bool { self.0.is_empty() || self.0.iter().all(|s| s.is_empty()) }
    pub fn lines(&self) -> &[String] { &self.0 }
    pub fn single(&self) -> String { self.0.join("\n") }
}

impl From<&str> for MultiLineText {
    fn from(s: &str) -> Self { Self(vec![s.to_string()]) }
}

impl From<String> for MultiLineText {
    fn from(s: String) -> Self { Self(vec![s]) }
}

impl MultiLineText {
    /// Expand `{key}` variables in each line. Accepts any UTF-8 characters.
    pub fn expand(&self, vars: &[(&str, &str)]) -> String {
        self.0.iter().map(|line| {
            let mut out = line.clone();
            for &(k, v) in vars {
                out = out.replace(&format!("{{{}}}", k), v);
            }
            out
        }).collect::<Vec<_>>().join("\n")
    }
}

fn deserialize_multiline<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct MultiLineVisitor;
    impl<'de> de::Visitor<'de> for MultiLineVisitor {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string or array of strings")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Vec<String>, E> {
            Ok(vec![v.to_string()])
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<Vec<String>, E> {
            Ok(vec![v])
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<String>, A::Error> {
            let mut items = Vec::new();
            while let Some(item) = seq.next_element::<String>()? {
                items.push(item);
            }
            Ok(items)
        }
        fn visit_none<E: de::Error>(self) -> Result<Vec<String>, E> { Ok(Vec::new()) }
        fn visit_unit<E: de::Error>(self) -> Result<Vec<String>, E> { Ok(Vec::new()) }
    }
    deserializer.deserialize_any(MultiLineVisitor)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub history: HistoryConfig,
    pub cursor: CursorConfig,
    pub prompt: PromptConfig,
    pub box_config: BoxConfig,
    pub colors: ColorsConfig,
    pub symbols: SymbolsConfig,
    pub ascii: AsciiConfig,
    pub execution: ExecutionConfig,
    pub startup: StartupConfig,
    pub editor: EditorConfig,
    pub display: DisplayConfig,
    pub signals: SignalsConfig,
    pub environment: EnvironmentConfig,
    pub branding: BrandingConfig,
    pub autosuggest: AutosuggestConfig,
    pub keybindings: KeybindingConfig,
    pub jobs: JobControlConfig,
    pub clipboard: ClipboardConfig,
    pub integration: IntegrationConfig,
    pub security: SecurityConfig,
    pub performance: PerformanceConfig,
    pub modes: ModesConfig,
    pub dynamic: DynamicColorsConfig,
    pub named_dirs: std::collections::HashMap<String, String>,
    pub python: PythonConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    pub max_size: u32,
    pub file: String,
    pub deduplicate: bool,
    pub ignore_space: bool,
    pub ignore_patterns: Vec<String>,
    pub share_across_sessions: bool,
    pub sync_on_command: bool,
    pub expire_days: u32,
    pub save_on_every_command: bool,
    pub search_case_sensitive: bool,
    pub substring_search: bool,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_size: default_256(),
            file: default_history_path(),
            deduplicate: true,
            ignore_space: true,
            ignore_patterns: vec![],
            share_across_sessions: true,
            sync_on_command: true,
            expire_days: 0,
            save_on_every_command: false,
            search_case_sensitive: false,
            substring_search: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CursorConfig {
    pub symbol: String,
    pub blink: bool,
    pub color: String,
    pub color_error: String,
    pub style: String,
    pub width: u32,
    pub format: MultiLineText,
    pub format_input_line: i32,
    pub format_align: String,
    pub format_padding: u32,
    pub format_colorize: bool,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            symbol: default_cursor_symbol(),
            blink: true,
            color: default_hex_dim(),
            color_error: default_hex_err(),
            style: "block".into(),
            width: 1,
            format: MultiLineText::empty(),
            format_input_line: -1,
            format_align: "left".into(),
            format_padding: 0,
            format_colorize: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptConfig {
    pub top_line_left: MultiLineText,
    pub top_line_right: MultiLineText,
    pub color_cwd: String,
    pub color_prompt: String,
    pub color_top: String,
    pub color_symbol: String,
    pub color_user: String,
    pub color_host: String,
    pub show_top_line: bool,
    pub gap_after_top: u32,
    pub gap_after_middle: u32,
    pub show_user_host: bool,
    pub user_host_format: String,
    pub cwd_max_depth: u32,
    pub show_git_branch: bool,
    pub git_branch_color: String,
    pub prompt_prefix: MultiLineText,
    pub prompt_suffix: MultiLineText,
    pub newline_before_prompt: bool,
    pub show_path: bool,
    pub right_prompt: MultiLineText,
    pub right_prompt_color: String,
    pub right_prompt_hide_threshold: f64,
    pub transient_prompt: bool,
    pub transient_prompt_format: MultiLineText,
    pub async_prompt: bool,
    pub async_prompt_cache_ms: u64,
    pub instant_prompt: bool,
    pub prompt_bold: bool,
    pub gap_before_prompt: u32,
    pub error_prompt_color: String,
    pub prompt_color_success: String,
    pub vi_prompt_insert: MultiLineText,
    pub vi_prompt_normal: MultiLineText,
    pub vi_prompt_visual: MultiLineText,
    pub vi_cmd_prompt: MultiLineText,
    pub vi_cmd_color: String,
    pub vi_cmd_color_error: String,
    pub vi_cmd_color_success: String,
    pub prompt_eol_escape: String,
    pub rprompt_eol_escape: String,
    pub show_branch_only_when_dirty: bool,
    pub git_dirty_char: String,
    pub git_clean_char: String,
    pub git_staged_char: String,
    pub git_untracked_char: String,
    pub git_ahead_char: String,
    pub git_behind_char: String,
    pub show_python_venv: bool,
    pub show_node_version: bool,
    pub show_rust_version: bool,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            top_line_left: MultiLineText::from("context"),
            top_line_right: MultiLineText::empty(),
            color_cwd: default_hex_info(),
            color_prompt: default_hex_success(),
            color_top: default_hex_primary(),
            color_symbol: default_hex_success(),
            color_user: default_hex_accent(),
            color_host: default_hex_info(),
            show_top_line: false,
            gap_after_top: default_zero(),
            gap_after_middle: default_zero(),
            show_user_host: false,
            user_host_format: default_user_host_format(),
            cwd_max_depth: default_zero(),
            show_git_branch: false,
            git_branch_color: default_hex_warning(),
            prompt_prefix: MultiLineText::empty(),
            prompt_suffix: MultiLineText::from("❯"),
            newline_before_prompt: false,
            show_path: true,
            right_prompt: MultiLineText::empty(),
            right_prompt_color: default_hex_dim(),
            right_prompt_hide_threshold: 0.8,
            transient_prompt: false,
            transient_prompt_format: MultiLineText::from("{user}@{host} {cwd} {suffix} "),
            async_prompt: false,
            async_prompt_cache_ms: 1000,
            instant_prompt: false,
            prompt_bold: false,
            gap_before_prompt: 0,
            error_prompt_color: default_hex_err(),
            prompt_color_success: default_hex_success(),
            vi_prompt_insert: MultiLineText::from("-- INSERT --"),
            vi_prompt_normal: MultiLineText::from("-- NORMAL --"),
            vi_prompt_visual: MultiLineText::from("-- VISUAL --"),
            vi_cmd_prompt: MultiLineText::from(":"),
            vi_cmd_color: default_hex_success(),
            vi_cmd_color_error: default_hex_err(),
            vi_cmd_color_success: default_hex_success(),
            prompt_eol_escape: "\\033[0m".into(),
            rprompt_eol_escape: "\\033[0m".into(),
            show_branch_only_when_dirty: false,
            git_dirty_char: "*".into(),
            git_clean_char: "".into(),
            git_staged_char: "+".into(),
            git_untracked_char: "?".into(),
            git_ahead_char: "↑".into(),
            git_behind_char: "↓".into(),
            show_python_venv: true,
            show_node_version: false,
            show_rust_version: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BoxConfig {
    pub enabled: bool,
    pub border_color: String,
    pub corner_tl: String,
    pub corner_tr: String,
    pub corner_bl: String,
    pub corner_br: String,
    pub horizontal_char: String,
    pub vertical_char: String,
    pub padding_left: u32,
    pub padding_right: u32,
    pub show_exit_code: bool,
    pub exit_label: String,
    pub exit_code_color: String,
    pub min_width: u32,
    pub gap_before: u32,
    pub gap_after: u32,
    pub title: MultiLineText,
    pub title_color: String,
    pub content_color: String,
    pub border_style: String,
    pub separator_char: String,
    pub show_title_only_when_busy: bool,
    pub top_line_padding: u32,
    pub bottom_line_char: String,
    pub box_width_mode: String,
}

impl Default for BoxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            border_color: default_hex_primary(),
            corner_tl: default_corner_tl(),
            corner_tr: default_corner_tr(),
            corner_bl: default_corner_bl(),
            corner_br: default_corner_br(),
            horizontal_char: default_horizontal(),
            vertical_char: default_vertical(),
            padding_left: default_one(),
            padding_right: default_one(),
            show_exit_code: true,
            exit_label: default_exit_label(),
            exit_code_color: default_hex_err(),
            min_width: default_40(),
            gap_before: default_zero(),
            gap_after: default_zero(),
            title: MultiLineText::empty(),
            title_color: default_hex_accent(),
            content_color: default_hex_text(),
            border_style: "rounded".into(),
            separator_char: default_separator_char(),
            show_title_only_when_busy: false,
            top_line_padding: default_one(),
            bottom_line_char: default_empty_string(),
            box_width_mode: "terminal".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorsConfig {
    pub primary: String,
    pub accent: String,
    pub info: String,
    pub success: String,
    pub err: String,
    pub warning: String,
    pub dim: String,
    pub text: String,
    pub bold: String,
    pub underline: String,
    pub reverse: String,
    pub bg_primary: String,
    pub bg_err: String,
    pub bg_success: String,
    pub bg_warning: String,
    pub bg_info: String,
    pub gradient_start: String,
    pub gradient_mid: String,
    pub gradient_end: String,
    pub rprompt_bg: String,
    pub transient: String,
    pub cwd_gradient_start: String,
    pub cwd_gradient_end: String,
    pub syntax_comment: String,
    pub syntax_string: String,
    pub syntax_variable: String,
    pub syntax_operator: String,
    pub syntax_command: String,
    pub syntax_flag: String,
    pub syntax_path: String,
    pub syntax_number: String,
    pub syntax_use_dynamic_colors: bool,
    pub input_color: String,
    pub input_use_accent_color: bool,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            primary: default_hex_primary(),
            accent: default_hex_accent(),
            info: default_hex_info(),
            success: default_hex_success(),
            err: default_hex_err(),
            warning: default_hex_warning(),
            dim: default_hex_dim(),
            text: default_hex_text(),
            bold: "#ffffff".into(),
            underline: default_hex_accent(),
            reverse: "#111827".into(),
            bg_primary: "#111827".into(),
            bg_err: "#3d0000".into(),
            bg_success: "#003d1a".into(),
            bg_warning: "#3d2e00".into(),
            bg_info: "#1f2937".into(),
            gradient_start: default_hex_primary(),
            gradient_mid: default_hex_info(),
            gradient_end: default_hex_success(),
            rprompt_bg: default_empty_string(),
            transient: default_hex_dim(),
            cwd_gradient_start: default_hex_accent(),
            cwd_gradient_end: default_hex_info(),
            syntax_comment: "#6b7280".into(),
            syntax_string: "#f59e0b".into(),
            syntax_variable: "#22d3ee".into(),
            syntax_operator: "#22c55e".into(),
            syntax_command: "#3b82f6".into(),
            syntax_flag: "#c084fc".into(),
            syntax_path: "#fb923c".into(),
            syntax_number: "#22d3ee".into(),
            syntax_use_dynamic_colors: true,
            input_color: default_empty_string(),
            input_use_accent_color: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SymbolsConfig {
    pub prompt_char: String,
    pub note_char: String,
    pub error_char: String,
    pub exit_prefix: String,
    pub success_char: String,
    pub warning_char: String,
    pub arrow_char: String,
    pub separator_char: String,
    pub git_branch_char: String,
    pub directory_char: String,
    pub file_char: String,
    pub executable_char: String,
    pub link_char: String,
    pub pipe_char: String,
    pub socket_char: String,
    pub continuation_char: String,
    pub job_char: String,
}

impl Default for SymbolsConfig {
    fn default() -> Self {
        Self {
            prompt_char: default_prompt_char(),
            note_char: default_note_char(),
            error_char: default_error_char(),
            exit_prefix: default_exit_prefix(),
            success_char: default_success_char(),
            warning_char: default_warning_char(),
            arrow_char: default_arrow_char(),
            separator_char: default_separator_char(),
            git_branch_char: "⌿".into(),
            directory_char: " ".into(),
            file_char: " ".into(),
            executable_char: "*".into(),
            link_char: "@".into(),
            pipe_char: "|".into(),
            socket_char: "=".into(),
            continuation_char: "·".into(),
            job_char: "&".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AsciiConfig {
    pub lines: Vec<String>,
    pub file: String,
    pub blocks: Vec<AsciiBlock>,
    pub show_in_box: bool,
    pub box_color: String,
    pub margin_top: u32,
    pub margin_bottom: u32,
    pub margin_left: u32,
    pub center: bool,
    pub color: String,
    pub animate: bool,
    pub animate_delay_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsciiBlock {
    pub color: String,
}

impl Default for AsciiConfig {
    fn default() -> Self {
        Self {
            lines: vec![],
            file: default_empty_string(),
            blocks: vec![],
            show_in_box: false,
            box_color: default_hex_primary(),
            margin_top: 1,
            margin_bottom: 1,
            margin_left: 2,
            center: false,
            color: default_empty_string(),
            animate: false,
            animate_delay_ms: 200,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecutionConfig {
    pub shell: String,
    pub timeout_seconds: u32,
    pub fork_method: String,
    pub umask: String,
    pub strip_env_on_exec: bool,
    pub path_override: String,
    pub cdspell: bool,
    pub float_precision: usize,
    pub bash_compat: bool,
    pub max_forks_per_command: u32,
    pub exit_on_pipefail: bool,
    pub command_not_found_hook: String,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            shell: default_shell(),
            timeout_seconds: default_zero(),
            fork_method: "fork".into(),
            umask: "0022".into(),
            strip_env_on_exec: false,
            path_override: default_empty_string(),
            cdspell: false,
            float_precision: 6,
            bash_compat: false,
            max_forks_per_command: 128,
            exit_on_pipefail: false,
            command_not_found_hook: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StartupConfig {
    pub show_welcome: bool,
    pub welcome_message: MultiLineText,
    pub welcome_color: String,
    pub run_commands: Vec<String>,
    pub startup_delay_ms: u32,
    pub show_config_path: bool,
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            show_welcome: true,
            welcome_message: MultiLineText::empty(),
            welcome_color: default_hex_text(),
            run_commands: vec![],
            startup_delay_ms: default_zero(),
            show_config_path: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorConfig {
    pub mode: String,
    pub bell: String,
    pub auto_cd: bool,
    pub expand_aliases: bool,
    pub colorize_output: bool,
    pub max_line_length: u32,
    pub word_delimiters: String,
    pub bracketed_paste: bool,
    pub auto_match_quotes: bool,
    pub vi_cursor_block: String,
    pub vi_cursor_insert: String,
    pub emacs_overwrite_mode: bool,
    pub hide_cursor_on_exec: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            mode: "emacs".into(),
            bell: "visible".into(),
            auto_cd: true,
            expand_aliases: true,
            colorize_output: true,
            max_line_length: 4096,
            word_delimiters: " \t\n|&;><`(){}[]$'\"\\".into(),
            bracketed_paste: true,
            auto_match_quotes: false,
            vi_cursor_block: "block".into(),
            vi_cursor_insert: "beam".into(),
            emacs_overwrite_mode: false,
            hide_cursor_on_exec: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    pub show_exit_code_on_error_only: bool,
    pub compact_mode: bool,
    pub color_mode: String,
    pub utf8_mode: String,
    pub show_command_duration: bool,
    pub duration_color: String,
    pub show_timestamp: bool,
    pub timestamp_format: String,
    pub timestamp_color: String,
    pub show_pid: bool,
    pub status_line: bool,
    pub status_line_format: MultiLineText,
    pub title_bar: bool,
    pub title_bar_format: MultiLineText,
    pub title_bar_color: String,
    pub show_session_info: bool,
    pub session_info_format: MultiLineText,
    pub powerline_symbols: bool,
    pub nerd_fonts: bool,
    pub show_command_summary: bool,
    pub command_summary_format: MultiLineText,

}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            show_exit_code_on_error_only: true,
            compact_mode: false,
            color_mode: "true_color".into(),
            utf8_mode: "full".into(),
            show_command_duration: false,
            duration_color: default_hex_dim(),
            show_timestamp: false,
            timestamp_format: "%H:%M:%S".into(),
            timestamp_color: default_hex_dim(),
            show_pid: false,
            status_line: false,
            status_line_format: MultiLineText::from("{cwd} | {exit_code} | {duration}"),
            title_bar: false,
            title_bar_format: MultiLineText::from("context — {cwd}"),
            title_bar_color: default_hex_dim(),
            show_session_info: false,
            session_info_format: MultiLineText::from("context {version} — {cwd}"),
            powerline_symbols: false,
            nerd_fonts: false,
            show_command_summary: false,
            command_summary_format: MultiLineText::from("[{status_char}] {command} (exit {exit_code})"),

        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SignalsConfig {
    pub sigint_action: String,
    pub sigquit_action: String,
    pub sigtstp_action: String,
    pub sigwinch_action: String,
    pub forward_signals_to_child: bool,
    pub reset_terminal_on_exit: bool,
    pub sigusr1_action: String,
    pub sigusr2_action: String,
}

impl Default for SignalsConfig {
    fn default() -> Self {
        Self {
            sigint_action: "cancel_line".into(),
            sigquit_action: "ignore".into(),
            sigtstp_action: "suspend".into(),
            sigwinch_action: "redraw".into(),
            forward_signals_to_child: true,
            reset_terminal_on_exit: true,
            sigusr1_action: "reload_config".into(),
            sigusr2_action: "ignore".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentConfig {
    pub passthrough: Vec<String>,
    pub filter: Vec<String>,
    pub set_defaults: Vec<String>,
    pub strip_on_exit: Vec<String>,
    pub inherit_parent: bool,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            passthrough: vec![],
            filter: vec![],
            set_defaults: vec!["TERM=xterm-256color".into()],
            strip_on_exit: vec![],
            inherit_parent: true,
        }
    }
}

fn default_branding_version() -> String { env!("CARGO_PKG_VERSION").into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrandingConfig {
    pub app_name: String,
    #[serde(default = "default_branding_version")]
    pub version: String,
    pub shell_name: String,
    pub author: String,
    pub tagline: String,
    pub show_version_on_start: bool,
    pub show_config_path_on_start: bool,
}

impl Default for BrandingConfig {
    fn default() -> Self {
        Self {
            app_name: "Context".into(),
            version: default_branding_version(),
            shell_name: "context".into(),
            author: String::new(),
            tagline: "a fast, fully configurable POSIX shell".into(),
            show_version_on_start: false,
            show_config_path_on_start: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutosuggestConfig {
    pub enabled: bool,
    pub min_chars: u32,
    pub strategy: String,
    pub highlight_color: String,
    pub accept_key: String,
    pub accept_word_key: String,
    pub max_suggestions: u32,
    pub case_sensitive: bool,
}

impl Default for AutosuggestConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_chars: 1,
            strategy: "history".into(),
            highlight_color: "#6b7280".into(),
            accept_key: "Right".into(),
            accept_word_key: "Alt+Right".into(),
            max_suggestions: 50,
            case_sensitive: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingConfig {
    pub bindings: Vec<KeybindingEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingEntry {
    pub key: String,
    pub widget: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct JobControlConfig {
    pub notify_on_job_done: bool,
    pub auto_report_stopped: bool,
    pub max_jobs: u32,
    pub warn_on_suspended: bool,
    pub done_format: MultiLineText,
    pub stopped_format: MultiLineText,
}

impl Default for JobControlConfig {
    fn default() -> Self {
        Self {
            notify_on_job_done: false,
            auto_report_stopped: true,
            max_jobs: 128,
            warn_on_suspended: true,
            done_format: MultiLineText::from("[done] {command} (exit {exit_code})"),
            stopped_format: MultiLineText::from("[stopped] {command} pid={pid}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipboardConfig {
    pub enabled: bool,
    pub method: String,
    pub yank_to_clipboard: bool,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            method: "auto".into(),
            yank_to_clipboard: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IntegrationConfig {
    pub enable_fzf: bool,
    pub enable_zoxide: bool,
    pub fzf_key_bindings: bool,
    pub zoxide_init: bool,
    pub starship_prompt: bool,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        Self {
            enable_fzf: false,
            enable_zoxide: false,
            fzf_key_bindings: true,
            zoxide_init: true,
            starship_prompt: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    pub restricted_mode: bool,
    pub no_exec_commands: Vec<String>,
    pub sanitize_path: bool,
    pub mask_secrets: bool,
    pub audit_log: bool,
    pub audit_log_path: String,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            restricted_mode: false,
            no_exec_commands: vec![],
            sanitize_path: true,
            mask_secrets: false,
            audit_log: false,
            audit_log_path: "~/.ctx_audit.log".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformanceConfig {
    pub max_history_load_lines: u32,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_history_load_lines: 100_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModesConfig {
    pub symbol_mode: String,
    pub border_mode: String,
    pub nerd_mode: String,
    pub powerline_mode: String,
    pub color_depth: String,
}

impl Default for ModesConfig {
    fn default() -> Self {
        Self {
            symbol_mode: "unicode".into(),
            border_mode: "unicode".into(),
            nerd_mode: "off".into(),
            powerline_mode: "off".into(),
            color_depth: "true_color".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DynamicColorsConfig {
    pub enabled: bool,
    pub wallpaper_path: Option<String>,
    pub fallback_to_terminal_bg: bool,
    pub color_mapping: std::collections::HashMap<String, String>,
}

impl Default for DynamicColorsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            wallpaper_path: None,
            fallback_to_terminal_bg: true,
            color_mapping: std::collections::HashMap::new(),
        }
    }
}

pub use cps::PythonConfig;
