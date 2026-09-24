use serde_json::json;

use super::{Context, Output};
use crate::query::Record;
use crate::tex::{CatcodeTable, Interner, Mouth, Tok};

/// The token stream the mouth produces under the LaTeX category codes, before
/// anything is expanded: one record per token, so `--format` is honored like
/// everywhere else instead of always printing the compact text form.
pub fn run(context: &Context) -> Result<Output, String> {
    let mut interner = Interner::default();
    let catcodes = CatcodeTable::latex();
    let mut mouth = Mouth::new(context.source, 0);
    let mut out = Vec::new();
    while let Some(token) = mouth.next(&catcodes, &mut interner) {
        let mut record = Record::new();
        record.insert("line".into(), json!(token.span.line));
        record.insert("col".into(), json!(token.span.col));
        match token.tok {
            Tok::Cs(sym) => {
                record.insert("kind".into(), json!("cs"));
                record.insert("name".into(), json!(format!("\\{}", interner.name(sym))));
            }
            Tok::Chr(c, cat) => {
                record.insert("kind".into(), json!("char"));
                record.insert("name".into(), json!(c.to_string()));
                record.insert("detail".into(), json!(format!("cat {}", cat as u8)));
            }
            Tok::Param(n) => {
                record.insert("kind".into(), json!("param"));
                record.insert("name".into(), json!(format!("#{n}")));
            }
        }
        out.push(record);
    }
    Ok(Output::Records(out))
}
