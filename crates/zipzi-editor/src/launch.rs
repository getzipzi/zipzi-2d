//! Parse OS arguments before claiming the existing Runtime endpoint.
use std::{ffi::OsString, path::PathBuf};

#[derive(Default, Debug)]
pub(super) struct LaunchOptions {
    pub no_window: bool,
    pub projects: Vec<PathBuf>,
}
impl LaunchOptions {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut result = Self::default();
        let mut paths_only = false;
        let directory = std::env::current_dir().map_err(|e| e.to_string())?;
        for arg in args {
            if !paths_only && arg == "--" {
                paths_only = true;
            } else if !paths_only && arg == "--no-window" {
                result.no_window = true;
            } else if !paths_only && arg.as_encoded_bytes().starts_with(b"-") {
                // Finder may pass the legacy process serial number.
                if !cfg!(target_os = "macos") || !arg.as_encoded_bytes().starts_with(b"-psn_") {
                    return Err(format!("Unknown launch option: {}", arg.to_string_lossy()));
                }
            } else {
                let path = PathBuf::from(arg);
                result.projects.push(if path.is_absolute() {
                    path
                } else {
                    directory.join(path)
                });
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projects_preserve_unicode_spaces_and_option_like_names() {
        let options = LaunchOptions::parse(
            ["--no-window", "角色 one.zipzi", "--", "-two.ZIPZI"].map(Into::into),
        )
        .unwrap();
        assert!(options.no_window);
        assert_eq!(options.projects.len(), 2);
        assert!(options.projects[0].ends_with("角色 one.zipzi"));
        assert!(options.projects[1].ends_with("-two.ZIPZI"));
        assert!(LaunchOptions::parse(["--unknown".into()]).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn native_arguments_do_not_require_utf8() {
        use std::os::unix::ffi::OsStringExt;
        assert!(
            LaunchOptions::parse([OsString::from_vec(b"a\xff.zipzi".to_vec())])
                .unwrap()
                .projects[0]
                .is_absolute()
        );
    }
}
