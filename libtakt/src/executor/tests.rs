#[cfg(test)]
mod tests {
    use crate::models::{Action, Shell};

    #[test]
    fn test_action_serialization() {
        let action = Action::RunCommand {
            command: "echo hello".to_string(),
            args: vec![],
            shell: Shell::Sh,
        };
        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();
        match deserialized {
            Action::RunCommand { command, .. } => assert_eq!(command, "echo hello"),
            _ => panic!("Wrong variant"),
        }
    }
}
