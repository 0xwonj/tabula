//! Handler for the `inspect` subcommand.

use tabula_sdk::State;

use crate::io::load_json;

pub fn cmd_inspect(state_path: &std::path::Path, table_filter: Option<u32>) -> anyhow::Result<()> {
    let state: State = load_json(state_path)?;

    let cells: Vec<_> = state
        .cells()
        .filter(|(key, _)| table_filter.is_none_or(|table| key.table.0 == table))
        .collect();

    if cells.is_empty() {
        println!("(no cells)");
        return Ok(());
    }

    println!("State: {} cells", cells.len());
    println!();
    for (key, value) in cells {
        println!(
            "  table={} row={} field={} = {:?}",
            key.table.0, key.row.0, key.col.0, value
        );
    }

    Ok(())
}
