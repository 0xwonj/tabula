//! `tabula query`

use crate::app::AppContext;
use crate::cli::QueryArgs;
use crate::io::{encode_json_args, load_context, load_program, load_state};
use crate::output::{query_run_output, render_query};

/// Execute one read-only query against the supplied state snapshot.
pub(crate) fn run(ctx: &AppContext, args: &QueryArgs) -> anyhow::Result<()> {
    let loaded = load_program(ctx.sdk()?, &args.program)?;
    let state = load_state(&args.state)?;
    let context = load_context(args.context.as_deref())?;
    let query = loaded.program.query(&args.query)?;
    let params = encode_json_args(
        &args.args,
        &query
            .params()
            .iter()
            .map(tabula_sdk::ParameterHandle::ty)
            .collect::<Vec<_>>(),
    )?;
    let result = loaded
        .program
        .runner()
        .query_symbol(&state, &args.query, params, &context)?;
    let output = query_run_output(&loaded.artifact, &args.query, &result);
    if ctx.wants_json(args.json) {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", render_query(&output));
    }
    Ok(())
}
