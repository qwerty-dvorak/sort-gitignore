use zed_extension_api::{self as zed, LanguageServerId, Result};

struct SortGitIgnoreExtension {
    cached_binary_path: Option<String>,
}

fn find_executable_on_path(binary_name: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(binary_name);
        if std::fs::metadata(&candidate).is_ok_and(|m| m.is_file()) {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

impl SortGitIgnoreExtension {
    fn language_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<String> {
        if let Some(path) = &self.cached_binary_path {
            if std::fs::metadata(path).is_ok_and(|m| m.is_file()) {
                return Ok(path.clone());
            }
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = zed::latest_github_release(
            "qwerty-dvorak/sort-gitignore",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        );

        let (platform, arch) = zed::current_platform();
        let binary_name = match platform {
            zed::Os::Windows => "git_ignore-lsp.exe",
            _ => "git_ignore-lsp",
        };

        let release = match release {
            Ok(release) => release,
            Err(release_err) => {
                if let Some(path) = find_executable_on_path(binary_name) {
                    self.cached_binary_path = Some(path.clone());
                    zed::set_language_server_installation_status(
                        language_server_id,
                        &zed::LanguageServerInstallationStatus::None,
                    );
                    return Ok(path);
                }

                return Err(format!(
                    "failed to fetch GitHub release for qwerty-dvorak/sort-gitignore ({release_err}); \
                     also could not find `{binary_name}` on PATH"
                ));
            }
        };

        let asset_name = format!(
            "git_ignore-lsp-{arch}-{os}.tar.gz",
            arch = match arch {
                zed::Architecture::Aarch64 => "aarch64",
                zed::Architecture::X8664 => "x86_64",
                zed::Architecture::X86 => "x86",
            },
            os = match platform {
                zed::Os::Mac => "apple-darwin",
                zed::Os::Linux => "unknown-linux-gnu",
                zed::Os::Windows => "pc-windows-msvc",
            },
        );

        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {asset_name}"))?;

        let version_dir = format!("git_ignore-lsp-{}", release.version);
        let binary_path = format!("{version_dir}/{binary_name}");

        if !std::fs::metadata(&binary_path).is_ok_and(|m| m.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            zed::download_file(
                &asset.download_url,
                &version_dir,
                zed::DownloadedFileType::GzipTar,
            )
            .map_err(|e| format!("failed to download git_ignore-lsp: {e}"))?;

            let entries = std::fs::read_dir(".")
                .map_err(|e| format!("failed to list extension directory: {e}"))?;

            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("git_ignore-lsp-") && name != version_dir {
                    std::fs::remove_dir_all(entry.path()).ok();
                }
            }

            zed::make_file_executable(&binary_path)?;
        }

        self.cached_binary_path = Some(binary_path.clone());

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::None,
        );

        Ok(binary_path)
    }
}

impl zed::Extension for SortGitIgnoreExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary_path = self.language_server_binary_path(language_server_id, worktree)?;
        Ok(zed::Command {
            command: binary_path,
            args: vec![],
            env: vec![],
        })
    }
}

zed::register_extension!(SortGitIgnoreExtension);
