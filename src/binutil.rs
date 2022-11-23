use derive_more::*;
use directories::ProjectDirs;
use std::{fmt::Display, path::PathBuf};

#[derive(Debug, Deref, DerefMut, FromStr)]

pub struct HanaDataDir(pub PathBuf);

impl HanaDataDir {
    pub fn chain_data_dir(&self) -> PathBuf {
        self.0.join("chaindata")
    }
}

impl Default for HanaDataDir {
    fn default() -> Self {
        Self(
            ProjectDirs::from("", "", "Hana")
                .map(|pd| pd.data_dir().to_path_buf())
                .unwrap_or_else(|| "data".into()),
        )
    }
}

impl Display for HanaDataDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_os_str().to_str().unwrap())
    }
}
