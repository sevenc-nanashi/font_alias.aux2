use crate::settings::{Alias, FamilyKey, Source};
use anyhow::{Context, Result, ensure};
use std::{collections::HashSet, fs, path::Path};
use windows::{
    Win32::Graphics::DirectWrite::*,
    core::{HSTRING, Interface, PCWSTR},
};

pub struct Family {
    pub key: FamilyKey,
    pub display_name: String,
    pub names: Vec<String>,
    font_family: IDWriteFontFamily1,
}

impl Family {
    pub fn label(&self) -> String {
        format!("{} ({})", self.display_name, self.key.source.label())
    }
}

pub struct Catalog {
    factory: IDWriteFactory5,
    pub families: Vec<Family>,
    pub existing_names: HashSet<String>,
}

impl Catalog {
    pub fn load(font_directory: &Path, warnings: &mut Vec<String>) -> Result<Self> {
        // DirectWrite supplies thread-safe COM interfaces; the shared factory is required by AviUtl2.
        let factory: IDWriteFactory5 = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
        let mut catalog = Self {
            factory,
            families: Vec::new(),
            existing_names: HashSet::new(),
        };
        unsafe {
            let factory3: IDWriteFactory3 = catalog.factory.cast()?;
            let mut system = None;
            factory3.GetSystemFontCollection(false, &mut system, false)?;
            catalog.add_collection(
                &system.expect("DirectWrite succeeded without a collection"),
                Source::System,
            )?;

            let builder = catalog.factory.CreateFontSetBuilder()?;
            collect_files(font_directory, true, &builder, &catalog.factory, warnings);
            let collection = factory3.CreateFontCollectionFromFontSet(&builder.CreateFontSet()?)?;
            catalog.add_collection(&collection, Source::FontDirectory)?;
        }
        catalog
            .families
            .sort_by_key(|family| family.label().to_lowercase());
        Ok(catalog)
    }

    fn add_collection(
        &mut self,
        collection: &IDWriteFontCollection1,
        source: Source,
    ) -> Result<()> {
        unsafe {
            for index in 0..collection.GetFontFamilyCount() {
                let family = collection.GetFontFamily(index)?;
                let names = localized_strings(&family.GetFamilyNames()?)?;
                ensure!(!names.is_empty(), "フォントのファミリー名がありません");
                let canonical = names
                    .iter()
                    .position(|(locale, _)| locale.eq_ignore_ascii_case("en-us"))
                    .unwrap_or(0);
                let display = names
                    .iter()
                    .position(|(locale, _)| locale.eq_ignore_ascii_case("ja-jp"))
                    .unwrap_or(canonical);
                self.existing_names
                    .extend(names.iter().map(|(_, name)| name.to_lowercase()));
                self.families.push(Family {
                    key: FamilyKey {
                        source,
                        family: names[canonical].1.clone(),
                    },
                    display_name: names[display].1.clone(),
                    names: names.into_iter().map(|(_, name)| name).collect(),
                    font_family: family,
                });
            }
        }
        Ok(())
    }

    pub fn find(&self, key: &FamilyKey) -> Option<&Family> {
        self.families.iter().find(|family| family.key == *key)
    }

    pub fn build_aliases(
        &self,
        aliases: &[Alias],
        warnings: &mut Vec<String>,
    ) -> Result<Option<IDWriteFontCollection>> {
        let factory3: IDWriteFactory3 = self.factory.cast()?;
        unsafe {
            let builder = factory3.CreateFontSetBuilder()?;
            let mut count = 0;
            for alias in aliases {
                let Some(family) = self.find(&alias.target) else {
                    warnings.push(format!(
                        "未登録: {} → {} ({}) が見つかりません",
                        alias.name,
                        alias.target.family,
                        alias.target.source.label()
                    ));
                    continue;
                };
                // Build a separate set first: an error must not register half of a family.
                let set = match self.alias_set(family, &alias.name) {
                    Ok(set) => set,
                    Err(error) => {
                        warnings.push(format!("未登録: {}: {error:#}", alias.name));
                        continue;
                    }
                };
                builder.AddFontSet(&set)?;
                count += 1;
            }
            if count == 0 {
                return Ok(None);
            }
            Ok(Some(crate::alias_collection::wrap(
                factory3
                    .CreateFontCollectionFromFontSet(&builder.CreateFontSet()?)?
                    .cast()?,
            )))
        }
    }

    fn alias_set(&self, family: &Family, alias: &str) -> Result<IDWriteFontSet> {
        unsafe {
            let factory3: IDWriteFactory3 = self.factory.cast()?;
            let builder = factory3.CreateFontSetBuilder()?;
            for index in 0..family.font_family.GetFontCount() {
                let font = family.font_family.GetFont(index)?;
                // Let DirectWrite synthesize styles for the new collection. Explicitly adding
                // synthesized faces preserves enumeration but breaks GetFirstMatchingFont.
                if font.GetSimulations() != DWRITE_FONT_SIMULATIONS_NONE {
                    continue;
                }
                // Supplying any custom properties disables automatic metadata extraction.
                // Carry the source font's matching attributes into the alias set.
                let mut values = Vec::new();
                for id in [
                    DWRITE_FONT_PROPERTY_ID_FAMILY_NAME,
                    DWRITE_FONT_PROPERTY_ID_TYPOGRAPHIC_FAMILY_NAME,
                    DWRITE_FONT_PROPERTY_ID_WIN32_FAMILY_NAME,
                ] {
                    values.push((id, HSTRING::from("en-us"), HSTRING::from(alias)));
                }
                for (id, value) in [
                    (DWRITE_FONT_PROPERTY_ID_WEIGHT, font.GetWeight().0),
                    (DWRITE_FONT_PROPERTY_ID_STRETCH, font.GetStretch().0),
                    (DWRITE_FONT_PROPERTY_ID_STYLE, font.GetStyle().0),
                ] {
                    values.push((id, HSTRING::new(), HSTRING::from(value.to_string())));
                }
                for (locale, name) in localized_strings(&font.GetFaceNames()?)? {
                    for id in [
                        DWRITE_FONT_PROPERTY_ID_FACE_NAME,
                        DWRITE_FONT_PROPERTY_ID_TYPOGRAPHIC_FACE_NAME,
                    ] {
                        values.push((id, HSTRING::from(&locale), HSTRING::from(&name)));
                    }
                    values.push((
                        DWRITE_FONT_PROPERTY_ID_FULL_NAME,
                        HSTRING::from(locale),
                        HSTRING::from(format!("{alias} {name}")),
                    ));
                }
                let properties: Vec<_> = values
                    .iter()
                    .map(|(id, locale, value)| DWRITE_FONT_PROPERTY {
                        propertyId: *id,
                        propertyValue: PCWSTR(value.as_ptr()),
                        localeName: PCWSTR(locale.as_ptr()),
                    })
                    .collect();
                builder.AddFontFaceReference(&font.GetFontFaceReference()?, &properties)?;
            }
            Ok(builder.CreateFontSet()?)
        }
    }
}

fn localized_strings(strings: &IDWriteLocalizedStrings) -> Result<Vec<(String, String)>> {
    unsafe {
        (0..strings.GetCount())
            .map(|index| {
                let mut locale = vec![0; strings.GetLocaleNameLength(index)? as usize + 1];
                let mut value = vec![0; strings.GetStringLength(index)? as usize + 1];
                strings.GetLocaleName(index, &mut locale)?;
                strings.GetString(index, &mut value)?;
                Ok((
                    String::from_utf16(&locale[..locale.len() - 1])?,
                    String::from_utf16(&value[..value.len() - 1])?,
                ))
            })
            .collect()
    }
}

fn collect_files(
    directory: &Path,
    descend: bool,
    builder: &IDWriteFontSetBuilder1,
    factory: &IDWriteFactory5,
    warnings: &mut Vec<String>,
) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            warnings.push(format!("{}: {error}", directory.display()));
            return;
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => paths.push(entry.path()),
            Err(error) => warnings.push(format!("{}: {error}", directory.display())),
        }
    }
    paths.sort();
    for path in paths {
        if path.is_dir() {
            if descend {
                collect_files(&path, false, builder, factory, warnings);
            }
            continue;
        }
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            continue;
        };
        if !["ttf", "otf", "ttc", "otc"]
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
        {
            continue;
        }
        let result = (|| -> Result<()> {
            unsafe {
                let file =
                    factory.CreateFontFileReference(&HSTRING::from(path.as_os_str()), None)?;
                builder
                    .AddFontFile(&file)
                    .context("フォントを読み込めません")?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            warnings.push(format!("{}: {error:#}", path.display()));
        }
    }
}
