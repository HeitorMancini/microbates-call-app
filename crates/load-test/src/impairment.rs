//! Network impairment hook (packet loss / latency / jitter).
//!
//! The load test never installs or drives a network emulator by itself. Instead
//! it calls an [`ImpairmentHook`] after everyone is connected and clears it at
//! the end. The bundled [`CommandHook`] runs user-supplied shell commands
//! (e.g. start/stop `clumsy` on Windows or `tc netem` on Linux) with the
//! requested values exposed as environment variables:
//! `LT_LOSS_PCT`, `LT_LATENCY_MS`, `LT_JITTER_MS`.

use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Impairment {
    pub loss_pct: f64,
    pub latency_ms: u32,
    pub jitter_ms: u32,
}

impl Impairment {
    pub fn is_none(&self) -> bool {
        self.loss_pct <= 0.0 && self.latency_ms == 0 && self.jitter_ms == 0
    }

    pub fn env_vars(&self) -> [(&'static str, String); 3] {
        [
            ("LT_LOSS_PCT", format!("{}", self.loss_pct)),
            ("LT_LATENCY_MS", self.latency_ms.to_string()),
            ("LT_JITTER_MS", self.jitter_ms.to_string()),
        ]
    }

    /// Suggested `clumsy` command line (Windows, WinDivert) limited to the LiveKit UDP range.
    pub fn clumsy_command(&self, udp_from: u16, udp_to: u16) -> String {
        let mut cmd = format!(
            "clumsy.exe --filter \"udp and ((udp.SrcPort >= {udp_from} and udp.SrcPort <= {udp_to}) or (udp.DstPort >= {udp_from} and udp.DstPort <= {udp_to}))\""
        );
        if self.latency_ms > 0 {
            cmd.push_str(&format!(" --lag on --lag-time {}", self.latency_ms));
        }
        if self.loss_pct > 0.0 {
            cmd.push_str(&format!(" --drop on --drop-chance {:.1}", self.loss_pct));
        }
        cmd
    }

    /// Suggested Linux `tc netem` command for interface `dev`.
    pub fn netem_command(&self, dev: &str) -> String {
        let mut cmd = format!("sudo tc qdisc replace dev {dev} root netem");
        if self.latency_ms > 0 || self.jitter_ms > 0 {
            cmd.push_str(&format!(" delay {}ms", self.latency_ms));
            if self.jitter_ms > 0 {
                cmd.push_str(&format!(" {}ms", self.jitter_ms));
            }
        }
        if self.loss_pct > 0.0 {
            cmd.push_str(&format!(" loss {}%", self.loss_pct));
        }
        cmd
    }
}

pub trait ImpairmentHook: Send + Sync {
    fn apply(&self, impairment: &Impairment) -> anyhow::Result<()>;
    fn clear(&self) -> anyhow::Result<()>;
}

/// Does nothing (default when no command is configured).
pub struct NoopHook;

impl ImpairmentHook for NoopHook {
    fn apply(&self, _: &Impairment) -> anyhow::Result<()> {
        Ok(())
    }

    fn clear(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Runs shell commands to enable/disable an external network emulator.
pub struct CommandHook {
    pub apply_cmd: String,
    pub clear_cmd: Option<String>,
    impairment: std::sync::Mutex<Impairment>,
}

impl CommandHook {
    pub fn new(apply_cmd: String, clear_cmd: Option<String>) -> Self {
        Self {
            apply_cmd,
            clear_cmd,
            impairment: Default::default(),
        }
    }

    fn shell(cmdline: &str) -> Command {
        if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", cmdline]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", cmdline]);
            c
        }
    }

    fn exec(&self, cmdline: &str, impairment: &Impairment) -> anyhow::Result<()> {
        let status = Self::shell(cmdline)
            .envs(impairment.env_vars())
            .status()
            .map_err(|e| anyhow::anyhow!("falha ao executar `{cmdline}`: {e}"))?;
        anyhow::ensure!(status.success(), "`{cmdline}` terminou com {status}");
        Ok(())
    }
}

impl ImpairmentHook for CommandHook {
    fn apply(&self, impairment: &Impairment) -> anyhow::Result<()> {
        *self.impairment.lock().unwrap() = *impairment;
        self.exec(&self.apply_cmd, impairment)
    }

    fn clear(&self) -> anyhow::Result<()> {
        match &self.clear_cmd {
            Some(cmd) => self.exec(cmd, &self.impairment.lock().unwrap()),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMP: Impairment = Impairment {
        loss_pct: 2.5,
        latency_ms: 40,
        jitter_ms: 10,
    };

    #[test]
    fn suggested_commands() {
        assert_eq!(
            IMP.netem_command("eth0"),
            "sudo tc qdisc replace dev eth0 root netem delay 40ms 10ms loss 2.5%"
        );
        let c = IMP.clumsy_command(50000, 50100);
        assert!(c.contains("--lag on --lag-time 40"));
        assert!(c.contains("--drop on --drop-chance 2.5"));
        assert!(c.contains("udp.DstPort <= 50100"));
        assert!(Impairment::default().is_none());
        assert!(!IMP.is_none());
    }

    #[test]
    fn command_hook_passes_values_as_env() {
        let check = if cfg!(windows) {
            "if \"%LT_LOSS_PCT%|%LT_LATENCY_MS%|%LT_JITTER_MS%\"==\"2.5|40|10\" (exit 0) else (exit 1)"
        } else {
            "test \"$LT_LOSS_PCT|$LT_LATENCY_MS|$LT_JITTER_MS\" = \"2.5|40|10\""
        };
        let hook = CommandHook::new(check.into(), Some(check.into()));
        hook.apply(&IMP).unwrap();
        hook.clear().unwrap();
    }

    #[test]
    fn command_hook_reports_failure() {
        let hook = CommandHook::new("exit 3".into(), None);
        assert!(hook.apply(&IMP).is_err());
        assert!(hook.clear().is_ok());
    }
}
