mod alias_collection;
mod fonts;
mod gui;
mod settings;

use aviutl2::{AnyResult, generic::GenericPlugin};
use std::{path::PathBuf, sync::Arc};
use windows::Win32::Graphics::DirectWrite::IDWriteFontCollection;

#[aviutl2::plugin(GenericPlugin)]
struct FontAliasPlugin {
    catalog: Arc<fonts::Catalog>,
    path: PathBuf,
    startup: Vec<settings::Alias>,
    warnings: Vec<String>,
    collection: Option<IDWriteFontCollection>,
}

impl GenericPlugin for FontAliasPlugin {
    fn new(_info: aviutl2::AviUtl2Info) -> AnyResult<Self> {
        aviutl2::tracing_subscriber::fmt()
            .event_format(aviutl2::logger::AviUtl2Formatter)
            .with_writer(aviutl2::logger::AviUtl2LogWriter)
            .try_init()
            .map_err(|error| anyhow::anyhow!("ログを初期化できません: {error}"))?;
        let data = aviutl2::config::app_data_path();
        let path = process_path::get_dylib_path()
            .unwrap()
            .with_file_name("font_alias.json")
            .to_owned();
        let mut warnings = Vec::new();
        let catalog = Arc::new(fonts::Catalog::load(&data.join("Font"), &mut warnings)?);
        let (aliases, load_error) = match settings::load(&path) {
            Ok(aliases) => (aliases, None),
            Err(error) => (Vec::new(), Some(format!("{error:#}"))),
        };
        aviutl2::tracing::info!(
            "設定を読み込みました: {} ({}件)",
            path.display(),
            aliases.len()
        );
        let collection = if load_error.is_none() {
            match settings::validate(&aliases, &catalog.existing_names)
                .and_then(|()| catalog.build_aliases(&aliases, &mut warnings))
            {
                Ok(collection) => collection,
                Err(error) => {
                    warnings.push(format!("エイリアスを登録できません: {error:#}"));
                    None
                }
            }
        } else {
            None
        };
        for warning in &warnings {
            aviutl2::tracing::warn!("{warning}");
        }
        if let Some(error) = &load_error {
            aviutl2::tracing::error!("{error}");
        }
        Ok(Self {
            catalog,
            path,
            startup: aliases,
            warnings,
            collection,
        })
    }

    fn plugin_info(&self) -> aviutl2::generic::GenericPluginTable {
        aviutl2::generic::GenericPluginTable {
            name: "font_alias.aux2".into(),
            information: format!(
                "Font Alias Plugin / v{} / https://github.com/sevenc-nanashi/font_alias.aux2",
                env!("CARGO_PKG_VERSION")
            ),
        }
    }

    fn register(&mut self, host: &mut aviutl2::generic::HostAppHandle) {
        if let Some(collection) = &self.collection {
            aviutl2::tracing::info!("エイリアスを登録します: {}ファミリー", unsafe {
                collection.GetFontFamilyCount()
            });
            host.register_font_collection(collection);
        }
        host.register_menus::<Self>();
    }
}

#[aviutl2::generic::menus]
impl FontAliasPlugin {
    #[config(name = "font_alias.aux2", error = "log")]
    fn configure(&self, window: aviutl2::generic::Win32WindowHandle) -> AnyResult<()> {
        use aviutl2::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use aviutl2_eframe::{eframe, egui};
        use windows::Win32::{
            Foundation::{GetLastError, HWND, SetLastError, WIN32_ERROR},
            UI::WindowsAndMessaging::{GWLP_HWNDPARENT, SetWindowLongPtrW},
        };

        let (aliases, load_error) = match settings::load(&self.path) {
            Ok(aliases) => (aliases, None),
            Err(error) => (Vec::new(), Some(format!("{error:#}"))),
        };
        let app = gui::App {
            catalog: Arc::clone(&self.catalog),
            path: self.path.clone(),
            saved: aliases.clone(),
            startup: self.startup.clone(),
            aliases,
            warnings: self.warnings.clone(),
            load_error,
            message: None,
            search: String::new(),
            selected_family: None,
            selected_alias: None,
            name: String::new(),
        };
        eframe::run_native(
            "font_alias.aux2",
            eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([640.0, 720.0])
                    .with_icon(egui::IconData::default()),
                persist_window: false,
                ..Default::default()
            },
            Box::new(move |cc| {
                let RawWindowHandle::Win32(handle) = cc.window_handle()?.as_raw() else {
                    return Err(
                        anyhow::anyhow!("設定ウィンドウがWin32ウィンドウではありません").into(),
                    );
                };
                // トップレベルウィンドウには親ではなくオーナーを設定する。
                unsafe {
                    SetLastError(WIN32_ERROR(0));
                    let previous = SetWindowLongPtrW(
                        HWND(handle.hwnd.get() as *mut std::ffi::c_void),
                        GWLP_HWNDPARENT,
                        window.hwnd.get(),
                    );
                    if previous == 0 && GetLastError() != WIN32_ERROR(0) {
                        return Err(windows::core::Error::from_thread().into());
                    }
                }
                cc.egui_ctx.set_fonts(aviutl2_eframe::aviutl2_fonts());
                cc.egui_ctx
                    .all_styles_mut(|style| style.visuals = aviutl2_eframe::aviutl2_visuals());
                Ok(Box::new(app))
            }),
        )
        .map_err(|error| anyhow::anyhow!("設定ウィンドウを開けません: {error}"))
    }
}

aviutl2::register_generic_plugin!(FontAliasPlugin);
