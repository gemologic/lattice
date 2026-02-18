use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Parser};
use tracing::warn;

#[derive(Clone, Debug, Parser)]
#[command(name = "lattice")]
pub struct Config {
    #[arg(long, env = "LATTICE_PORT", default_value_t = 7400)]
    pub port: u16,

    #[arg(long, env = "LATTICE_DB_URL", default_value = "sqlite://./lattice.db")]
    pub db_url: String,

    #[arg(long, env = "LATTICE_TOKEN")]
    pub token: Option<String>,

    #[arg(long, env = "LATTICE_LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    #[arg(long, env = "LATTICE_STORAGE_DIR", default_value = "./storage")]
    pub storage_dir: PathBuf,

    #[arg(long, env = "LATTICE_MAX_FILE_SIZE", default_value_t = 10 * 1024 * 1024)]
    pub max_file_size: u64,

    #[command(flatten)]
    pub rate_limits: RateLimitConfig,
}

#[derive(Clone, Debug, Args)]
pub struct RateLimitConfig {
    #[arg(
        long = "rate-limit-read-per-min",
        env = "LATTICE_RATE_LIMIT_READ_PER_MIN",
        default_value_t = 240
    )]
    pub read_per_min: u32,

    #[arg(
        long = "rate-limit-read-burst",
        env = "LATTICE_RATE_LIMIT_READ_BURST",
        default_value_t = 60
    )]
    pub read_burst: u32,

    #[arg(
        long = "rate-limit-write-per-min",
        env = "LATTICE_RATE_LIMIT_WRITE_PER_MIN",
        default_value_t = 120
    )]
    pub write_per_min: u32,

    #[arg(
        long = "rate-limit-write-burst",
        env = "LATTICE_RATE_LIMIT_WRITE_BURST",
        default_value_t = 30
    )]
    pub write_burst: u32,

    #[arg(
        long = "rate-limit-attachment-per-min",
        env = "LATTICE_RATE_LIMIT_ATTACHMENT_PER_MIN",
        default_value_t = 30
    )]
    pub attachment_per_min: u32,

    #[arg(
        long = "rate-limit-attachment-burst",
        env = "LATTICE_RATE_LIMIT_ATTACHMENT_BURST",
        default_value_t = 10
    )]
    pub attachment_burst: u32,

    #[arg(
        long = "rate-limit-webhook-test-per-min",
        env = "LATTICE_RATE_LIMIT_WEBHOOK_TEST_PER_MIN",
        default_value_t = 20
    )]
    pub webhook_test_per_min: u32,

    #[arg(
        long = "rate-limit-webhook-test-burst",
        env = "LATTICE_RATE_LIMIT_WEBHOOK_TEST_BURST",
        default_value_t = 5
    )]
    pub webhook_test_burst: u32,

    #[arg(
        long = "rate-limit-mcp-per-min",
        env = "LATTICE_RATE_LIMIT_MCP_PER_MIN",
        default_value_t = 80
    )]
    pub mcp_per_min: u32,

    #[arg(
        long = "rate-limit-mcp-burst",
        env = "LATTICE_RATE_LIMIT_MCP_BURST",
        default_value_t = 20
    )]
    pub mcp_burst: u32,

    #[arg(
        long = "rate-limit-sse-connect-per-min",
        env = "LATTICE_RATE_LIMIT_SSE_CONNECT_PER_MIN",
        default_value_t = 40
    )]
    pub sse_connect_per_min: u32,

    #[arg(
        long = "rate-limit-sse-connect-burst",
        env = "LATTICE_RATE_LIMIT_SSE_CONNECT_BURST",
        default_value_t = 10
    )]
    pub sse_connect_burst: u32,

    #[arg(
        long = "rate-limit-sse-max-per-identity",
        env = "LATTICE_RATE_LIMIT_SSE_MAX_PER_IDENTITY",
        default_value_t = 10
    )]
    pub sse_max_per_identity: u32,

    #[arg(
        long = "rate-limit-sse-max-global",
        env = "LATTICE_RATE_LIMIT_SSE_MAX_GLOBAL",
        default_value_t = 400
    )]
    pub sse_max_global: u32,

    #[arg(long = "max-request-body-bytes", env = "LATTICE_MAX_REQUEST_BODY_BYTES", default_value_t = 12 * 1024 * 1024)]
    pub max_request_body_bytes: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            read_per_min: 240,
            read_burst: 60,
            write_per_min: 120,
            write_burst: 30,
            attachment_per_min: 30,
            attachment_burst: 10,
            webhook_test_per_min: 20,
            webhook_test_burst: 5,
            mcp_per_min: 80,
            mcp_burst: 20,
            sse_connect_per_min: 40,
            sse_connect_burst: 10,
            sse_max_per_identity: 10,
            sse_max_global: 400,
            max_request_body_bytes: 12 * 1024 * 1024,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let config = <Self as Parser>::parse();
        config.validate();
        config
    }

    pub fn auth_enabled(&self) -> bool {
        self.token
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())
    }

    pub fn ensure_storage_dir(&self) -> std::io::Result<()> {
        ensure_directory(&self.storage_dir)
    }

    pub fn log_startup_warnings(&self) {
        if !self.auth_enabled() {
            warn!("LATTICE_TOKEN is unset, auth is disabled and all requests are allowed");
            warn!(
                "no-auth mode enabled, rate limiting identity falls back to forwarded client IP headers"
            );
        }
    }

    fn validate(&self) {
        assert_non_zero_u32(
            "LATTICE_RATE_LIMIT_READ_PER_MIN",
            self.rate_limits.read_per_min,
        );
        assert_non_zero_u32("LATTICE_RATE_LIMIT_READ_BURST", self.rate_limits.read_burst);
        assert_non_zero_u32(
            "LATTICE_RATE_LIMIT_WRITE_PER_MIN",
            self.rate_limits.write_per_min,
        );
        assert_non_zero_u32(
            "LATTICE_RATE_LIMIT_WRITE_BURST",
            self.rate_limits.write_burst,
        );
        assert_non_zero_u32(
            "LATTICE_RATE_LIMIT_ATTACHMENT_PER_MIN",
            self.rate_limits.attachment_per_min,
        );
        assert_non_zero_u32(
            "LATTICE_RATE_LIMIT_ATTACHMENT_BURST",
            self.rate_limits.attachment_burst,
        );
        assert_non_zero_u32(
            "LATTICE_RATE_LIMIT_WEBHOOK_TEST_PER_MIN",
            self.rate_limits.webhook_test_per_min,
        );
        assert_non_zero_u32(
            "LATTICE_RATE_LIMIT_WEBHOOK_TEST_BURST",
            self.rate_limits.webhook_test_burst,
        );
        assert_non_zero_u32(
            "LATTICE_RATE_LIMIT_MCP_PER_MIN",
            self.rate_limits.mcp_per_min,
        );
        assert_non_zero_u32("LATTICE_RATE_LIMIT_MCP_BURST", self.rate_limits.mcp_burst);
        assert_non_zero_u32(
            "LATTICE_RATE_LIMIT_SSE_CONNECT_PER_MIN",
            self.rate_limits.sse_connect_per_min,
        );
        assert_non_zero_u32(
            "LATTICE_RATE_LIMIT_SSE_CONNECT_BURST",
            self.rate_limits.sse_connect_burst,
        );
        assert_non_zero_u32(
            "LATTICE_RATE_LIMIT_SSE_MAX_PER_IDENTITY",
            self.rate_limits.sse_max_per_identity,
        );
        assert_non_zero_u32(
            "LATTICE_RATE_LIMIT_SSE_MAX_GLOBAL",
            self.rate_limits.sse_max_global,
        );
        assert_non_zero_usize(
            "LATTICE_MAX_REQUEST_BODY_BYTES",
            self.rate_limits.max_request_body_bytes,
        );
    }
}

fn ensure_directory(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)
}

fn assert_non_zero_u32(key: &'static str, value: u32) {
    assert!(value > 0, "{key} must be greater than 0");
}

fn assert_non_zero_usize(key: &'static str, value: usize) {
    assert!(value > 0, "{key} must be greater than 0");
}
