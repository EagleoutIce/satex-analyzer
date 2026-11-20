//! siunitx (<https://ctan.org/pkg/siunitx>): its `S` column aligns numbers on
//! their decimal separator and rounds them when told to, and `\qtyrange`
//! sets a range of quantities.

use super::{Draft, Module, Subject};
use crate::analysis::machine::{Analysis, loaded, macro_text};

pub const MODULE: Module = Module { package: "siunitx", review };

fn review(analysis: &Analysis, draft: &mut Draft) {
    match draft.subject {
        Subject::Precision { kind, column, low, high } => {
            let Some(options) = kind.strip_prefix('S').filter(|o| o.is_empty() || o.starts_with('[')) else {
                draft.fix = Some("round them by hand, or let siunitx `S` round with `round-mode = places`".into());
                return;
            };
            // `round-mode` lives in this variable (siunitx.dtx, number options), a str
            // since 2026, a tl before: `S` prints numbers as written while it is
            // `none` and the column sets none.
            let mode = ["l__siunitx_number_round_mode_str", "l__siunitx_number_round_mode_tl"]
                .into_iter()
                .find_map(|name| macro_text(analysis, name));
            let none = mode.as_deref() == Some("none");
            draft.keep = none && !options.contains("round-mode");
            draft.message =
                format!("`S` column {column} prints its numbers as written, with {low} to {high} decimal places");
            draft.fix = Some("let siunitx round them: `\\sisetup{round-mode = places, round-precision = 2}`, or `S[round-mode = places]` for this column".into());
        }
        Subject::Alignment { kind, column, header, decimals } => {
            let how = match (decimals, kind) {
                (true, "r") => "right-aligned, not on the decimal separator: use siunitx `S`".to_string(),
                (true, _) => {
                    format!("set `{kind}`: align its decimal separators with siunitx `S`, or at least use `r`")
                }
                (false, _) => return draft.message.push_str(", or use siunitx `S`"),
            };
            draft.message = format!("numeric column {column} is {how}");
            let fix = format!(
                "add `\\usepackage{{siunitx}}`, set column {column} as `S` and brace its header: `{{{header}}}`"
            );
            draft.fix = Some(fix);
        }
        Subject::QuantityRange => draft.message.push_str(match loaded(analysis, "siunitx") {
            true => ", or write the whole range with siunitx's `\\qtyrange`",
            false => ", and siunitx sets a range of quantities with `\\qtyrange`",
        }),
    }
}
