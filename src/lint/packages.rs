//! A package the document loads that this machine does not have, but the
//! distribution could install.  `unresolved-file` reports the file the
//! search did not find and cannot say why. This rule separates the typo
//! from the package that is only missing here, and names the package that
//! ships the file, which is often not the one the document loads:
//! `subcaption.sty` is in `caption`.
//!
//! The answer comes from the distribution's own manager
//! ([`crate::plugin::packages::Manager`]), never from a table here, and
//! only while `plugins.packages` is on.

use super::{Category, Ref, Report, Rule, Severity, Source};
use crate::plugin::packages::{Manager, Provided, shell, tree_writable, unresolved};

pub static RULES: &[Rule] = &[Rule {
    code: "uninstalled-package",
    category: Category::Correctness,
    severity: Severity::Warning,
    summary: "the package is not installed, and the distribution could install it",
    explanation: "\
The file was not found in any tree the run searched, and the distribution's
package manager names a package that ships it.  The load is therefore no
typo: the package is only missing from this machine.  A LaTeX package and
the package that carries it often differ in name, so the finding names both.

Fix by running the command the finding gives, or `satex install`, which never
runs as part of `lint --fix`.  The question is off by default because it reads
the package repository: turn on `plugins.packages` in `satex.yaml`.",
    references: &[Ref { source: Source::TexLive, at: "tlmgr" }, Ref { source: Source::MikTeX, at: "miktex packages" }],
    fixes: &[],
    run: uninstalled_packages,
}];

fn uninstalled_packages(report: &mut Report) {
    let analysis = report.analysis();
    let Ok(manager) = Manager::enabled(&analysis.settings, analysis.plugins.provider) else { return };
    if analysis.distribution.indexed == 0 {
        return;
    }
    let usermode = tree_writable(&analysis.distribution.roots) == Some(false);
    for missing in unresolved(analysis) {
        let Provided::By(package) = manager.provides(&missing.file) else { continue };
        let command = shell(&manager.install_command(std::slice::from_ref(&package), usermode));
        let message = format!("`{}` is not installed here, and `{package}` ships {}", missing.name, missing.file);
        report.add(missing.span, &missing.name, message, Some(format!("run `{command}`, or `satex install`")));
    }
}
