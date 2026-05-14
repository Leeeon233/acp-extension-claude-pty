use crate::terminal::recognizers::{PermissionDecision, PermissionDialog};

pub fn prompt_submit(prompt: &str) -> Vec<u8> {
    submit_line(prompt)
}

pub fn slash_command(command: &str) -> Vec<u8> {
    let command = if command.starts_with('/') {
        command.to_string()
    } else {
        format!("/{command}")
    };
    submit_line(&command)
}

pub fn ctrl_c() -> Vec<u8> {
    vec![0x03]
}

pub fn ctrl_d() -> Vec<u8> {
    vec![0x04]
}

pub fn ctrl_j() -> Vec<u8> {
    vec![0x0a]
}

pub fn permission_choice(
    dialog: &PermissionDialog,
    decision: PermissionDecision,
) -> Option<Vec<u8>> {
    let option = dialog
        .options
        .iter()
        .find(|option| option.decision == decision)?;
    option
        .accelerator
        .as_ref()
        .map(|accelerator| submit_line(accelerator))
}

fn submit_line(line: &str) -> Vec<u8> {
    let mut bytes = line.as_bytes().to_vec();
    bytes.push(b'\r');
    bytes
}
