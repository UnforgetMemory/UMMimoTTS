use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub mimo_api_key: String,
    pub server_port: u16,
    pub allowed_origins: Vec<String>,
    pub max_concurrent_tasks: usize,
    pub task_cleanup_hours: u64,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        let mimo_api_key = std::env::var("MIMO_API_KEY").unwrap_or_default();
        let server_port = std::env::var("SERVER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(30231);

        let allowed_origins = std::env::var("ALLOWED_ORIGINS")
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(|_| vec!["http://localhost:5173".to_string()]);

        let max_concurrent_tasks = std::env::var("MAX_CONCURRENT_TASKS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(5);

        let task_cleanup_hours = std::env::var("TASK_CLEANUP_HOURS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(24);

        Self {
            mimo_api_key,
            server_port,
            allowed_origins,
            max_concurrent_tasks,
            task_cleanup_hours,
        }
    }

    /// 从命令行参数解析配置
    pub fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut config = Self::from_env();

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--port" | "-p" => {
                    if let Some(port_str) = args.get(i + 1) {
                        if let Ok(port) = port_str.parse::<u16>() {
                            config.server_port = port;
                        }
                    }
                    i += 2;
                }
                "--help" | "-h" => {
                    Self::print_help();
                    std::process::exit(0);
                }
                _ => {
                    i += 1;
                }
            }
        }

        config
    }

    /// 检查端口是否可用，如果不可用则自动寻找可用端口
    pub fn find_available_port(&mut self) {
        if Self::is_port_available(self.server_port) {
            return;
        }

        tracing::warn!("Port {} is in use, searching for available port...", self.server_port);

        // 从当前端口 +1 开始查找，最多尝试 100 个端口
        for offset in 1..=100 {
            let candidate = self.server_port.wrapping_add(offset);
            if candidate >= 1024 && Self::is_port_available(candidate) {
                tracing::info!("Found available port: {}", candidate);
                self.server_port = candidate;
                return;
            }
        }

        // 如果还没找到，从 10000 开始随机查找
        for port in 10000..65535 {
            if Self::is_port_available(port) {
                tracing::info!("Found available port: {}", port);
                self.server_port = port;
                return;
            }
        }

        tracing::error!("No available port found!");
        std::process::exit(1);
    }

    /// 检查端口是否可用
    fn is_port_available(port: u16) -> bool {
        std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
    }

    fn print_help() {
        println!("UM-MIMO-TTS Server v{}", env!("CARGO_PKG_VERSION"));
        println!();
        println!("USAGE:");
        println!("  um-mimo-tts-server [OPTIONS]");
        println!();
        println!("OPTIONS:");
        println!("  -p, --port <PORT>    Server port (default: 30231)");
        println!("  -h, --help           Print help");
        println!();
        println!("ENVIRONMENT VARIABLES:");
        println!("  MIMO_API_KEY            MIMO API key");
        println!("  SERVER_PORT             Server port (can also use --port)");
        println!("  MAX_CONCURRENT_TASKS    Max concurrent tasks (default: 5)");
        println!("  RUST_LOG                Log level (default: info)");
        println!();
        println!("EXAMPLES:");
        println!("  um-mimo-tts-server                    # Start on default port 30231");
        println!("  um-mimo-tts-server --port 8080        # Start on port 8080");
        println!("  um-mimo-tts-server -p 3000            # Start on port 3000");
        println!();
        println!("If the specified port is in use, the server will automatically");
        println!("find the next available port.");
    }
}

pub type AppConfig = Arc<Config>;
