use crate::browser_discovery::get_browser_name_from_path;
use copypasta::{ClipboardContext, ClipboardProvider};
use log::{error, warn};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::collections::HashMap;

const BROWSER_CHOOSER_TITLE: &str = "Choose Browser";
const BROWSER_CHOOSER_PROMPT: &str = "Select a browser to open:";
const BROWSER_CHOOSER_REMEMBER_CHOICE: &str = "Remember this choice for this site";
const BROWSER_CHOOSER_CANCEL: &str = "Cancel";

slint::slint! {
    import { Button, CheckBox, HorizontalBox, ScrollView, VerticalBox } from "std-widgets.slint";

    export component BrowserChooserDialog inherits Window {
        in property <string> window_title;
        in property <string> prompt_text;
        in property <string> remember_choice_text;
        in property <string> cancel_text;
        in property <string> copy_url_text;
        in property <string> url;
        in property <[string]> browsers;
        in-out property <bool> remember_choice: false;
        callback browser_selected(int, bool);
        callback copy_url();
        callback cancel();

        width: 520px;
        preferred-height: 480px;
        title: root.window_title;

        VerticalBox {
            padding: 16px;
            spacing: 12px;

            Text {
                text: root.prompt_text;
                font-size: 18px;
                wrap: word-wrap;
            }

            Rectangle {
                height: 40px;
                clip: true;
                Text {
                    text: root.url;
                    color: #888888;
                    wrap: no-wrap;
                    overflow: elide;
                    width: parent.width;
                }
            }

            ScrollView {
                vertical-scrollbar-policy: as-needed;
                preferred-height: 220px;
                VerticalBox {
                    spacing: 8px;

                    for browser[i] in root.browsers : Button {
                        text: browser;
                        clicked => { root.browser_selected(i, root.remember_choice); }
                    }
                }
            }

            CheckBox {
                text: root.remember_choice_text;
                checked <=> root.remember_choice;
            }

            HorizontalBox {
                spacing: 8px;
                Button {
                    text: root.copy_url_text;
                    clicked => { root.copy_url(); }
                }
                Button {
                    text: root.cancel_text;
                    clicked => { root.cancel(); }
                }
            }
        }
    }
}

pub enum GuiChooserOutcome {
    Selected {
        browser_path: String,
        save_rule: bool,
    },
    Cancelled,
    Unavailable,
}

pub fn prompt_browser_selection_slint(url: &str, browsers: &[String]) -> GuiChooserOutcome {
    let dialog = match BrowserChooserDialog::new() {
        Ok(dialog) => dialog,
        Err(e) => {
            warn!("Slint chooser could not be started: {}", e);
            return GuiChooserOutcome::Unavailable;
        }
    };

    dialog.set_window_title(SharedString::from(BROWSER_CHOOSER_TITLE));
    dialog.set_prompt_text(SharedString::from(BROWSER_CHOOSER_PROMPT));
    dialog.set_remember_choice_text(SharedString::from(BROWSER_CHOOSER_REMEMBER_CHOICE));
    dialog.set_cancel_text(SharedString::from(BROWSER_CHOOSER_CANCEL));
    dialog.set_copy_url_text(SharedString::from("Copy URL"));

    let browser_names: Vec<SharedString> = browser_display_names(browsers)
        .into_iter()
        .map(SharedString::from)
        .collect();

    dialog.set_url(SharedString::from(url));
    dialog.set_browsers(ModelRc::new(VecModel::from(browser_names)));

    let browser_paths = browsers.to_vec();
    let result = std::rc::Rc::new(std::cell::RefCell::new(None::<GuiChooserOutcome>));

    {
        let result = std::rc::Rc::clone(&result);
        let weak = dialog.as_weak();
        let browser_paths = browser_paths.clone();

        dialog.on_browser_selected(move |index, save_rule| {
            let browser_index = index as usize;
            let browser_path = match browser_paths.get(browser_index) {
                Some(path) => path.clone(),
                None => {
                    error!("Invalid browser index selected: {}", index);
                    if let Some(dialog) = weak.upgrade() {
                        let _ = dialog.hide();
                    }
                    return;
                }
            };

            *result.borrow_mut() = Some(GuiChooserOutcome::Selected {
                browser_path,
                save_rule,
            });

            if let Some(dialog) = weak.upgrade() {
                let _ = dialog.hide();
            }
        });
    }

    {
        let url = url.to_string();
        dialog.on_copy_url(move || match ClipboardContext::new() {
            Ok(mut clipboard) => {
                if let Err(e) = clipboard.set_contents(url.clone()) {
                    warn!("Failed to copy URL to clipboard: {}", e);
                }
            }
            Err(e) => {
                warn!("Failed to access clipboard: {}", e);
            }
        });
    }

    {
        let result = std::rc::Rc::clone(&result);
        let weak = dialog.as_weak();

        dialog.on_cancel(move || {
            *result.borrow_mut() = Some(GuiChooserOutcome::Cancelled);

            if let Some(dialog) = weak.upgrade() {
                let _ = dialog.hide();
            }
        });
    }

    if let Err(e) = dialog.run() {
        warn!("Slint chooser failed to run: {}", e);
        return GuiChooserOutcome::Unavailable;
    }

    result
        .borrow_mut()
        .take()
        .unwrap_or(GuiChooserOutcome::Cancelled)
}

fn browser_display_names(browsers: &[String]) -> Vec<String> {
    let derived_names: Vec<String> = browsers
        .iter()
        .map(|browser_path| get_browser_name_from_path(browser_path))
        .collect();

    let mut counts: HashMap<String, usize> = HashMap::new();
    for name in &derived_names {
        *counts.entry(name.clone()).or_insert(0) += 1;
    }

    browsers
        .iter()
        .zip(derived_names)
        .map(|(browser_path, browser_name)| {
            let browser_key = browser_name.clone();
            let mut display_name = if browser_path.starts_with("flatpak:") {
                format!("{} (Flatpak)", browser_name)
            } else {
                browser_name
            };

            if counts.get(browser_key.as_str()).copied().unwrap_or(0) > 1 {
                display_name = format!("{} ({})", display_name, browser_path);
            }

            display_name
        })
        .collect()
}
