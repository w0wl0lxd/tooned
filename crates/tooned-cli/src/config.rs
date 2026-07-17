// SPDX-License-Identifier: AGPL-3.0-only

//! User/project configuration file support for the `tooned` CLI.
//!
//! Configuration is loaded from (in precedence order):
//! - the `--config` command-line flag,
//! - the `TOONED_CONFIG` environment variable,
//! - `$XDG_CONFIG_HOME/tooned/config.toml`,
//! - `$HOME/.config/tooned/config.toml` (or `%USERPROFILE%\.config\tooned\config.toml`),
//! - `.tooned.toml` in the current directory.
//!
//! CLI flags always override config-file values.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tooned_core::ConversionOptions;

use crate::cli::FormatHint;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// Default `margin_pct` for conversion operations.
    pub margin_pct: Option<f64>,
    /// Default `max_input_bytes` for conversion operations.
    pub max_input_bytes: Option<usize>,
    /// Default `format_hint` ("json", "ndjson", "yaml", "toml", "csv", "tsv", "xml").
    pub format_hint: Option<String>,
    /// Default `precise_tokens` for `tooned check`.
    pub precise_tokens: Option<bool>,
    /// `tooned index watch` defaults.
    pub watch: Option<WatchConfig>,
}

#[derive(Debug, Default, Deserialize)]
pub struct WatchConfig {
    /// Quiet period, in milliseconds, before a filesystem change triggers
    /// an incremental `tooned index sync`.
    pub debounce_ms: Option<u64>,
}

impl Config {
    /// Load the configuration file. Returns an empty config if no config file
    /// is found. Errors only when an explicitly-specified file cannot be read
    /// or parsed.
    pub fn load(explicit: Option<&Path>) -> anyhow::Result<Self> {
        let path = match explicit {
            Some(p) => p.to_path_buf(),
            None => match Self::discover_path() {
                Some(p) => p,
                None => return Ok(Self::default()),
            },
        };

        if !path.is_file() {
            if explicit.is_some() {
                anyhow::bail!("config file not found: {}", path.display());
            }
            return Ok(Self::default());
        }

        let text = std::fs::read_to_string(&path)?;
        let config: Self = toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("failed to parse config file {}: {e}", path.display()))?;
        Ok(config)
    }

    fn discover_path() -> Option<PathBuf> {
        if let Some(env_path) = std::env::var_os("TOONED_CONFIG") {
            return Some(PathBuf::from(env_path));
        }

        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            let mut p = PathBuf::from(xdg);
            p.push("tooned");
            p.push("config.toml");
            if p.is_file() {
                return Some(p);
            }
        }

        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            let mut p = PathBuf::from(home);
            p.push(".config");
            p.push("tooned");
            p.push("config.toml");
            if p.is_file() {
                return Some(p);
            }
        }

        let local = PathBuf::from(".tooned.toml");
        if local.is_file() {
            return Some(local);
        }

        None
    }

    /// Parse the configured `format_hint` string into a typed value, if any.
    pub fn format_hint(&self) -> Option<FormatHint> {
        self.format_hint.as_ref().and_then(|s| match s.to_lowercase().as_str() {
            "json" => Some(FormatHint::Json),
            "ndjson" => Some(FormatHint::Ndjson),
            "yaml" => Some(FormatHint::Yaml),
            "toml" => Some(FormatHint::Toml),
            "csv" => Some(FormatHint::Csv),
            "tsv" => Some(FormatHint::Tsv),
            "xml" => Some(FormatHint::Xml),
            "msgpack" => Some(FormatHint::Msgpack),
            "cbor" => Some(FormatHint::Cbor),
            "json5" => Some(FormatHint::Json5),
            _ => None,
        })
    }

    /// Build a `ConversionOptions` by layering config-file defaults underneath
    /// explicit CLI values. `max_bytes` is in `u64` because CLI flags expose
    /// it as such before clamping to `usize`.
    pub fn conversion_options(
        &self,
        margin: Option<f64>,
        max_bytes: Option<u64>,
        format_hint: Option<FormatHint>,
        precise: Option<bool>,
    ) -> ConversionOptions {
        let mut opts = ConversionOptions::default();

        if let Some(m) = margin.or(self.margin_pct) {
            opts.margin_pct = m;
        }

        let configured_bytes = self.max_input_bytes.map(|n| n as u64);
        if let Some(b) = max_bytes.or(configured_bytes) {
            opts.max_input_bytes = match usize::try_from(b) {
                Ok(clamped) => clamped,
                Err(_) => usize::MAX,
            };
        }

        if let Some(h) = format_hint.or(self.format_hint()) {
            opts.format_hint = Some(h.into());
        }

        if let Some(p) = precise.or(self.precise_tokens) {
            opts.precise_tokens = p;
        }

        opts
    }
}
