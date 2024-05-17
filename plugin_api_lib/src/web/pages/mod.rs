use std::{path::PathBuf, str::FromStr};

use axum::{extract::{Path, State}, http::StatusCode};
use log::{error, info};
use maud::{html, Markup, PreEscaped, DOCTYPE};
use tokio::fs::{self, DirEntry};

use crate::utils::Value;

use super::utils::DataStoreLocked;

mod dashboard;

pub(super) async fn serve_asset(file: &str) -> PreEscaped<String> {
    let mut path = std::path::PathBuf::from_str("./plugin_api_lib/assets").unwrap();
    path.push(file);

    let val = std::fs::read_to_string(path.as_path()).unwrap();
    PreEscaped(val)
}

fn header(name: &str) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="utf-8";
        title { "DataRace - " (name) }
        link rel="stylesheet" href="style.css";
    }
} 

async fn generate_page(content: Markup, item: usize) -> Markup {
    let pages = [("./", "Home"),("./dashboard","Dashboards"),("./properties", "Properties"),("./setting","Settings")];

    html! {
        (header(pages[item].1))
        nav {
            input type="checkbox" id="check";
            label for="check" class="mobile-nav-check-btn" {
                i class="mobile-nav-check-icon" { "☰" }
            }
            div class="mobile-nav-bar" {
                ul {
                    @for ((link, page),index) in pages.iter().zip(0..pages.len()) {
                        @if index == item {
                            li { a class="mobile-nav-item item-current" { (page) }}
                        } @else {
                            li { a class="mobile-nav-item" href=(link) { (page) } }
                        }
                    }
                }
            }

            a class="nav-bar-title" { "DataRace" }
        }
        div class="page-wrapper" {
            div class="nav-menu" {
                ul class="nav-menu-list" {
                    @for ((link, page),index) in pages.iter().zip(0..pages.len()) {
                        @if index == item {
                            li { a class="nav-menu-item item-current" { (page) }}
                        } @else {
                            li { a class="nav-menu-item" href=(link) { (page) } }
                        }
                    }
                }
            }
            div class="content" {
                (content)
            }
        }
    }
}

pub(super) async fn index(State(datastore): State<DataStoreLocked>) -> Markup {
    let (plugin_count,properties_count) = {
        let ds_r = datastore.read().await;
        (ds_r.count_plugins(),ds_r.count_properties())
    };

    use crate::built_info::*;
    let cont = html!{
        h1 { "DataRace" }
        p {
            "Version: " (PKG_VERSION_MAJOR) "." (PKG_VERSION_MINOR) "." (PKG_VERSION_PATCH)
            br;
            "Apiversion: " (crate::API_VERSION) " - " (CFG_OS)
            br;
            "Plugins Loaded: " (plugin_count)
            br;
            br;
            "Properties: " (properties_count)
            br;
            br;
            a href=(PKG_REPOSITORY) { "GitHub" }
            br;
            (PKG_LICENSE)
        }
    };
    generate_page(cont, 0).await
}

async fn get_dashboard_folder(datastore: DataStoreLocked) -> Result<PathBuf, StatusCode> {
    let ds_r = datastore.read().await;
    let folder = ds_r.get_config().get_dashboards_folder();
    // We keep lock over datarace to prevent a race condition with folder creation

    if !folder.exists() {
        // Creating folder
        info!("Dashboards folder did not exist, creating...");
        if let Err(e) = std::fs::create_dir_all(folder.as_path()) {
            error!("Failed to create Dashboards Folder: {}", e.to_string());
        }
    }

    drop(ds_r);

    if folder.is_file() {
        // We are screwed
        error!("Unable to open dashboards folder because it is a file!");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(folder)
}

pub(super) async fn dashboard_list(State(datastore): State<DataStoreLocked>) -> Result<Markup, StatusCode> {
    fn parse_dir_entry(item: DirEntry) -> Option<String> {
        let path = item.path();

        let name = path.file_stem()?.to_str()?;
        Some(name.to_string())
    }

    let folder = get_dashboard_folder(datastore).await?;

    let mut iter = match fs::read_dir(folder.as_path()).await {
        Ok(iter) => iter,
        Err(e) => {
            error!("Unable to read content of the Dashboards folder: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let cont = html! {
        h1 { "Dashboards" }

        ul class="dashboard-list" {
            @while let Ok(Some(item)) = iter.next_entry().await {
                @if let Some(cont) = parse_dir_entry(item) {
                    li {
                        div class="dashboard-entry" {
                            h3 { (cont) }
                            div {
                                a class="button" target="_blank" href=(format!("./dashboard/render/{}", cont)) { "Open" }
                                a class="button" target="_blank" href=(format!("./dashboard/edit/{}", cont)) { "Edit" }
                            }
                        }
                    }
                }
            }
        }
    };
    Ok(generate_page(cont, 1).await)
}

pub(super) async fn properties(State(datastore): State<DataStoreLocked>) -> Markup {
    let property_list = {
        let ds_r = datastore.read().await;
        let mut list = vec![];

        for key in ds_r.iter_properties() {
            if let (Some(name),Some(cont)) = (ds_r.read_property_name(key),ds_r.get_property_container(key)) {
                let value = cont.read_web();
                let ouput = match value {
                    Value::None => "None".to_string(),
                    Value::Int(i) => format!("Int: {}", i),
                    Value::Float(f) => format!("Float: {}", f),
                    Value::Dur(d) => format!("Duration: {}µs", d),
                    Value::Bool(b) => format!("Boolean: {}", b),
                    Value::Str(s) => format!("Str: {}", s)
                };
                list.push((name,ouput));
            }
        }

        list
    };

    let cont = html! {
        h1 { "Properties" }

        ul class="property-list" {
            @for (name, output) in property_list {
                li {
                    div class="property-entry" {
                        div { (name) }
                        div { (output) }
                    }
                }
            }
        }
    };
    generate_page(cont, 2).await
}

pub(super) async fn settings() -> Markup {
    let cont = html! {
        h1 style="font-style: italic;" { "Todo..." }
    };
    generate_page(cont, 3).await
}

pub(super) async fn load_dashboard(Path(path): Path<String>, State(datastore): State<DataStoreLocked>) -> Result<Markup, StatusCode> {
    let mut folder = get_dashboard_folder(datastore).await?;

    folder.push(path.as_str());
    folder.set_extension("json");

    if !folder.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let content = match fs::read(folder.as_path()).await {
        Ok(cont) => cont,
        Err(e) => {
            error!("Unable to open Dashboard {} file: {}", path, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    match serde_json::from_slice(content.as_slice()) {
        Ok(dash) => {
            let dash: dashboard::Dashboard = dash;
            Ok(html!{ (dash) })
        },
        Err(e) => {
            error!("Unable to parse Dashboard {} json file: {}", path, e);
            Err(StatusCode::IM_A_TEAPOT) // LOL
        }
    }
}

pub(super) async fn edit_dashboard(Path(path): Path<String>, State(datastore): State<DataStoreLocked>) -> Result<Markup, StatusCode> {
    let mut folder = get_dashboard_folder(datastore).await?;

    let test_dash = dashboard::Dashboard {
        size_x: 1000,
        size_y: 750,
        name: path.clone(),
        elements: vec![dashboard::DashElement { name: "tester_1".to_string(), x: 150, y: 200, size_x: 500, size_y: 500, element: dashboard::DashElementType::Square("red".to_string()) }]
    };

    folder.push(path.as_str());
    folder.set_extension("json");


    let json = match serde_json::to_string_pretty(&test_dash) {
        Ok(val) => val,
        Err(e) => {
            error!("Unable to parse Dashboard {} to a json: {}", path, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if let Err(e) = fs::write(folder.as_path(), json.as_bytes()).await {
        error!("Unable to save Dashboard {}: {}", path, e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(html!{
        "Created template dashboard under name " (path)
    })
}
