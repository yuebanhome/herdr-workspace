use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

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
        Some("--extract-pane-launch-cwd") => {
            println!(
                "{}",
                host::extract_pane_launch_cwd_from_reader(host::stdin())?.display()
            );
            Ok(())
        }
        Some("--extract-workspace-checkout") => {
            println!(
                "{}",
                host::extract_workspace_checkout_from_reader(host::stdin())?.display()
            );
            Ok(())
        }
        Some("--extract-workspace-root") => {
            println!(
                "{}",
                host::extract_workspace_root_from_reader(host::stdin())?.display()
            );
            Ok(())
        }
        Some("--extract-context-pane") => {
            println!(
                "{}",
                host::extract_context_pane_id_from_reader(host::stdin())?
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
        Some("--list-workspaces") => {
            for workspace in host::workspace_ids_from_reader(host::stdin())? {
                println!("{workspace}");
            }
            Ok(())
        }
        Some("--active-tab") => {
            let workspace = required_string(&mut arguments, "--active-tab requires a workspace")?;
            println!(
                "{}",
                host::active_tab_from_workspace_reader(host::stdin(), &workspace)?
            );
            Ok(())
        }
        Some("--candidate-panes") => {
            let workspace =
                required_string(&mut arguments, "--candidate-panes requires a workspace")?;
            for (pane, tab) in host::candidate_panes_from_reader(host::stdin(), &workspace)? {
                println!("{pane}\t{tab}");
            }
            Ok(())
        }
        Some("--select-target-pane") => {
            let workspace =
                required_string(&mut arguments, "--select-target-pane requires a workspace")?;
            let tab = required_string(&mut arguments, "--select-target-pane requires a tab")?;
            let preferred = required_string(
                &mut arguments,
                "--select-target-pane requires a preferred-pane placeholder",
            )?;
            let excluded = arguments
                .map(|argument| {
                    argument
                        .into_string()
                        .map_err(|_| anyhow::anyhow!("pane id is not valid UTF-8"))
                })
                .collect::<Result<Vec<_>>>()?;
            if let Some(pane) = host::select_target_pane_from_reader(
                host::stdin(),
                &workspace,
                &tab,
                (!preferred.is_empty()).then_some(preferred.as_str()),
                &excluded,
            )? {
                println!("{pane}");
            }
            Ok(())
        }
        Some("--verify-process") => {
            if !host::is_reporadar_process_from_reader(host::stdin())? {
                bail!("pane does not run herdr-reporadar");
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

fn required_string(
    arguments: &mut impl Iterator<Item = std::ffi::OsString>,
    message: &'static str,
) -> Result<String> {
    arguments
        .next()
        .context(message)?
        .into_string()
        .map_err(|_| anyhow::anyhow!("argument is not valid UTF-8"))
}
