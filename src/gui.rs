use crate::{
    fonts::Catalog,
    settings::{self, Alias},
};
use aviutl2_eframe::{eframe, egui};
use std::{path::PathBuf, sync::Arc};

pub struct App {
    pub catalog: Arc<Catalog>,
    pub path: PathBuf,
    pub aliases: Vec<Alias>,
    pub saved: Vec<Alias>,
    pub startup: Vec<Alias>,
    pub warnings: Vec<String>,
    pub load_error: Option<String>,
    pub message: Option<String>,
    pub search: String,
    pub selected_family: Option<usize>,
    pub selected_alias: Option<usize>,
    pub name: String,
}

impl App {
    fn save(&mut self) {
        let result = settings::validate(&self.aliases, &self.catalog.existing_names)
            .and_then(|()| settings::save(&self.path, &self.aliases));
        match result {
            Ok(()) => {
                self.saved = self.aliases.clone();
                self.message = Some("保存しました。変更はAviUtl2の再起動後に反映されます。".into());
            }
            Err(error) => {
                aviutl2::tracing::error!("{error:#}");
                self.message = Some(format!("{error:#}"));
            }
        }
    }

    fn edit(&mut self, index: usize) {
        let alias = &self.aliases[index];
        self.name.clone_from(&alias.name);
        self.selected_family = self
            .catalog
            .families
            .iter()
            .position(|family| family.key == alias.target);
        self.selected_alias = Some(index);
        self.search.clear();
    }

    fn apply_entry(&mut self) {
        let Some(family) = self.selected_family else {
            return;
        };
        let entry = Alias {
            name: self.name.trim().to_owned(),
            target: self.catalog.families[family].key.clone(),
        };
        let mut next = self.aliases.clone();
        if let Some(index) = self.selected_alias {
            next[index] = entry;
        } else {
            next.push(entry);
        }
        match settings::validate(&next, &self.catalog.existing_names) {
            Ok(()) => {
                self.aliases = next;
                self.selected_alias = None;
                self.name.clear();
                self.message = None;
            }
            Err(error) => self.message = Some(format!("{error:#}")),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
          egui::ScrollArea::vertical().auto_shrink([false, true]).id_salt("content").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("font_alias.aux2");
                if ui.add_enabled(self.load_error.is_none(), egui::Button::new("保存")).clicked() {
                    self.save();
                }
                if self.aliases != self.saved {
                    ui.label("未保存の変更があります");
                }
                if self.saved != self.startup {
                    ui.label("反映には再起動が必要です");
                }
            });
            ui.label("ファミリーの別名を設定します。追加・変更・削除は保存し、AviUtl2を再起動すると反映されます。");
            if let Some(error) = &self.load_error {
                ui.colored_label(ui.visuals().error_fg_color, error);
                ui.label("設定ファイルを修復してから再読込してください。元のファイルを保護するため保存できません。");
                if ui.button("設定を再読込").clicked() {
                    match settings::load(&self.path) {
                        Ok(aliases) => {
                            self.saved = aliases.clone();
                            self.aliases = aliases;
                            self.load_error = None;
                            self.selected_alias = None;
                            self.name.clear();
                        }
                        Err(error) => self.load_error = Some(format!("{error:#}")),
                    }
                }
            }
            if !self.warnings.is_empty() {
                egui::CollapsingHeader::new(format!("起動時の警告 ({})", self.warnings.len())).show(ui, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, true]).id_salt("warnings").max_height(100.0).show(ui, |ui| {
                        for warning in &self.warnings {
                            ui.colored_label(ui.visuals().warn_fg_color, warning);
                        }
                    });
                });
            }
            if let Some(message) = &self.message {
                ui.label(message);
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label("エイリアス名");
                ui.text_edit_singleline(&mut self.name);
                let label = if self.selected_alias.is_some() { "変更" } else { "追加" };
                if ui.add_enabled(self.selected_family.is_some() && self.load_error.is_none(), egui::Button::new(label)).clicked() {
                    self.apply_entry();
                }
                if self.selected_alias.is_some() && ui.button("編集をキャンセル").clicked() {
                    self.selected_alias = None;
                    self.name.clear();
                }
            });
            if let Some(index) = self.selected_family {
                ui.label(format!("選択中: {}", self.catalog.families[index].label()));
            } else {
                ui.label("元のフォントファミリーを選択してください");
            }
            ui.horizontal_wrapped(|ui| {
                ui.label("フォント検索");
                ui.text_edit_singleline(&mut self.search);
            });
            let query = self.search.to_lowercase();
            let matching: Vec<_> = self.catalog.families.iter().enumerate().filter(|(_, family)| {
                family.names.iter().any(|name| name.to_lowercase().contains(&query))
            }).collect();
            egui::ScrollArea::vertical().auto_shrink([false, true]).id_salt("families").max_height(180.0).show_rows(
                ui, ui.text_style_height(&egui::TextStyle::Body), matching.len(), |ui, range| {
                    for row in range {
                        let (index, family) = matching[row];
                        if ui.selectable_label(self.selected_family == Some(index), family.label()).clicked() {
                            self.selected_family = Some(index);
                        }
                    }
                },
            );
            ui.separator();
            ui.label(format!("設定済みのエイリアス ({})", self.aliases.len()));
            let mut edit = None;
            let mut delete = None;
            egui::ScrollArea::vertical().auto_shrink([false, true]).id_salt("aliases").max_height(240.0).show(ui, |ui| {
                for (index, alias) in self.aliases.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        if ui.selectable_label(self.selected_alias == Some(index), &alias.name).clicked() {
                            edit = Some(index);
                        }
                        match self.catalog.find(&alias.target) {
                            Some(family) => { ui.label(format!("→ {}", family.label())); }
                            None => { ui.colored_label(ui.visuals().warn_fg_color, format!("→ {} ({}) [見つかりません]", alias.target.family, alias.target.source.label())); }
                        }
                        if ui.add_enabled(self.load_error.is_none(), egui::Button::new("削除")).clicked() {
                            delete = Some(index);
                        }
                    });
                }
            });
            if let Some(index) = edit { self.edit(index); }
            if let Some(index) = delete {
                self.aliases.remove(index);
                self.selected_alias = None;
                self.name.clear();
            }
          });
        });
    }
}
