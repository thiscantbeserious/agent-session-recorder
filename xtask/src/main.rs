//! xtask - Build tasks for AGR
//!
//! Run with: cargo xtask <command>
//!
//! Commands:
//! - gen-docs: Generate documentation (man pages, COMMANDS.md, wiki)

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};

use agr::cli::Cli;

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Build tasks for AGR")]
struct Xtask {
    #[command(subcommand)]
    command: XtaskCommand,
}

#[derive(Subcommand)]
enum XtaskCommand {
    /// Generate documentation from CLI definitions
    #[command(name = "gen-docs")]
    GenDocs {
        /// Output directory (default: docs/)
        #[arg(long, short, default_value = "docs")]
        output: PathBuf,

        /// Generate man pages
        #[arg(long)]
        man: bool,

        /// Generate COMMANDS.md
        #[arg(long)]
        markdown: bool,

        /// Generate wiki pages
        #[arg(long)]
        wiki: bool,

        /// Generate all formats (default if no specific format is specified)
        #[arg(long)]
        all: bool,
    },
}

fn main() -> Result<()> {
    let args = Xtask::parse();

    match args.command {
        XtaskCommand::GenDocs {
            output,
            man,
            markdown,
            wiki,
            all,
        } => {
            // If no specific format is specified, generate all
            let gen_all = all || (!man && !markdown && !wiki);

            if gen_all || man {
                generate_man_pages(&output)?;
            }
            if gen_all || markdown {
                generate_markdown(&output)?;
            }
            if gen_all || wiki {
                generate_wiki(&output)?;
            }
        }
    }

    Ok(())
}

/// Generate man pages using clap_mangen
fn generate_man_pages(output: &Path) -> Result<()> {
    use clap_mangen::Man;

    let man_dir = output.join("man");
    fs::create_dir_all(&man_dir).context("Failed to create man directory")?;

    let cmd = Cli::command();

    // Generate main man page
    let man = Man::new(cmd.clone());
    let mut buffer = Vec::new();
    man.render(&mut buffer)?;
    fs::write(man_dir.join("agr.1"), buffer)?;
    println!("Generated: {}/agr.1", man_dir.display());

    // Generate man pages for subcommands
    for subcommand in cmd.get_subcommands() {
        if subcommand.is_hide_set() {
            continue;
        }

        let name = subcommand.get_name();
        let man = Man::new(subcommand.clone());
        let mut buffer = Vec::new();
        man.render(&mut buffer)?;
        fs::write(man_dir.join(format!("agr-{}.1", name)), buffer)?;
        println!("Generated: {}/agr-{}.1", man_dir.display(), name);

        // Generate man pages for nested subcommands
        for nested in subcommand.get_subcommands() {
            if nested.is_hide_set() {
                continue;
            }
            let nested_name = nested.get_name();
            let man = Man::new(nested.clone());
            let mut buffer = Vec::new();
            man.render(&mut buffer)?;
            fs::write(
                man_dir.join(format!("agr-{}-{}.1", name, nested_name)),
                buffer,
            )?;
            println!(
                "Generated: {}/agr-{}-{}.1",
                man_dir.display(),
                name,
                nested_name
            );
        }
    }

    println!("Man pages generated in {}", man_dir.display());
    Ok(())
}

/// Generate COMMANDS.md markdown documentation
fn generate_markdown(output: &Path) -> Result<()> {
    fs::create_dir_all(output).context("Failed to create output directory")?;

    let cmd = Cli::command();
    let mut markdown = String::new();

    // Header
    markdown.push_str("# AGR Command Reference\n\n");
    markdown.push_str("This document is auto-generated from the CLI definitions.\n\n");
    markdown.push_str("## Table of Contents\n\n");

    // Build TOC
    for subcommand in cmd.get_subcommands() {
        if subcommand.is_hide_set() {
            continue;
        }
        let name = subcommand.get_name();
        markdown.push_str(&format!("- [{}](#agr-{})\n", name, name));
    }
    markdown.push_str("\n---\n\n");

    // Main command
    markdown.push_str("## agr\n\n");
    if let Some(about) = cmd.get_about() {
        markdown.push_str(&format!("{}\n\n", about));
    }
    if let Some(long_about) = cmd.get_long_about() {
        markdown.push_str("```\n");
        markdown.push_str(&format!("{}\n", long_about));
        markdown.push_str("```\n\n");
    }

    // Subcommands
    for subcommand in cmd.get_subcommands() {
        if subcommand.is_hide_set() {
            continue;
        }

        let name = subcommand.get_name();
        markdown.push_str(&format!("## agr {}\n\n", name));

        if let Some(about) = subcommand.get_about() {
            markdown.push_str(&format!("{}\n\n", about));
        }

        // Arguments
        let args: Vec<_> = subcommand.get_arguments().collect();
        if !args.is_empty() {
            let positional: Vec<_> = args.iter().filter(|a| a.is_positional()).collect();
            let options: Vec<_> = args.iter().filter(|a| !a.is_positional()).collect();

            if !positional.is_empty() {
                markdown.push_str("### Arguments\n\n");
                for arg in positional {
                    let arg_name = arg.get_id().as_str();
                    if arg_name == "help" || arg_name == "version" {
                        continue;
                    }
                    markdown.push_str(&format!("- `<{}>`: ", arg_name.to_uppercase()));
                    if let Some(help) = arg.get_help() {
                        markdown.push_str(&format!("{}", help));
                    }
                    markdown.push('\n');
                }
                markdown.push('\n');
            }

            if !options.is_empty() {
                let non_help: Vec<_> = options
                    .iter()
                    .filter(|a| {
                        let id = a.get_id().as_str();
                        id != "help" && id != "version"
                    })
                    .collect();

                if !non_help.is_empty() {
                    markdown.push_str("### Options\n\n");
                    for arg in non_help {
                        let long = arg.get_long().map(|l| format!("--{}", l));
                        let short = arg.get_short().map(|s| format!("-{}", s));
                        let flag = match (long, short) {
                            (Some(l), Some(s)) => format!("{}, {}", s, l),
                            (Some(l), None) => l,
                            (None, Some(s)) => s,
                            _ => continue,
                        };
                        markdown.push_str(&format!("- `{}`: ", flag));
                        if let Some(help) = arg.get_help() {
                            markdown.push_str(&format!("{}", help));
                        }
                        markdown.push('\n');
                    }
                    markdown.push('\n');
                }
            }
        }

        // Long description
        if let Some(long_about) = subcommand.get_long_about() {
            markdown.push_str("### Description\n\n");
            markdown.push_str("```\n");
            markdown.push_str(&format!("{}\n", long_about));
            markdown.push_str("```\n\n");
        }

        // Nested subcommands
        let nested: Vec<_> = subcommand.get_subcommands().collect();
        if !nested.is_empty() {
            markdown.push_str("### Subcommands\n\n");
            for nested_cmd in nested {
                if nested_cmd.is_hide_set() {
                    continue;
                }
                let nested_name = nested_cmd.get_name();
                markdown.push_str(&format!("#### agr {} {}\n\n", name, nested_name));

                if let Some(about) = nested_cmd.get_about() {
                    markdown.push_str(&format!("{}\n\n", about));
                }

                // Nested arguments
                let nested_args: Vec<_> = nested_cmd.get_arguments().collect();
                let nested_positional: Vec<_> =
                    nested_args.iter().filter(|a| a.is_positional()).collect();
                let nested_options: Vec<_> =
                    nested_args.iter().filter(|a| !a.is_positional()).collect();

                if !nested_positional.is_empty() {
                    for arg in nested_positional {
                        let arg_name = arg.get_id().as_str();
                        if arg_name == "help" || arg_name == "version" {
                            continue;
                        }
                        markdown.push_str(&format!("- `<{}>`: ", arg_name.to_uppercase()));
                        if let Some(help) = arg.get_help() {
                            markdown.push_str(&format!("{}", help));
                        }
                        markdown.push('\n');
                    }
                    markdown.push('\n');
                }

                if !nested_options.is_empty() {
                    let non_help: Vec<_> = nested_options
                        .iter()
                        .filter(|a| {
                            let id = a.get_id().as_str();
                            id != "help" && id != "version"
                        })
                        .collect();

                    if !non_help.is_empty() {
                        for arg in non_help {
                            let long = arg.get_long().map(|l| format!("--{}", l));
                            let short = arg.get_short().map(|s| format!("-{}", s));
                            let flag = match (long, short) {
                                (Some(l), Some(s)) => format!("{}, {}", s, l),
                                (Some(l), None) => l,
                                (None, Some(s)) => s,
                                _ => continue,
                            };
                            markdown.push_str(&format!("- `{}`: ", flag));
                            if let Some(help) = arg.get_help() {
                                markdown.push_str(&format!("{}", help));
                            }
                            markdown.push('\n');
                        }
                        markdown.push('\n');
                    }
                }

                if let Some(long_about) = nested_cmd.get_long_about() {
                    markdown.push_str("```\n");
                    markdown.push_str(&format!("{}\n", long_about));
                    markdown.push_str("```\n\n");
                }
            }
        }

        markdown.push_str("---\n\n");
    }

    // Footer
    markdown.push_str("\n*Generated by `cargo xtask gen-docs`*\n");

    let output_path = output.join("COMMANDS.md");
    fs::write(&output_path, markdown)?;
    println!("Generated: {}", output_path.display());

    Ok(())
}

/// Generate GitHub Wiki pages
fn generate_wiki(output: &Path) -> Result<()> {
    let wiki_dir = output.join("wiki");
    fs::create_dir_all(&wiki_dir).context("Failed to create wiki directory")?;

    let cmd = Cli::command();

    // Home page
    let mut home = String::new();
    home.push_str("# AGR Wiki\n\n");
    home.push_str("Welcome to the AGR (Agent Session Recorder) wiki.\n\n");
    home.push_str("## Commands\n\n");

    for subcommand in cmd.get_subcommands() {
        if subcommand.is_hide_set() {
            continue;
        }
        let name = subcommand.get_name();
        if let Some(about) = subcommand.get_about() {
            home.push_str(&format!("- [[{}|Command-{}]] - {}\n", name, name, about));
        } else {
            home.push_str(&format!("- [[{}|Command-{}]]\n", name, name));
        }
    }

    fs::write(wiki_dir.join("Home.md"), home)?;
    println!("Generated: {}/Home.md", wiki_dir.display());

    // Individual command pages
    for subcommand in cmd.get_subcommands() {
        if subcommand.is_hide_set() {
            continue;
        }

        let name = subcommand.get_name();
        let mut page = String::new();

        page.push_str(&format!("# agr {}\n\n", name));

        if let Some(about) = subcommand.get_about() {
            page.push_str(&format!("{}\n\n", about));
        }

        page.push_str("## Usage\n\n");
        page.push_str(&format!("```\nagr {} [OPTIONS]", name));

        // Add positional args to usage
        for arg in subcommand.get_arguments() {
            if arg.is_positional() {
                let arg_name = arg.get_id().as_str();
                if arg_name != "help" && arg_name != "version" {
                    if arg.is_required_set() {
                        page.push_str(&format!(" <{}>", arg_name.to_uppercase()));
                    } else {
                        page.push_str(&format!(" [{}]", arg_name.to_uppercase()));
                    }
                }
            }
        }
        page.push_str("\n```\n\n");

        // Arguments
        let args: Vec<_> = subcommand.get_arguments().collect();
        let positional: Vec<_> = args.iter().filter(|a| a.is_positional()).collect();
        let options: Vec<_> = args.iter().filter(|a| !a.is_positional()).collect();

        if !positional.is_empty() {
            page.push_str("## Arguments\n\n");
            page.push_str("| Argument | Description |\n");
            page.push_str("|----------|-------------|\n");
            for arg in positional {
                let arg_name = arg.get_id().as_str();
                if arg_name == "help" || arg_name == "version" {
                    continue;
                }
                let help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();
                page.push_str(&format!("| `{}` | {} |\n", arg_name.to_uppercase(), help));
            }
            page.push('\n');
        }

        let non_help_options: Vec<_> = options
            .iter()
            .filter(|a| {
                let id = a.get_id().as_str();
                id != "help" && id != "version"
            })
            .collect();

        if !non_help_options.is_empty() {
            page.push_str("## Options\n\n");
            page.push_str("| Option | Description |\n");
            page.push_str("|--------|-------------|\n");
            for arg in non_help_options {
                let long = arg.get_long().map(|l| format!("--{}", l));
                let short = arg.get_short().map(|s| format!("-{}", s));
                let flag = match (long, short) {
                    (Some(l), Some(s)) => format!("{}, {}", s, l),
                    (Some(l), None) => l,
                    (None, Some(s)) => s,
                    _ => continue,
                };
                let help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();
                page.push_str(&format!("| `{}` | {} |\n", flag, help));
            }
            page.push('\n');
        }

        // Description
        if let Some(long_about) = subcommand.get_long_about() {
            page.push_str("## Description\n\n");
            page.push_str(&format!("{}\n\n", long_about));
        }

        // Subcommands
        let nested: Vec<_> = subcommand.get_subcommands().collect();
        if !nested.is_empty() {
            page.push_str("## Subcommands\n\n");
            for nested_cmd in nested {
                if nested_cmd.is_hide_set() {
                    continue;
                }
                let nested_name = nested_cmd.get_name();
                page.push_str(&format!("### {} {}\n\n", name, nested_name));

                if let Some(about) = nested_cmd.get_about() {
                    page.push_str(&format!("{}\n\n", about));
                }

                if let Some(long_about) = nested_cmd.get_long_about() {
                    page.push_str(&format!("{}\n\n", long_about));
                }
            }
        }

        let filename = format!("Command-{}.md", name);
        fs::write(wiki_dir.join(&filename), page)?;
        println!("Generated: {}/{}", wiki_dir.display(), filename);
    }

    println!("Wiki pages generated in {}", wiki_dir.display());
    Ok(())
}
