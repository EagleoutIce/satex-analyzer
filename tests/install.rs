//! `satex install` and the package manager behind it, without a network:
//! what is asked, what is not, and the commands that would be run.

use satex::config::Config;
use satex::plugin::packages::{Manager, Unasked, tree_writable};
use satex::plugin::{Platform, Provider};

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

#[test]
fn a_lint_run_asks_nothing_until_the_switch_is_on() {
    let cfg = Config::default();
    assert!(!cfg.plugins.packages, "plugins.packages is off by default");
    assert_eq!(Manager::enabled(&cfg, Some(Provider::TexLive)).err(), Some(Unasked::Disabled));
    assert_eq!(Manager::enabled(&cfg, Some(Provider::MikTeX)).err(), Some(Unasked::Disabled));
}

#[test]
fn tectonic_has_nothing_to_install_and_nothing_missing() {
    let cfg =
        Config { plugins: satex::config::PluginSwitches { packages: true, ..Default::default() }, ..Config::default() };
    assert_eq!(Manager::requested(&cfg, Some(Provider::Tectonic)).err(), Some(Unasked::Bundled));
    assert_eq!(Provider::Tectonic.manager(), None);
}

#[test]
fn an_explicit_request_needs_a_manager_on_the_machine() {
    let cfg = Config::default();
    assert_eq!(Manager::requested(&cfg, None).err(), Some(Unasked::NoManager));
}

#[test]
fn the_command_is_the_managers_own() {
    if !installed() || which::which("tlmgr").is_err() {
        return;
    }
    let cfg = Config::default();
    let Ok(manager) = Manager::requested(&cfg, Some(Provider::TexLive)) else { return };
    let packages = vec!["caption".to_string()];
    assert_eq!(manager.install_command(&packages, false), ["tlmgr", "install", "caption"]);
    assert_eq!(manager.install_command(&packages, true), ["tlmgr", "--usermode", "install", "caption"]);
    assert!(manager.elevated(&packages, Platform::Linux).starts_with("sudo "));
    assert!(manager.elevated(&packages, Platform::Windows).contains("administrator"));
}

#[test]
fn a_writable_tree_is_told_from_one_that_is_not() {
    assert_eq!(tree_writable(&[]), None);
    assert_eq!(tree_writable(&[std::env::temp_dir()]), Some(true));
    assert_eq!(tree_writable(&[std::path::PathBuf::from("/proc/1/no-such-tree")]), None);
}
