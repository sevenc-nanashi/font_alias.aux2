mod fonts;
mod gui;
mod settings;

use aviutl2::{AnyResult, generic::GenericPlugin};
use std::sync::Arc;
use windows::Win32::Graphics::DirectWrite::IDWriteFontCollection;

#[aviutl2::plugin(GenericPlugin)]
struct FontAliasPlugin {
    window: aviutl2_eframe::EframeWindow,
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
        let path = data.join("font_alias.json");
        let mut warnings = Vec::new();
        let catalog = Arc::new(fonts::Catalog::load(&data.join("Font"), &mut warnings)?);
        let (aliases, load_error) = match settings::load(&path) {
            Ok(aliases) => (aliases, None),
            Err(error) => (Vec::new(), Some(format!("{error:#}"))),
        };
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
        let window = aviutl2_eframe::EframeWindow::new("FontAlias", move |cc, handle| {
            cc.egui_ctx.set_fonts(aviutl2_eframe::aviutl2_fonts());
            cc.egui_ctx
                .all_styles_mut(|style| style.visuals = aviutl2_eframe::aviutl2_visuals());
            Ok(Box::new(gui::App {
                catalog,
                path,
                saved: aliases.clone(),
                startup: aliases.clone(),
                aliases,
                warnings,
                load_error,
                message: None,
                search: String::new(),
                selected_family: None,
                selected_alias: None,
                name: String::new(),
                handle,
            }))
        })?;
        Ok(Self { window, collection })
    }

    fn plugin_info(&self) -> aviutl2::generic::GenericPluginTable {
        aviutl2::generic::GenericPluginTable {
            name: "フォントエイリアス".into(),
            information: format!(
                "フォントエイリアス v{} / sevenc-nanashi",
                env!("CARGO_PKG_VERSION")
            ),
        }
    }

    fn register(&mut self, host: &mut aviutl2::generic::HostAppHandle) {
        if let Some(collection) = &self.collection {
            host.register_font_collection(collection);
        }
        match self.window.handle().and_then(|handle| {
            host.register_window_client("フォントエイリアス", &handle)?;
            Ok(())
        }) {
            Ok(()) => {}
            Err(error) => {
                let _ = aviutl2::logger::write_error_log(&format!(
                    "設定ウィンドウを登録できません: {error:#}"
                ));
            }
        }
    }
}

aviutl2::register_generic_plugin!(FontAliasPlugin);
