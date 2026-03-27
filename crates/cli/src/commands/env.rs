//! `tabula env ...`

use crate::app::AppContext;
use crate::cli::{EnvCommand, EnvDoctorArgs};
use crate::output::{environment_status_output, render_env_doctor};

/// Dispatch the `env` namespace.
pub(crate) fn run(ctx: &AppContext, command: &EnvCommand) -> anyhow::Result<()> {
    match command {
        EnvCommand::Doctor(args) => doctor(ctx, args),
    }
}

fn doctor(ctx: &AppContext, args: &EnvDoctorArgs) -> anyhow::Result<()> {
    let output = environment_status_output(ctx.environment_status());
    if ctx.wants_json(args.json) {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", render_env_doctor(&output));
    }
    Ok(())
}
