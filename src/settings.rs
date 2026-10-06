use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, io::Write, path::Path};
use windows::{
    Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
    core::HSTRING,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    System,
    FontDirectory,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "システム",
            Self::FontDirectory => "Fontフォルダ",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FamilyKey {
    pub source: Source,
    pub family: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Alias {
    pub name: String,
    pub target: FamilyKey,
}

pub fn load(path: &Path) -> Result<Vec<Alias>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("設定ファイルを読み込めません"),
    };
    serde_json::from_slice(&bytes).context("設定ファイルの形式が正しくありません")
}

pub fn validate(aliases: &[Alias], existing_names: &HashSet<String>) -> Result<()> {
    let mut names = HashSet::new();
    for alias in aliases {
        ensure!(
            !alias.name.trim().is_empty(),
            "エイリアス名を入力してください"
        );
        ensure!(
            !alias.name.chars().any(char::is_control),
            "エイリアス名に制御文字は使えません: {:?}",
            alias.name
        );
        let name = alias.name.to_lowercase();
        ensure!(
            !existing_names.contains(&name),
            "既存のフォント名と重複しています: {}",
            alias.name
        );
        ensure!(
            names.insert(name),
            "エイリアス名が重複しています: {}",
            alias.name
        );
        ensure!(
            !alias.target.family.is_empty(),
            "元のファミリーが指定されていません"
        );
    }
    Ok(())
}

pub fn save(path: &Path, aliases: &[Alias]) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(aliases)?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .context("保存用の一時ファイルを作成できません")?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        // Both paths are in the same directory. Never delete the old settings before replacing them.
        unsafe {
            MoveFileExW(
                &HSTRING::from(temporary.as_os_str()),
                &HSTRING::from(path.as_os_str()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.context("設定を保存できません（元の設定は保持されています）")
}
