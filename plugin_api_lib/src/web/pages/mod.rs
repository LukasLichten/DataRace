use std::str::FromStr;

use maud::{html, Markup, PreEscaped, DOCTYPE};


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
    let pages = [("./", "Home"),("./dashboard","Dashboards"),("./setting","Settings")];

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

pub(super) async fn index() -> Markup {
    use crate::built_info::*;
    let cont = html!{
        h1 { "DataRace" }
        p {
            "Version: " (PKG_VERSION_MAJOR) "." (PKG_VERSION_MINOR) "." (PKG_VERSION_PATCH)
            br;
            "Apiversion: " (crate::API_VERSION) " - " (CFG_OS)
            br;
            br;
            a href=(PKG_REPOSITORY) { "GitHub" }
            br;
            (PKG_LICENSE)
            // ({  let mut out = html!();
            //     for i in 0..256 {
            //         out = html!{ (out) (i) br; };
            //     }
            //     out
            // })
        }
    };
    generate_page(cont, 0).await
}
