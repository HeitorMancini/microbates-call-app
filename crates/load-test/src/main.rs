use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use load_test::{Codec, CommandHook, Impairment, ImpairmentHook, NoopHook, RunConfig, Thresholds};

/// Teste de carga/qualidade do Sheeshcord: N participantes, 1 tela sintética.
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// URL do LiveKit (ws://IP:7880).
    #[arg(long, env = "LIVEKIT_URL", default_value = "ws://127.0.0.1:7880")]
    url: String,
    #[arg(long, env = "LIVEKIT_API_KEY", default_value = "devkey")]
    api_key: String,
    #[arg(
        long,
        env = "LIVEKIT_API_SECRET",
        default_value = "devsecretdevsecretdevsecretdevsecret",
        hide_env_values = true
    )]
    api_secret: String,
    #[arg(long, default_value = "load-test")]
    room: String,
    /// Total de participantes (1 publicador + N-1 espectadores).
    #[arg(short = 'n', long, default_value_t = 7)]
    participants: usize,
    #[arg(long, default_value_t = 1920)]
    width: u32,
    #[arg(long, default_value_t = 1080)]
    height: u32,
    #[arg(long, default_value_t = 30)]
    fps: u32,
    /// Bitrate máximo do publicador (kbps).
    #[arg(long, default_value_t = 3000)]
    bitrate_kbps: u32,
    /// vp8 | h264 | vp9 | av1
    #[arg(long, default_value = "vp8")]
    codec: Codec,
    #[arg(long, default_value_t = 5)]
    warmup_secs: u64,
    #[arg(long, default_value_t = 20)]
    duration_secs: u64,
    /// Arquivo JSON de saída (stdout recebe o resumo).
    #[arg(long, short)]
    output: Option<PathBuf>,

    #[arg(long, default_value_t = Thresholds::default().min_bitrate_kbps)]
    min_bitrate_kbps: f64,
    #[arg(long, default_value_t = Thresholds::default().min_fps)]
    min_fps: f64,
    #[arg(long, default_value_t = Thresholds::default().max_loss_pct)]
    max_loss_pct: f64,
    #[arg(long, default_value_t = Thresholds::default().max_jitter_ms)]
    max_jitter_ms: f64,
    #[arg(long, default_value_t = Thresholds::default().max_connect_ms)]
    max_connect_ms: f64,

    /// Perda de pacotes simulada (%), repassada ao gancho como LT_LOSS_PCT.
    #[arg(long, default_value_t = 0.0)]
    loss_pct: f64,
    /// Latência simulada (ms), repassada como LT_LATENCY_MS.
    #[arg(long, default_value_t = 0)]
    latency_ms: u32,
    /// Jitter simulado (ms), repassado como LT_JITTER_MS.
    #[arg(long, default_value_t = 0)]
    jitter_ms: u32,
    /// Comando executado para ativar a degradação de rede (ex.: iniciar o clumsy).
    #[arg(long)]
    impair_cmd: Option<String>,
    /// Comando executado ao final para remover a degradação.
    #[arg(long)]
    restore_cmd: Option<String>,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "load_test=info,livekit=warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let args = Args::parse();

    let cfg = RunConfig {
        url: args.url,
        api_key: args.api_key,
        api_secret: args.api_secret,
        room: args.room,
        participants: args.participants,
        width: args.width,
        height: args.height,
        fps: args.fps,
        bitrate_kbps: args.bitrate_kbps,
        codec: args.codec,
        warmup: Duration::from_secs(args.warmup_secs),
        duration: Duration::from_secs(args.duration_secs),
        ..RunConfig::default()
    };
    let thresholds = Thresholds {
        min_bitrate_kbps: args.min_bitrate_kbps,
        min_fps: args.min_fps,
        max_loss_pct: args.max_loss_pct,
        max_jitter_ms: args.max_jitter_ms,
        max_connect_ms: args.max_connect_ms,
    };
    let impairment = Impairment {
        loss_pct: args.loss_pct,
        latency_ms: args.latency_ms,
        jitter_ms: args.jitter_ms,
    };
    let hook: Box<dyn ImpairmentHook> = match args.impair_cmd {
        Some(cmd) => Box::new(CommandHook::new(cmd, args.restore_cmd)),
        None => {
            if !impairment.is_none() {
                eprintln!(
                    "aviso: --loss-pct/--latency-ms/--jitter-ms sem --impair-cmd; aplique manualmente, ex.:\n  {}\n  {}",
                    impairment.clumsy_command(50000, 50100),
                    impairment.netem_command("eth0")
                );
            }
            Box::new(NoopHook)
        }
    };

    let report = match load_test::run(&cfg, &thresholds, Some(impairment), hook.as_ref()).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("erro: {e:#}");
            return ExitCode::from(2);
        }
    };

    print!("{}", report.human_summary());
    let json = serde_json::to_string_pretty(&report).expect("report serializes");
    match &args.output {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &json) {
                eprintln!("erro ao gravar {}: {e}", path.display());
                return ExitCode::from(2);
            }
        }
        None => println!("{json}"),
    }
    if report.passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
