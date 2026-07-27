use std::env;
use std::path::PathBuf;

use anyhow::{Result, bail};

use herdr_reporadar::{app, host};

fn main() {
    if let Err(error) = try_main() {
        eprintln!("herdr-reporadar: {error:#}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<()> {
    let mut arguments = env::args_os().skip(1);
    let first = arguments.next();
    match first.as_deref().and_then(|value| value.to_str()) {
        Some("--extract-pane-cwd") => {
            println!(
                "{}",
                host::extract_pane_cwd_from_reader(host::stdin())?.display()
            );
            Ok(())
        }
        Some("--extract-opened-pane") => {
            println!(
                "{}",
                host::extract_opened_pane_id_from_reader(host::stdin())?
            );
            Ok(())
        }
        Some("--find-pane") => {
            if let Some(pane) = host::find_plugin_pane_from_reader(host::stdin())? {
                println!("{pane}");
            }
            Ok(())
        }
        Some("--root") => {
            let Some(root) = arguments.next() else {
                bail!("--root requires a path");
            };
            app::run(host::resolve_root(Some(PathBuf::from(root)))?)
        }
        Some(argument) => bail!("unknown argument: {argument}"),
        None => app::run(host::resolve_root(None)?),
    }
}
