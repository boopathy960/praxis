//! axsh — the Axiom kernel's built-in command line.
//!
//! Every OS ships a shell; Axiom's is built into the kernel rather than layered
//! over it, so the primitives it exposes are the kernel's own: proof-scheduled
//! dispatch, the tier table, the drift/transition ledger, and the
//! boundary-crossing benchmark. Two escape hatches reach the old world, both
//! honestly priced:
//!
//!   * `sh <cmdline>` runs a raw command as a Tier-2 caged process through the
//!     supervised device layer — the full Linux tax, always.
//!   * `praxis <args>` runs the Praxis CLI (the agentic userland command,
//!     `ASTRA_PRAXIS_BIN` / `ASTRA_CLAW_BIN` or `claw` on PATH; `claw` is kept
//!     as an alias) the same way: the AI agent is just another Tier-2 program
//!     until its claims say otherwise. That is PRAXIS.md §4.3 — agents
//!     confined by tier membership, not by ambient authority.
//!
//! The shell is transport-agnostic: `exec` takes a line and returns text, so
//! the same interpreter serves the HTTP route, the desktop console, or a TTY.

use serde::Serialize;
use serde_json::{Value, json};

use super::{AxiomKernel, RegisterComponent};
use crate::common::AppError;

/// One executed shell line. Command failures are output, not transport errors
/// — a shell that 500s on a typo is not a shell.
#[derive(Debug, Clone, Serialize)]
pub struct AxshOutput {
    pub ok: bool,
    pub command: String,
    pub output: String,
}

/// The interpreter. Cheap to clone; owns nothing but handles.
#[derive(Clone)]
pub struct Axsh {
    kernel: AxiomKernel,
    claw_bin: String,
}

impl Axsh {
    #[must_use]
    pub fn new(kernel: AxiomKernel) -> Self {
        let claw_bin = std::env::var("ASTRA_PRAXIS_BIN")
            .or_else(|_| std::env::var("ASTRA_CLAW_BIN"))
            .unwrap_or_else(|_| "claw".into());
        Self { kernel, claw_bin }
    }

    /// Execute one line. Only transport-level problems (never a failed
    /// command) surface as `Err`.
    pub fn exec(&self, line: &str) -> Result<AxshOutput, AppError> {
        let line = line.trim();
        let (head, rest) = match line.split_once(char::is_whitespace) {
            Some((head, rest)) => (head, rest.trim()),
            None => (line, ""),
        };
        let result = match head {
            "" | "help" => Ok(HELP.trim().to_string()),
            "uname" => self.uname(),
            "ps" => self.ps(),
            "tiers" => self.tiers(),
            "claims" => self.claims(rest),
            "dispatch" => self.dispatch(rest),
            "register" => self.register(rest),
            "resync" => self.resync(),
            "drift" => self.drift(rest),
            "ledger" => self.ledger(rest),
            "bench" => self.bench(rest),
            "sh" => self.caged_shell(rest),
            // `praxis` is the agentic userland CLI; `claw` kept as an alias.
            "praxis" | "claw" => self.claw(rest),
            other => Err(AppError::Validation(format!(
                "unknown command '{other}' — try `help`"
            ))),
        };
        Ok(match result {
            Ok(output) => AxshOutput {
                ok: true,
                command: line.to_string(),
                output,
            },
            Err(error) => AxshOutput {
                ok: false,
                command: line.to_string(),
                output: error.to_string(),
            },
        })
    }

    fn uname(&self) -> Result<String, AppError> {
        pretty(&self.kernel.kernel_info()?)
    }

    fn ps(&self) -> Result<String, AppError> {
        let components = self.kernel.components()?;
        if components.is_empty() {
            return Ok("no components registered".into());
        }
        let mut out = format!(
            "{:<24} {:<9} {:<6} {:>10} {:>12}  FLAGS\n",
            "NAME", "TIER", "RANK", "DISPATCHES", "AVG_NS"
        );
        for c in &components {
            let mut flags = Vec::new();
            if c.kernel_builtin {
                flags.push("tcb");
            }
            if c.quarantined {
                flags.push("QUARANTINED");
            }
            out.push_str(&format!(
                "{:<24} {:<9} {:<6} {:>10} {:>12}  {}\n",
                c.name,
                format!("{:?}", c.tier).to_lowercase(),
                c.tier.rank(),
                c.dispatches,
                c.avg_ns(),
                flags.join(",")
            ));
        }
        Ok(out.trim_end().to_string())
    }

    fn tiers(&self) -> Result<String, AppError> {
        let info = self.kernel.kernel_info()?;
        Ok(format!(
            "tier 0 proven    {:>4}  uncaged: dispatch is a function call\n\
             tier 1 partial   {:>4}  guarded domain: envelope check + audit per call\n\
             tier 2 unproven  {:>4}  process cage: the full Linux tax\n\
             promotion is earned in the proof economy; demotion is sentinel drift",
            info.per_tier[0], info.per_tier[1], info.per_tier[2]
        ))
    }

    fn claims(&self, name: &str) -> Result<String, AppError> {
        if name.is_empty() {
            return Err(AppError::Validation("usage: claims <component>".into()));
        }
        let component = self
            .kernel
            .find(name)?
            .ok_or_else(|| AppError::NotFound(format!("component {name}")))?;
        if component.claims.is_empty() {
            return Ok(format!(
                "{}: no claims declared ({})",
                component.name,
                if component.kernel_builtin {
                    "kernel TCB"
                } else {
                    "unproven"
                }
            ));
        }
        let mut out = String::new();
        for claim in &component.claims {
            out.push_str(&format!(
                "{:?}: claim={} minted={} holding={} refuted={} — {}\n",
                claim.class,
                claim.claim_id,
                claim.minted,
                claim.holding,
                claim.refuted,
                claim.last_detail
            ));
        }
        Ok(out.trim_end().to_string())
    }

    fn dispatch(&self, rest: &str) -> Result<String, AppError> {
        let (name, args_text) = match rest.split_once(char::is_whitespace) {
            Some((name, json_text)) => (name, json_text.trim()),
            None => (rest, ""),
        };
        if name.is_empty() {
            return Err(AppError::Validation(
                "usage: dispatch <component> [json-args]".into(),
            ));
        }
        let args: Value = if args_text.is_empty() {
            json!({})
        } else {
            serde_json::from_str(args_text)
                .map_err(|e| AppError::Validation(format!("bad json args: {e}")))?
        };
        pretty(&self.kernel.dispatch(name, &args)?)
    }

    fn register(&self, rest: &str) -> Result<String, AppError> {
        if rest.is_empty() {
            return Err(AppError::Validation(
                "usage: register <json RegisterComponent>".into(),
            ));
        }
        let request: RegisterComponent = serde_json::from_str(rest)
            .map_err(|e| AppError::Validation(format!("bad component json: {e}")))?;
        pretty(&self.kernel.register(request)?)
    }

    fn resync(&self) -> Result<String, AppError> {
        pretty(&self.kernel.resync()?)
    }

    fn drift(&self, rest: &str) -> Result<String, AppError> {
        let limit = rest.parse::<usize>().unwrap_or(20);
        let transitions = self.kernel.transitions(limit)?;
        if transitions.is_empty() {
            return Ok("no tier transitions recorded".into());
        }
        let mut out = String::new();
        for t in &transitions {
            out.push_str(&format!(
                "{} {}: {:?} -> {:?} ({})\n",
                t.at_ms, t.name, t.from, t.to, t.reason
            ));
        }
        Ok(out.trim_end().to_string())
    }

    fn ledger(&self, rest: &str) -> Result<String, AppError> {
        let limit = rest.parse::<usize>().unwrap_or(10);
        let minted = self.kernel_economy_ledger(limit)?;
        if minted.is_empty() {
            return Ok("the minted ledger is empty".into());
        }
        Ok(minted.join("\n"))
    }

    fn kernel_economy_ledger(&self, limit: usize) -> Result<Vec<String>, AppError> {
        Ok(self
            .kernel
            .economy()
            .ledger()?
            .into_iter()
            .take(limit.clamp(1, 200))
            .map(|claim| format!("{} — {}", claim.claim_id, claim.statement))
            .collect())
    }

    fn bench(&self, rest: &str) -> Result<String, AppError> {
        let iters = rest.parse::<u32>().unwrap_or(1000);
        pretty(&self.kernel.bench(iters)?)
    }

    /// Raw command → Tier-2 caged process. Always the full tax.
    fn caged_shell(&self, cmdline: &str) -> Result<String, AppError> {
        if cmdline.is_empty() {
            return Err(AppError::Validation("usage: sh <command>".into()));
        }
        self.run_caged_line(cmdline)
    }

    /// The Praxis CLI as the agentic userland command — a Tier-2 program
    /// like any other unproven code. (`claw` remains as an alias for the
    /// binary this wraps.)
    fn claw(&self, args: &str) -> Result<String, AppError> {
        let line = if args.is_empty() {
            format!("{} --help", self.claw_bin)
        } else {
            format!("{} {}", self.claw_bin, args)
        };
        self.run_caged_line(&line)
    }

    fn run_caged_line(&self, cmdline: &str) -> Result<String, AppError> {
        let output = self
            .kernel
            .device()
            .execute(
                "shell_exec",
                &json!({ "command": cmdline, "timeout_ms": 120_000 }),
            )
            .map_err(|e| AppError::Internal(format!("caged process failed: {e}")))?;
        let stdout = output.get("stdout").and_then(Value::as_str).unwrap_or("");
        let stderr = output.get("stderr").and_then(Value::as_str).unwrap_or("");
        let exit = output
            .get("exit_code")
            .and_then(Value::as_i64)
            .unwrap_or(-1);
        let mut text = String::new();
        if !stdout.trim().is_empty() {
            text.push_str(stdout.trim_end());
        }
        if !stderr.trim().is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(stderr.trim_end());
        }
        if text.is_empty() {
            text = format!("(no output, exit {exit})");
        } else if exit != 0 {
            text.push_str(&format!("\n(exit {exit})"));
        }
        Ok(text)
    }
}

fn pretty<T: Serialize>(value: &T) -> Result<String, AppError> {
    serde_json::to_string_pretty(value)
        .map_err(|e| AppError::Internal(format!("axsh serialize error: {e}")))
}

const HELP: &str = "
axsh — the Axiom kernel shell (proof-scheduled OS, Phase 0)

kernel
  uname                       kernel identity and tier census
  ps                          registered components, tiers, dispatch stats
  tiers                       the three execution tiers and their cost models
  claims <component>          a component's claims and their live standing
  dispatch <component> [json] proof-scheduled syscall (path depends on tier)
  register <json>             admit a component with its economy claim ids
  resync                      re-read proofs + drift; promote/demote live
  drift [n]                   the tier-transition ledger (newest first)
  ledger [n]                  minted claims in the proof economy
  bench [iters]               measure the cage tax: call vs domain vs process

userland (always Tier 2 — the cage never comes off an escape hatch)
  sh <command>                run a raw command as a supervised caged process
  praxis <args>               run the Praxis CLI (agentic userland command;
                              `claw` is kept as an alias)
";

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::device_agent::{DeviceCapabilities, DevicePolicy};
    use crate::proof_economy::ProofEconomy;
    use crate::sentinel::Sentinel;

    fn shell() -> Axsh {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let economy = ProofEconomy::in_memory(device.clone()).expect("economy");
        let sentinel = Sentinel::in_memory(device.clone()).expect("sentinel");
        let kernel = AxiomKernel::in_memory(economy, sentinel, device).expect("kernel");
        kernel.seed_kernel_builtins().expect("builtins");
        Axsh::new(kernel)
    }

    #[test]
    fn help_ps_and_dispatch_work() {
        let sh = shell();
        assert!(sh.exec("help").unwrap().output.contains("axsh"));
        let ps = sh.exec("ps").unwrap();
        assert!(ps.ok);
        assert!(ps.output.contains("axiom.echo"));
        let dispatched = sh.exec(r#"dispatch axiom.echo {"message":"hi"}"#).unwrap();
        assert!(dispatched.ok, "{}", dispatched.output);
        assert!(dispatched.output.contains("uncaged_call"));
    }

    #[test]
    fn unknown_command_is_output_not_error() {
        let sh = shell();
        let out = sh.exec("blorp").unwrap();
        assert!(!out.ok);
        assert!(out.output.contains("unknown command"));
    }

    #[test]
    fn sh_runs_a_caged_process() {
        let sh = shell();
        let out = sh.exec("sh echo caged-world").unwrap();
        assert!(out.ok, "{}", out.output);
        assert!(out.output.contains("caged-world"));
    }
}
