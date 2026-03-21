//! Handler for the `inspect` subcommand.

use crate::io::{State, load_json};

pub fn cmd_inspect(state_path: &std::path::Path, table_filter: Option<u32>) -> anyhow::Result<()> {
    let state: State = load_json(state_path)?;

    let cells: Vec<_> = state
        .cells
        .iter()
        .filter(|c| table_filter.is_none_or(|t| c.table == t))
        .collect();

    if cells.is_empty() {
        println!("(no cells)");
        return Ok(());
    }

    println!("State: {} cells", cells.len());
    println!();
    for cell in cells {
        println!(
            "  table={} row={} col={} = {:?}",
            cell.table, cell.row, cell.col, cell.value
        );
    }

    Ok(())
}
