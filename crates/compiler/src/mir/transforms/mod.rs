mod canonicalize;
mod inline;

pub(crate) use canonicalize::canonicalize_program;
pub(crate) use inline::inline_functions;
