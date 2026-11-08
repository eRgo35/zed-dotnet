use std::fs;

use zed_extension_api::{
    self as zed, settings::LspSettings, DownloadedFileType, GithubReleaseOptions, LanguageServerId,
    LanguageServerInstallationStatus, Result,
};

const SERVER_NAME: &str = "wpf-xaml-ls";
const RELEASES_REPO: &str = "eRgo35/wpf-xaml-language-server";
/// Each release is unpacked into `wpf-xaml-ls-<version>/` in the extension's work directory.
const DIR_PREFIX: &str = "wpf-xaml-ls-";
const SERVER_DLL: &str = "wpf-xaml-ls.dll";

struct WpfXamlExtension {
    /// The downloaded server found on the first start, reused for the rest of the session.
    cached_dll: Option<String>,
}

impl WpfXamlExtension {
    /// Path (relative to the work directory) of the newest released server, downloading it if needed.
    fn server_dll(&mut self, id: &LanguageServerId) -> Result<String> {
        if let Some(dll) = &self.cached_dll {
            if fs::metadata(dll).is_ok() {
                return Ok(dll.clone());
            }
        }

        zed::set_language_server_installation_status(id, &LanguageServerInstallationStatus::CheckingForUpdate);
        let dir = match download_latest(id) {
            Ok(dir) => dir,
            // Offline or rate-limited: keep using what was downloaded before.
            Err(error) => newest_installed().ok_or(error)?,
        };
        zed::set_language_server_installation_status(id, &LanguageServerInstallationStatus::None);

        let dll = format!("{dir}/{SERVER_DLL}");
        self.cached_dll = Some(dll.clone());
        Ok(dll)
    }
}

/// Makes sure the latest release is unpacked and removes older ones. Returns its directory.
fn download_latest(id: &LanguageServerId) -> Result<String> {
    let release = zed::latest_github_release(
        RELEASES_REPO,
        GithubReleaseOptions { require_assets: true, pre_release: false },
    )?;
    let version = release.version.trim_start_matches('v');
    let asset_name = format!("{DIR_PREFIX}{version}.zip");
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| format!("release {} of {RELEASES_REPO} has no {asset_name}", release.version))?;

    let dir = format!("{DIR_PREFIX}{version}");
    if fs::metadata(format!("{dir}/{SERVER_DLL}")).is_err() {
        zed::set_language_server_installation_status(id, &LanguageServerInstallationStatus::Downloading);
        zed::download_file(&asset.download_url, &dir, DownloadedFileType::Zip)
            .map_err(|error| format!("failed to download {asset_name}: {error}"))?;

        for old in installed_dirs().into_iter().filter(|name| *name != dir) {
            fs::remove_dir_all(&old).ok();
        }
    }

    Ok(dir)
}

/// The newest unpacked release, if any.
fn newest_installed() -> Option<String> {
    installed_dirs()
        .into_iter()
        .filter(|dir| fs::metadata(format!("{dir}/{SERVER_DLL}")).is_ok())
        .max_by_key(|dir| version_key(&dir[DIR_PREFIX.len()..]))
}

fn installed_dirs() -> Vec<String> {
    fs::read_dir(".")
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| name.starts_with(DIR_PREFIX))
                .collect()
        })
        .unwrap_or_default()
}

/// `0.10.1` sorts after `0.9.0`.
fn version_key(version: &str) -> Vec<u64> {
    version.split(['.', '-', '+']).map(|part| part.parse().unwrap_or(0)).collect()
}

impl zed::Extension for WpfXamlExtension {
    fn new() -> Self {
        Self { cached_dll: None }
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary = LspSettings::for_worktree(SERVER_NAME, worktree)
            .ok()
            .and_then(|settings| settings.binary);
        let args = binary
            .as_ref()
            .and_then(|binary| binary.arguments.clone())
            .unwrap_or_else(|| vec!["--stdio".to_string()]);

        // The shell environment carries PATH, DOTNET_ROOT and DOTNET_CLI_HOME, which MSBuild and the
        // targeting pack lookup need.
        let mut env = worktree.shell_env();
        if let Some(extra) = binary.as_ref().and_then(|binary| binary.env.clone()) {
            env.extend(extra);
        }

        // 1. An explicit path in settings, 2. a `dotnet tool install` on PATH, 3. the released zip.
        if let Some(path) = binary.and_then(|binary| binary.path).or_else(|| worktree.which(SERVER_NAME)) {
            return Ok(zed::Command { command: path, args, env });
        }

        let dotnet = worktree
            .which("dotnet")
            .ok_or("wpf-xaml-ls needs the .NET SDK: `dotnet` was not found on PATH")?;
        let dll = match self.server_dll(id) {
            Ok(dll) => dll,
            Err(error) => {
                zed::set_language_server_installation_status(id, &LanguageServerInstallationStatus::Failed(error.clone()));
                return Err(error);
            }
        };
        let dll = std::env::current_dir()
            .map_err(|error| format!("no extension work directory: {error}"))?
            .join(dll)
            .to_string_lossy()
            .into_owned();

        Ok(zed::Command { command: dotnet, args: std::iter::once(dll).chain(args).collect(), env })
    }

    fn language_server_initialization_options(
        &mut self,
        _id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(LspSettings::for_worktree(SERVER_NAME, worktree)
            .ok()
            .and_then(|settings| settings.initialization_options))
    }
}

zed::register_extension!(WpfXamlExtension);
