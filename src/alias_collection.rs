use windows::Win32::Graphics::DirectWrite::*;
use windows::core::{BOOL, Interface, OutRef, PCWSTR, Ref, Result, implement};

pub fn wrap(inner: IDWriteFontCollection) -> IDWriteFontCollection {
    AliasCollection { inner }.into()
}

#[implement(IDWriteFontCollection)]
struct AliasCollection {
    inner: IDWriteFontCollection,
}

impl IDWriteFontCollection_Impl for AliasCollection_Impl {
    fn GetFontFamilyCount(&self) -> u32 {
        unsafe { self.inner.GetFontFamilyCount() }
    }

    fn GetFontFamily(&self, index: u32) -> Result<IDWriteFontFamily> {
        unsafe { wrap_family(self.inner.GetFontFamily(index)?) }
    }

    fn FindFamilyName(&self, name: &PCWSTR, index: *mut u32, exists: *mut BOOL) -> Result<()> {
        unsafe { self.inner.FindFamilyName(*name, index, exists) }
    }

    fn GetFontFromFontFace(&self, face: Ref<IDWriteFontFace>) -> Result<IDWriteFont> {
        unsafe {
            let font = self.inner.GetFontFromFontFace(face.ok()?)?;
            let family = font.GetFontFamily()?;
            Ok(wrap_font(font, family))
        }
    }
}

fn wrap_family(inner: IDWriteFontFamily) -> Result<IDWriteFontFamily> {
    let fonts = inner.cast()?;
    Ok(AliasFamily { inner, fonts }.into())
}

#[implement(IDWriteFontFamily)]
struct AliasFamily {
    inner: IDWriteFontFamily,
    fonts: IDWriteFontList,
}

impl IDWriteFontList_Impl for AliasFamily_Impl {
    fn GetFontCollection(&self) -> Result<IDWriteFontCollection> {
        unsafe { Ok(wrap(self.fonts.GetFontCollection()?)) }
    }

    fn GetFontCount(&self) -> u32 {
        unsafe { self.fonts.GetFontCount() }
    }

    fn GetFont(&self, index: u32) -> Result<IDWriteFont> {
        unsafe { Ok(wrap_font(self.fonts.GetFont(index)?, self.inner.clone())) }
    }
}

impl IDWriteFontFamily_Impl for AliasFamily_Impl {
    fn GetFamilyNames(&self) -> Result<IDWriteLocalizedStrings> {
        unsafe { self.inner.GetFamilyNames() }
    }

    fn GetFirstMatchingFont(
        &self,
        weight: DWRITE_FONT_WEIGHT,
        stretch: DWRITE_FONT_STRETCH,
        style: DWRITE_FONT_STYLE,
    ) -> Result<IDWriteFont> {
        unsafe {
            Ok(wrap_font(
                self.inner.GetFirstMatchingFont(weight, stretch, style)?,
                self.inner.clone(),
            ))
        }
    }

    fn GetMatchingFonts(
        &self,
        weight: DWRITE_FONT_WEIGHT,
        stretch: DWRITE_FONT_STRETCH,
        style: DWRITE_FONT_STYLE,
    ) -> Result<IDWriteFontList> {
        let fonts = unsafe { self.inner.GetMatchingFonts(weight, stretch, style)? };
        let family: IDWriteFontFamily = AliasFamily {
            inner: self.inner.clone(),
            fonts,
        }
        .into();
        family.cast()
    }
}

fn wrap_font(inner: IDWriteFont, family: IDWriteFontFamily) -> IDWriteFont {
    AliasFont { inner, family }.into()
}

#[implement(IDWriteFont)]
struct AliasFont {
    inner: IDWriteFont,
    family: IDWriteFontFamily,
}

impl IDWriteFont_Impl for AliasFont_Impl {
    fn GetFontFamily(&self) -> Result<IDWriteFontFamily> {
        wrap_family(self.family.clone())
    }

    fn GetWeight(&self) -> DWRITE_FONT_WEIGHT {
        unsafe { self.inner.GetWeight() }
    }

    fn GetStretch(&self) -> DWRITE_FONT_STRETCH {
        unsafe { self.inner.GetStretch() }
    }

    fn GetStyle(&self) -> DWRITE_FONT_STYLE {
        unsafe { self.inner.GetStyle() }
    }

    fn IsSymbolFont(&self) -> BOOL {
        unsafe { self.inner.IsSymbolFont() }
    }

    fn GetFaceNames(&self) -> Result<IDWriteLocalizedStrings> {
        unsafe { self.inner.GetFaceNames() }
    }

    fn GetInformationalStrings(
        &self,
        id: DWRITE_INFORMATIONAL_STRING_ID,
        strings: OutRef<IDWriteLocalizedStrings>,
        exists: *mut BOOL,
    ) -> Result<()> {
        unsafe {
            // AviUtl2 uses these names rather than the collection's family names.
            if id == DWRITE_INFORMATIONAL_STRING_WIN32_FAMILY_NAMES
                || id == DWRITE_INFORMATIONAL_STRING_TYPOGRAPHIC_FAMILY_NAMES
            {
                strings.write(Some(self.family.GetFamilyNames()?))?;
                *exists = true.into();
                return Ok(());
            }
            let mut original = None;
            self.inner
                .GetInformationalStrings(id, &mut original, exists)?;
            strings.write(original)
        }
    }

    fn GetSimulations(&self) -> DWRITE_FONT_SIMULATIONS {
        unsafe { self.inner.GetSimulations() }
    }

    fn GetMetrics(&self, metrics: *mut DWRITE_FONT_METRICS) {
        unsafe { self.inner.GetMetrics(metrics) }
    }

    fn HasCharacter(&self, character: u32) -> Result<BOOL> {
        unsafe { self.inner.HasCharacter(character) }
    }

    fn CreateFontFace(&self) -> Result<IDWriteFontFace> {
        unsafe { self.inner.CreateFontFace() }
    }
}
