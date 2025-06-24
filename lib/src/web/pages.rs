use std::str::FromStr;

use axum::{extract::{Path, State}, response::{IntoResponse, Response}};
use datarace_socket_spec::dashboard::{DashElement, Dashboard, Property};
use log::error;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use tokio::fs::{self, DirEntry};

use crate::utils::ValueCache;
use datarace_socket_spec::socket::Value;

use super::{dashboard::WebDashboard, utils::DataStoreLocked, FsResourceError};


#[allow(dead_code)]
pub(super) async fn serve_asset(file: &str) -> PreEscaped<String> {
    let mut path = std::path::PathBuf::from_str("./lib/assets").unwrap();
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
            @if let Some(version) = GIT_VERSION {
                " - " (version)
            }
            br;
            "Build Date: " (BUILT_TIME_UTC)
            br;
            "Api Version: " (crate::API_VERSION)
            br;
            "Enviroment: " (CFG_OS) " - " (CFG_ENV)
            br;
            br;
            "Plugins Loaded: " (plugin_count)
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


pub(super) async fn dashboard_list(State(datastore): State<DataStoreLocked>) -> Result<Markup, Response> {
    async fn parse_dir_entry(item: DirEntry) -> Option<(String, Dashboard)> {
        let path = item.path();

        let name = path.file_stem()?.to_str()?.to_string();

        if let Ok(dash) = super::read_dashboard_from_path(path).await {
            Some((name, dash))
        } else {
            None
        }
    }

    let folder = super::get_dashboard_folder(datastore).await.map_err(|e| e.into_response("list of all Dashboards".to_string()))?;

    let mut iter = match fs::read_dir(folder.as_path()).await {
        Ok(iter) => iter,
        Err(e) => {
            error!("Unable to read content of the Dashboards folder: {}", e);
            return Err(super::FsResourceError::from(e).into_response("list of all Dashboards".to_string()));
        }
    };

    let cont = html! {
        h1 { "Dashboards" }

        ul class="dashboard-list" {
            @while let Ok(Some(item)) = iter.next_entry().await {
                @if let Some((path, dash)) = parse_dir_entry(item).await {
                    li {
                        div class="dashboard-entry" {
                            h3 { (dash.name) }
                            div {
                                a class="button" target="_blank" href=(format!("./dashboard/render/{}", path)) { "Open" }
                                a class="button" target="_blank" href=(format!("./dashboard/edit/{}", path)) { "Edit" }
                            }
                        }
                    }
                }
            }
        }
    };
    Ok(generate_page(cont, 1).await)
}

/// Microseconds per second: 1s = 1000ms, 1ms = 1000us
const US_PER_SEC: f64 = 1000.0 * 1000.0;

pub(super) async fn properties(State(datastore): State<DataStoreLocked>) -> Markup {
    let property_list = {
        let ds_r = datastore.read().await;
        let mut list = vec![];

        for key in ds_r.iter_properties() {
            if let (Some(name),Some(cont)) = (ds_r.read_property_name(key),ds_r.get_property_container(key)) {
                let mut cache = ValueCache::default();
                cont.read_web(&mut cache);
                let ouput = match cache.value {
                    Value::None => "None".to_string(),
                    Value::Int(i) => format!("Int: {}", i),
                    Value::Float(f) => format!("Float: {}", f),
                    Value::Dur(d) => format!("Duration: {}s", (d as f64) / US_PER_SEC ),
                    Value::Bool(b) => format!("Boolean: {}", b),
                    Value::Str(s) => format!("Str: {}", s),
                    Value::Arr(arr) => {
                        let mut arr_str = format!("Arr: [");
                        for item in arr {
                            arr_str = format!{"{}{}, ", arr_str,
                                match item {
                                    Value::Int(i) => i.to_string(),
                                    Value::Str(s) => s,
                                    Value::Bool(b) => b.to_string(),
                                    Value::Dur(d) => format!("{}s", (d as f64) / US_PER_SEC),
                                    Value::Float(f) => f.to_string(),
                                    _ => String::new()
                                }
                            }
                        }

                        if let Some(stripped) = arr_str.strip_suffix(", ") {
                            arr_str = format!("{}]",stripped);
                        } else {
                            arr_str = format!("{}]", arr_str);
                        }

                        arr_str
                    },
                    Value::ArrUpdate(_) => String::new()
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

pub(super) async fn load_dashboard(Path(path): Path<String>, State(datastore): State<DataStoreLocked>) -> Response {
    match super::get_dashboard(datastore, path.clone()).await {
        Ok(dash) =>  {
            let ds_r = datastore.read().await;
            let secret = ds_r.dashboard_hasher_secret.clone();
            drop(ds_r);

            html!{ 
                (dash.generate_dashboard(secret)) 
            }.into_response()
        },
        Err(e) => e.into_response(path)
    }
}

pub(super) async fn edit_dashboard(Path(path): Path<String>, State(datastore): State<DataStoreLocked>) -> Result<Markup, Response> {
    let mut folder = super::get_dashboard_folder(datastore).await.map_err(|e| e.into_response(path.clone()))?;

    let test_dash = Dashboard {
        size_x: 1000,
        size_y: 750,
        name: path.clone(),
        elements: vec![
            DashElement {
                name: "tester_3".to_string(),
                x: Property::Fixed(150),
                y: Property::Fixed(200),
                size_x: Property::Fixed(500),
                size_y: Property::Fixed(400),
                visible: Property::Fixed(true),
                element: datarace_socket_spec::dashboard::DashElementType::Square("red".to_string()) 
            }],
        font_size: 12
    };

    folder.push(path.as_str());
    folder.set_extension("json");


    let json = match serde_json::to_string_pretty(&test_dash) {
        Ok(val) => val,
        Err(e) => {
            error!("Unable to parse Dashboard {} to a json: {}", path, e);
            return Err(FsResourceError::from(e).into_response(path));
        }
    };

    if let Err(e) = fs::write(folder.as_path(), json.as_bytes()).await {
        error!("Unable to save Dashboard {}: {}", path, e);
        return Err(FsResourceError::from(e).into_response(path));
    }

    Ok(html!{
        "Created template dashboard under name " (path)
    })
}
