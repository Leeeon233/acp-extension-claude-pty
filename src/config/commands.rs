use std::path::{Path, PathBuf};

use agent_client_protocol::schema::{
    AvailableCommand, AvailableCommandInput, UnstructuredCommandInput,
};

const UNSUPPORTED_COMMANDS: &[&str] = &[
    "cost",
    "keybindings-help",
    "login",
    "logout",
    "output-style:new",
    "release-notes",
    "todos",
];

pub fn available_commands(cwd: &Path) -> Vec<AvailableCommand> {
    let mut commands = builtin_commands();
    commands.extend(discover_project_commands(cwd));
    commands.extend(discover_project_skills(cwd));
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    commands.dedup_by(|a, b| a.name == b.name);
    commands
        .into_iter()
        .filter(|command| !UNSUPPORTED_COMMANDS.contains(&command.name.as_str()))
        .collect()
}

fn builtin_commands() -> Vec<AvailableCommand> {
    vec![
        AvailableCommand::new("init", "Create or update project instructions"),
        AvailableCommand::new(
            "compact",
            "Free up context by summarizing the conversation so far",
        )
        .input(AvailableCommandInput::Unstructured(
            UnstructuredCommandInput::new("<optional custom summarization instructions>"),
        )),
        AvailableCommand::new("clear", "Clear the current Claude conversation"),
        AvailableCommand::new("resume", "Resume a Claude session").input(
            AvailableCommandInput::Unstructured(UnstructuredCommandInput::new(
                "Optional session selector or instructions",
            )),
        ),
    ]
}

fn discover_project_commands(cwd: &Path) -> Vec<AvailableCommand> {
    let root = cwd.join(".claude/commands");
    markdown_files(&root)
        .into_iter()
        .filter_map(|path| {
            let name = command_name(&root, &path)?;
            let metadata = command_metadata(&path).ok()?;
            Some(command_from_metadata(name, metadata))
        })
        .collect()
}

fn discover_project_skills(cwd: &Path) -> Vec<AvailableCommand> {
    let root = cwd.join(".claude/skills");
    let mut files = markdown_files(&root);
    files.extend(skill_entrypoints(&root));
    files.sort();
    files.dedup();
    files
        .into_iter()
        .filter_map(|path| {
            let name = skill_name(&root, &path)?;
            let metadata = command_metadata(&path).ok()?;
            Some(command_from_metadata(name, metadata))
        })
        .collect()
}

fn markdown_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(entry_path);
            } else if file_type.is_file()
                && entry_path.extension().and_then(|ext| ext.to_str()) == Some("md")
            {
                files.push(entry_path);
            }
        }
    }
    files
}

fn skill_entrypoints(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path().join("SKILL.md");
        if path.is_file() {
            files.push(path);
        }
    }
    files
}

fn command_name(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut parts = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let file = parts.pop()?;
    parts.push(file.strip_suffix(".md").unwrap_or(&file).to_string());
    Some(parts.join(":"))
}

fn skill_name(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let parts = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [file] => Some(file.strip_suffix(".md").unwrap_or(file).to_string()),
        [dir, file] if file == "SKILL.md" => Some(dir.clone()),
        _ => command_name(root, path),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CommandMetadata {
    description: String,
    argument_hint: Option<String>,
}

fn command_metadata(path: &Path) -> anyhow::Result<CommandMetadata> {
    let text = std::fs::read_to_string(path)?;
    let frontmatter = frontmatter(&text);
    let description = frontmatter
        .as_ref()
        .and_then(|metadata| metadata_value(metadata, "description"))
        .or_else(|| first_heading_or_line(&text))
        .unwrap_or_default();
    let argument_hint = frontmatter
        .as_ref()
        .and_then(|metadata| metadata_value(metadata, "argument-hint"))
        .or_else(|| {
            frontmatter
                .as_ref()
                .and_then(|metadata| metadata_value(metadata, "argument_hint"))
        });

    Ok(CommandMetadata {
        description,
        argument_hint,
    })
}

fn command_from_metadata(name: String, metadata: CommandMetadata) -> AvailableCommand {
    let mut command = AvailableCommand::new(name, metadata.description);
    if let Some(hint) = metadata
        .argument_hint
        .filter(|hint| !hint.trim().is_empty())
    {
        command = command.input(AvailableCommandInput::Unstructured(
            UnstructuredCommandInput::new(hint),
        ));
    }
    command
}

fn frontmatter(text: &str) -> Option<String> {
    let mut lines = text.lines();
    if lines.next()? != "---" {
        return None;
    }
    let mut metadata = Vec::new();
    for line in lines {
        if line == "---" {
            return Some(metadata.join("\n"));
        }
        metadata.push(line);
    }
    None
}

fn metadata_value(metadata: &str, key: &str) -> Option<String> {
    for line in metadata.lines() {
        if let Some((line_key, value)) = line.split_once(':')
            && line_key.trim() == key
        {
            return Some(trim_yaml_scalar(value));
        }
    }
    None
}

fn trim_yaml_scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn first_heading_or_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "---")
        .find_map(|line| {
            let line = line.strip_prefix('#').map(str::trim).unwrap_or(line);
            (!line.is_empty()).then(|| line.to_string())
        })
}
