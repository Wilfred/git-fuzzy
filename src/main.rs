use clap::Parser;
use colored::Colorize;
use std::env;
use std::path::PathBuf;
use std::process::{exit, Command};

/// A git tool that simplifies branch checkout by allowing partial branch name matching
#[derive(Parser)]
#[command(name = "git-fuzzy")]
#[command(version)]
#[command(about = "Fuzzy git branch checkout", long_about = None)]
struct Cli {
    /// Branch name or pattern to match (e.g., 'dev' to match 'develop'). If not provided, lists all local branches alphabetically.
    pattern: Option<String>,
}

#[derive(Debug, Clone)]
struct Branch {
    name: String,
    is_remote: bool,
}

impl Branch {
    fn new(name: String, is_remote: bool) -> Self {
        Branch { name, is_remote }
    }
}

/// Find the git directory by walking up the filesystem
fn find_git_directory() -> Option<PathBuf> {
    let mut current_dir = env::current_dir().ok()?;

    loop {
        let git_path = current_dir.join(".git");
        if git_path.exists() {
            return Some(current_dir);
        }

        if !current_dir.pop() {
            return None;
        }
    }
}

/// Execute a git command and return its output
fn run_git_command(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to execute git: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Get all git remotes
fn get_git_remotes() -> Vec<String> {
    run_git_command(&["remote"])
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect()
}

/// Get the current branch name
fn get_current_branch() -> Option<String> {
    run_git_command(&["branch", "--show-current"])
        .ok()
        .map(|s| s.trim().to_string())
}

/// Get all git refs (branches)
fn get_git_refs(prefix: &str) -> Vec<String> {
    let format_arg = "--format=%(refname:short)";
    run_git_command(&["for-each-ref", format_arg, prefix])
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect()
}

/// Get all branches (local and remote)
fn get_all_branches() -> Vec<Branch> {
    let mut branches = Vec::new();

    // Get local branches
    for branch in get_git_refs("refs/heads/") {
        branches.push(Branch::new(branch, false));
    }

    // Get remote branches
    let remotes = get_git_remotes();
    for remote in remotes {
        let prefix = format!("refs/remotes/{}/", remote);
        for branch in get_git_refs(&prefix) {
            branches.push(Branch::new(branch, true));
        }
    }

    branches
}

/// Get tracking branches (local branches + remote branches without local counterparts)
fn get_tracking_branches() -> Vec<Branch> {
    let all_branches = get_all_branches();
    let mut result = Vec::new();

    // Get all local branch names (without remote prefix)
    let local_branches: Vec<String> = all_branches
        .iter()
        .filter(|b| !b.is_remote)
        .map(|b| b.name.clone())
        .collect();

    for branch in all_branches {
        if !branch.is_remote {
            // Include all local branches
            result.push(branch);
        } else {
            // For remote branches, only include if there's no corresponding local branch
            let remote_name = &branch.name;
            // Remote branches are in format "remote/branch-name"
            if let Some(idx) = remote_name.find('/') {
                let branch_name = &remote_name[idx + 1..];
                if !local_branches.contains(&branch_name.to_string()) {
                    result.push(branch);
                }
            }
        }
    }

    result
}

/// Match branches exactly by name
fn match_branch_exactly(branches: &[Branch], needle: &str) -> Vec<Branch> {
    branches
        .iter()
        .filter(|b| b.name == needle)
        .cloned()
        .collect()
}

/// Match branches by substring
fn match_branch_substring(branches: &[Branch], needle: &str) -> Vec<Branch> {
    branches
        .iter()
        .filter(|b| b.name.contains(needle))
        .cloned()
        .collect()
}

/// Checkout a branch
fn checkout_branch(branch: &Branch) -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.arg("checkout");

    // If it's a remote branch, create a local tracking branch
    if branch.is_remote {
        cmd.arg("--track");
    }

    cmd.arg(&branch.name);

    let status = cmd
        .status()
        .map_err(|e| format!("Failed to execute git checkout: {}", e))?;

    if !status.success() {
        return Err(format!("git checkout failed for branch: {}", branch.name));
    }

    Ok(())
}

/// Checkout a commit
fn checkout_commit(commit: &str) -> Result<(), String> {
    let status = Command::new("git")
        .arg("checkout")
        .arg(commit)
        .status()
        .map_err(|e| format!("Failed to execute git checkout: {}", e))?;

    if !status.success() {
        return Err(format!("git checkout failed for commit: {}", commit));
    }

    Ok(())
}

/// Check if a string could be a commit hash substring (only hex characters)
fn is_possible_commit_hash(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Highlight the matched substring in a branch name
fn highlight_match(branch_name: &str, needle: &str) -> String {
    if let Some(pos) = branch_name.find(needle) {
        let before = &branch_name[..pos];
        let matched = &branch_name[pos..pos + needle.len()];
        let after = &branch_name[pos + needle.len()..];
        format!("{}{}{}", before, matched.green().bold(), after)
    } else {
        branch_name.to_string()
    }
}

fn main() {
    // Parse command line arguments
    let cli = Cli::parse();

    // Check if we're in a git repository
    if find_git_directory().is_none() {
        eprintln!("Error: Not in a git repository");
        exit(1);
    }

    // If no pattern is provided, list all local branches alphabetically
    if cli.pattern.is_none() {
        let mut local_branches = get_git_refs("refs/heads/");
        local_branches.sort();
        let current_branch = get_current_branch();
        for branch in local_branches {
            if Some(&branch) == current_branch.as_ref() {
                println!("{} {}", "*".green().bold(), branch.green().bold());
            } else {
                println!("  {}", branch);
            }
        }
        return;
    }

    let needle = cli.pattern.as_ref().unwrap();

    // Get all tracking branches
    let branches = get_tracking_branches();

    // Try exact match first
    let mut matches = match_branch_exactly(&branches, needle);

    // If no exact match, try substring match
    if matches.is_empty() {
        matches = match_branch_substring(&branches, needle);
    }

    match matches.len() {
        0 => {
            // No branch matches; only try as a commit if the argument could be a commit hash
            if !is_possible_commit_hash(needle) {
                eprintln!("No branches match '{}'.", needle);
                exit(1);
            }
            println!("No branches match '{}', trying as commit...", needle);
            if let Err(e) = checkout_commit(needle) {
                eprintln!("Error: {}", e);
                exit(1);
            }
        }
        1 => {
            // Exactly one match, checkout that branch
            let branch = &matches[0];
            if let Err(e) = checkout_branch(branch) {
                eprintln!("Error: {}", e);
                exit(1);
            }
        }
        _ => {
            // Multiple matches, show them to the user
            eprintln!("Ambiguous branch name '{}'. Multiple matches:", needle);
            let current_branch = get_current_branch();
            for branch in matches {
                let highlighted = highlight_match(&branch.name, needle);
                if Some(&branch.name) == current_branch.as_ref() {
                    eprintln!("{} {}", "*".green().bold(), highlighted);
                } else {
                    eprintln!("  {}", highlighted);
                }
            }
            exit(1);
        }
    }
}
