use std::path::Path;

use zed_extension_api::{
    self as zed, settings::LspSettings, Command, EnvVars, LanguageServerId, Result, Worktree,
};

struct PactoleExtension;

impl zed::Extension for PactoleExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;

        if let Some(binary) = settings.binary {
            if let Some(path) = binary.path {
                return Ok(Command {
                    command: path,
                    args: binary.arguments.unwrap_or_default(),
                    env: binary.env.unwrap_or_default().into_iter().collect(),
                });
            }
        }

        let root = worktree.root_path();
        let development_binary = Path::new(&root)
            .join("target")
            .join("debug")
            .join("pactole-lsp");

        if development_binary.is_file() {
            return Ok(Command {
                command: development_binary.to_string_lossy().into_owned(),
                args: vec![],
                env: EnvVars::default(),
            });
        }

        let installed_binary = worktree.which("pactole-lsp").ok_or_else(|| {
            "pactole-lsp not found. Build it with `cargo build -p pactole-lsp` \
                 or configure lsp.pactole-lsp.binary.path."
                .to_string()
        })?;

        Ok(Command {
            command: installed_binary,
            args: vec![],
            env: EnvVars::default(),
        })
    }
}

zed::register_extension!(PactoleExtension);
