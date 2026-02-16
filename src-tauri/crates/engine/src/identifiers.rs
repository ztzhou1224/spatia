use crate::EngineResult;

pub fn validate_table_name(table_name: &str) -> EngineResult<()> {
    let mut chars = table_name.chars();
    let Some(first) = chars.next() else {
        return Err("table name is empty".into());
    };
    if !is_ident_start(first) || !chars.all(is_ident_continue) {
        return Err(
            "table name must be alphanumeric or underscore and start with a letter or underscore"
                .into(),
        );
    }
    Ok(())
}

fn is_ident_start(value: char) -> bool {
    value == '_' || value.is_ascii_alphabetic()
}

fn is_ident_continue(value: char) -> bool {
    is_ident_start(value) || value.is_ascii_digit()
}
